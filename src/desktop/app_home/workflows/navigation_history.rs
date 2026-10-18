//! Native conversation-navigation ingress for the retained Sonic IA reducer.
//! No synchronous storage work in selection/render paths. The browser store is
//! partitioned by the existing opaque account UUID, never by a Slack token.
use super::*;
mod read;
pub(in crate::app_home) use read::capture_for;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};
use super_platinum_core::block_actions::home::workflow::directory_navigation as nav;

#[derive(Default)]
pub(in crate::app_home) struct Store {
    // One reducer and serial writer per account for the lifetime of this Root.
    // A -> B -> A must rejoin A's pending writer rather than reload its key
    // under a second gate while an older A snapshot is still in flight.
    accounts: BTreeMap<String, Arc<Mutex<Shared>>>,
}
impl Store {
    fn for_account(&mut self, account: &str) -> Arc<Mutex<Shared>> {
        self.accounts
            .entry(account.to_owned())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Shared {
                    history: None,
                    queued: VecDeque::new(),
                    writes: VecDeque::new(),
                    gate: Arc::new(tokio::sync::Mutex::new(())),
                    error: None,
                    revision: 0,
                }))
            })
            .clone()
    }
}
struct Shared {
    history: Option<nav::History>,
    queued: VecDeque<nav::Entry>,
    writes: VecDeque<nav::Write>,
    gate: Arc<tokio::sync::Mutex<()>>,
    error: Option<String>,
    revision: u64,
}
#[derive(Clone)]
pub(in crate::app_home) struct Witness {
    account: String,
    shared: Arc<Mutex<Shared>>,
    revision: u64,
}
impl Witness {
    pub(in crate::app_home) fn current(&self, s: &ShellState) -> bool {
        identity(s).as_deref() == Some(&self.account)
            && s.app_home
                .workflow_navigation
                .as_ref()
                .and_then(|store| store.accounts.get(&self.account))
                .is_some_and(|current| Arc::ptr_eq(current, &self.shared))
            && self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .revision
                == self.revision
    }
}
fn identity(s: &ShellState) -> Option<String> {
    #[cfg(feature = "protected-test-support")]
    if let Some(owner) = fixture::navigation_owner(s) {
        return Some(owner);
    }
    s.core
        .active_account
        .as_ref()
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .cloned()
}
fn ensure(s: &mut ShellState) -> Option<Arc<Mutex<Shared>>> {
    let account = identity(s)?;
    Some(
        s.app_home
            .workflow_navigation
            .get_or_insert_with(Store::default)
            .for_account(&account),
    )
}
/// Carry only private navigation history/writer custody into a freshly loaded
/// shell. Workflow dialogs, permissions, query leases and account authority stay
/// on the new Root. Old captured writers and a returning account share one gate.
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    loaded.app_home.workflow_navigation = previous.app_home.workflow_navigation.take();
}
/// Called only after select_channel has installed the exact native conversation.
/// This native event mapping is explicit; it does not read palette frecency.
pub(crate) fn record(s: &mut ShellState, channel: &str) {
    let Some(team) = s.core.active_team.clone() else {
        return;
    };
    if s.core.active_channel.as_deref() != Some(channel) {
        return;
    }
    let Some(shared) = ensure(s) else { return };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let entry = nav::Entry::new(
        serde_json::json!({"primaryView":channel,"teamId":team,"ts":now.as_millis()}),
    );
    let mut inner = shared.lock().unwrap_or_else(|e| e.into_inner());
    inner.revision = inner.revision.wrapping_add(1);
    if let Some(history) = &mut inner.history {
        let write = history.push(Some(entry), &[]);
        inner.writes.push_back(write);
    } else {
        inner.queued.push_back(entry);
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    key: String,
    kind: String,
    value: Option<Value>,
}
async fn storage(key: &str, write: Option<Value>) -> Result<Reply, Error> {
    let request = serde_json::json!({"key":key,"write":write});
    let script = format!(
        "{}\ndioxus.send(workflowNavigationStorage({}));await dioxus.recv();",
        include_str!("navigation_history.js"),
        request
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Reply>()).await;
    let _ = eval.send(());
    let reply = result
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?;
    if reply.key != key {
        return Err(unavailable());
    }
    Ok(reply)
}
/// Single serial writer per retained store. Late completion can only touch the
/// captured account namespace and captured Arc, never a replacement Root.
async fn load_and_flush(account: &str, shared: &Arc<Mutex<Shared>>) -> Result<(), Error> {
    let gate = shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .gate
        .clone();
    let _serial = gate.lock().await;
    let result = drive(account, shared).await;
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    if result.is_err() {
        s.error = Some("native navigation storage unavailable".into());
    }
    result
}
async fn drive(account: &str, shared: &Arc<Mutex<Shared>>) -> Result<(), Error> {
    let key = format!("super-platinum/{account}/client-ia-history");
    let cold = shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .history
        .is_none();
    if cold {
        #[cfg(feature = "protected-test-support")]
        let fixture = account.starts_with("fixture-navigation-");
        #[cfg(not(feature = "protected-test-support"))]
        let fixture = false;
        let reply = if fixture {
            Reply {
                key: key.clone(),
                kind: "missing".into(),
                value: None,
            }
        } else {
            storage(&key, None).await?
        };
        let read = match reply.kind.as_str() {
            "missing" => nav::StorageRead::Missing,
            "read-failed" => nav::StorageRead::ReadFailed,
            "invalid-json" => nav::StorageRead::InvalidJson,
            "parsed" => nav::StorageRead::Parsed(reply.value.ok_or_else(unavailable)?),
            _ => return Err(unavailable()),
        };
        let history = nav::History::initialize(read).map_err(|_| unavailable())?;
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.history = Some(history);
        while let Some(entry) = s.queued.pop_front() {
            let write = s.history.as_mut().unwrap().push(Some(entry), &[]);
            s.writes.push_back(write);
        }
    }
    loop {
        let write = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .writes
            .pop_front();
        let Some(write) = write else { return Ok(()) };
        let raw = match write {
            nav::Write::None => continue,
            nav::Write::Immediate(v) => v,
            nav::Write::Deferred(v) => v.snapshot_when_run(),
        };
        // A storage error retains actual in-memory history; source persistence
        // failure does not roll the reducer back or manufacture a new read.
        #[cfg(feature = "protected-test-support")]
        if account.starts_with("fixture-navigation-") {
            continue;
        }
        if storage(&key, Some(raw)).await?.kind != "written" {
            return Err(unavailable());
        }
    }
}
/// Called by the existing async channel refresh, and by the first owned picker.
pub(crate) fn flush(
    mut state: Signal<ShellState>,
) -> impl std::future::Future<Output = ()> + 'static {
    let captured = {
        let mut s = state.write();
        ensure(&mut s).and_then(|shared| identity(&s).map(|a| (a, shared)))
    };
    // Capture before returning the future. Replacement between spawn and its
    // first poll must not redirect the old store's pending writes.
    async move {
        if let Some((account, shared)) = captured {
            let _ = load_and_flush(&account, &shared).await;
        }
    }
}
pub(super) async fn capture(
    mut state: Signal<ShellState>,
    query: &entity_search::Query,
) -> Result<(Value, Witness), Error> {
    let (account, shared) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        let shared = ensure(&mut s).ok_or_else(unavailable)?;
        (identity(&s).unwrap(), shared)
    };
    load_and_flush(&account, &shared).await?;
    let s = state.peek();
    if !query.current(&s) {
        return Err(unavailable());
    }
    let mut inner = shared.lock().unwrap_or_else(|e| e.into_inner());
    // This is the native client's conversation history. Multi-team entries are
    // retained. This native desktop product supports multiple workspaces; that
    // explicit capability is independent of auth flags or credential count.
    let projected = inner
        .history
        .as_mut()
        .ok_or_else(unavailable)?
        .project(true, Some(&query.owner().owner.ctx.session.team_id), true)
        .map_err(|_| unavailable())?;
    let witness = Witness {
        account,
        shared: shared.clone(),
        revision: inner.revision,
    };
    drop(inner);
    if !witness.current(&s) {
        return Err(unavailable());
    }
    Ok((nav::projection_json(&projected), witness))
}
fn unavailable() -> Error {
    Error::Api("home_workflow_navigation_history_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_account_rejoins_original_writer_and_keeps_pending_history() {
        let mut store = Store::default();
        let first = store.for_account("A");
        let gate = {
            let mut s = first.lock().unwrap();
            let mut history = nav::History::initialize(nav::StorageRead::Missing).unwrap();
            let write = history.push(
                Some(nav::Entry::new(
                    serde_json::json!({"primaryView":"C00000OLD","teamId":"T1","ts":1000}),
                )),
                &[],
            );
            s.history = Some(history);
            s.writes.push_back(write);
            s.revision = 1;
            s.gate.clone()
        };
        let original_writer = gate.try_lock().unwrap();
        let other = store.for_account("B");
        let returned = store.for_account("A");
        assert!(Arc::ptr_eq(&first, &returned));
        assert!(!Arc::ptr_eq(&first, &other));
        let returned_gate = returned.lock().unwrap().gate.clone();
        assert!(Arc::ptr_eq(&gate, &returned_gate));
        assert!(returned_gate.try_lock().is_err());
        {
            let mut s = returned.lock().unwrap();
            let write = s.history.as_mut().unwrap().push(
                Some(nav::Entry::new(
                    serde_json::json!({"primaryView":"C00000NEW","teamId":"T1","ts":2000}),
                )),
                &[],
            );
            s.writes.push_back(write);
            s.revision += 1;
            assert_eq!(s.writes.len(), 2);
            let projection = s
                .history
                .as_mut()
                .unwrap()
                .project(true, Some("T1"), true)
                .unwrap();
            assert!(projection.contains_key("C00000OLD") && projection.contains_key("C00000NEW"));
            assert_eq!(s.revision, 2);
        }
        drop(original_writer);
        assert!(returned_gate.try_lock().is_ok());
    }

    #[test]
    fn another_account_has_independent_history_and_gate() {
        let mut store = Store::default();
        let a = store.for_account("A");
        let b = store.for_account("B");
        let ag = a.lock().unwrap().gate.clone();
        let bg = b.lock().unwrap().gate.clone();
        let _held = ag.try_lock().unwrap();
        assert!(bg.try_lock().is_ok());
        a.lock().unwrap().error = Some("retained A error".into());
        assert!(b.lock().unwrap().error.is_none());
        assert_eq!(
            store.for_account("A").lock().unwrap().error.as_deref(),
            Some("retained A error")
        );
    }
    #[test]
    fn actual_shell_install_rejoins_old_account_writer_without_action_authority() {
        use std::{cell::Cell, rc::Rc};
        type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
        const A: &str = "00000000-0000-4000-8000-000000000001";
        const B: &str = "00000000-0000-4000-8000-000000000002";
        fn shell(account: &str) -> ShellState {
            let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
            s.core.active_account = Some(account.into());
            s.app_home = Default::default();
            s
        }
        let slot = Slot::default();
        let mut dom = VirtualDom::new_with_props(
            |slot: Slot| {
                let state = use_signal(|| shell(A));
                slot.set(Some(state));
                VNode::empty()
            },
            slot.clone(),
        );
        dom.rebuild_to_vec();
        let mut state = slot.get().unwrap();
        let original = {
            let mut s = state.write();
            s.core.active_channel = Some("C00000OLD".into());
            record(&mut s, "C00000OLD");
            s.app_home.workflow_names_revision = 99;
            s.app_home
                .fixture_forms
                .push(serde_json::json!({"old":"authority"}));
            ensure(&mut s).unwrap()
        };
        let gate = original.lock().unwrap().gate.clone();
        let old_writer = gate.try_lock().unwrap();
        let old_witness = Witness {
            account: A.into(),
            shared: original.clone(),
            revision: 1,
        };
        crate::bootstrap::install_loaded(state, shell(B));
        assert!(!old_witness.current(&state.peek()));
        assert!(state.peek().app_home.fixture_forms.is_empty());
        assert_eq!(state.peek().app_home.workflow_names_revision, 0);
        let b = ensure(&mut state.write()).unwrap();
        assert!(!Arc::ptr_eq(&b, &original));
        assert!(b.lock().unwrap().gate.try_lock().is_ok());
        crate::bootstrap::install_loaded(state, shell(A));
        let returned = ensure(&mut state.write()).unwrap();
        assert!(Arc::ptr_eq(&original, &returned));
        assert!(returned.lock().unwrap().gate.try_lock().is_err());
        assert!(old_witness.current(&state.peek()));
        {
            let mut s = state.write();
            s.core.active_channel = Some("C00000NEW".into());
            record(&mut s, "C00000NEW");
        }
        assert!(!old_witness.current(&state.peek()));
        let held = returned.lock().unwrap();
        assert_eq!(held.queued.len(), 2);
        assert_eq!(held.queued[0].raw()["primaryView"], "C00000OLD");
        assert_eq!(held.queued[1].raw()["primaryView"], "C00000NEW");
        drop(held);
        drop(old_writer);
        assert!(gate.try_lock().is_ok());
    }
}
