//! Component-independent local persistence. No network calls or automatic retry.
use super::*;
use super_platinum_core::drafts::sync::ConsumeKind;

#[path = "checkpoint_poll.rs"]
mod poll;
pub(crate) use poll::Signature;

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub serial: u64,
    pub view: View,
    pub pending: Option<LocalDraft>,
    pub error: Option<Fault>,
    /// Input has not become the authoritative paired local draft. A blocked
    /// alternate may nevertheless hold it durably for explicit recovery.
    pub unsaved: bool,
    pub running: bool,
}

impl Snapshot {
    fn from_state(state: &State) -> Self {
        Self {
            serial: state.serial,
            view: state.base.clone(),
            pending: state.pending.as_ref().map(|(_, local)| local.clone()),
            error: state.error,
            unsaved: state.pending.is_some(),
            running: state.running || state.settling,
        }
    }
}

struct State {
    base: View,
    pending: Option<(u64, LocalDraft)>,
    serial: u64,
    running: bool,
    settling: bool,
    return_handoff: Option<(u64, u64)>,
    error: Option<Fault>,
    direct_local: Option<super_platinum_core::drafts::sync::ConsumeIntent>,
    local_writer: super_platinum_core::drafts::store::LocalWriter,
    released: bool,
    released_tail: LocalDraft,
    local_writes: std::collections::VecDeque<(u64, LocalDraft, storage::LocalWrite)>,
}

pub(crate) struct Editor {
    owner: Owner,
    runtime: tokio::runtime::Handle,
    state: Mutex<State>,
    gate: tokio::sync::Mutex<()>,
    changed: tokio::sync::Notify,
}

impl Editor {
    fn new(owner: Owner, view: View) -> Result<Arc<Self>, Fault> {
        let stored = view.stored.as_ref().ok_or(Fault::Unmanaged)?;
        if stored.state.scope() != &owner.scope {
            return Err(Fault::Stale);
        }
        let error =
            (view.blocked.is_some() || view.uncommitted.is_some()).then_some(Fault::AlternateEdits);
        Ok(Arc::new(Self {
            owner,
            runtime: tokio::runtime::Handle::try_current().map_err(|_| Fault::Storage)?,
            state: Mutex::new(State {
                direct_local: local::direct_intent(stored),
                local_writer: Default::default(),
                released_tail: view.local.clone(),
                released: false,
                local_writes: Default::default(),
                base: view,
                pending: None,
                serial: 0,
                running: false,
                settling: false,
                return_handoff: None,
                error,
            }),
            gate: tokio::sync::Mutex::new(()),
            changed: tokio::sync::Notify::new(),
        }))
    }

    pub fn owner(&self) -> Owner {
        self.owner.clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        Snapshot::from_state(&state)
    }

    /// Enqueue and start immediately. The task owns this editor through account
    /// switches/component unmounts; no UI tick is needed to initiate a save.
    pub fn submit(self: &Arc<Self>, local: LocalDraft) {
        let start = {
            let mut state = self.state.lock().unwrap();
            state.serial = state
                .serial
                .checked_add(1)
                .expect("editor revision exhausted");
            state.pending = Some((state.serial, local.clone()));
            if state.released && !state.settling {
                self.enqueue_released(&mut state, local);
            }
            let start = !state.running;
            state.running = true;
            start
        };
        if start {
            let editor = self.clone();
            self.runtime.spawn(async move {
                editor.run().await;
            });
        }
    }

    async fn run(self: Arc<Self>) {
        let _guard = self.gate.lock().await;
        let released = self.state.lock().unwrap().released;
        if released {
            self.run_released().await;
            self.changed.notify_waiters();
            return;
        }
        loop {
            let next = {
                let mut state = self.state.lock().unwrap();
                match state.pending.clone() {
                    Some(next) => Some((next, state.error)),
                    None => {
                        state.running = false;
                        None
                    }
                }
            };
            let Some(((serial, local), parked)) = next else {
                break;
            };
            let result = if let Some(error) = parked {
                Err(error)
            } else {
                self.checkpoint(local.clone()).await
            };
            match result {
                Ok(view) => {
                    let mut state = self.state.lock().unwrap();
                    state.base = view;
                    if state.pending.as_ref().is_some_and(|(id, _)| *id == serial) {
                        state.pending = None;
                    }
                }
                Err(error) => {
                    self.state.lock().unwrap().error = Some(error);
                    // Always preserve the newest input, including input that
                    // arrived while the failed paired write was on disk.
                    loop {
                        let (serial, local) = self.state.lock().unwrap().pending.clone().unwrap();
                        let writer = self.state.lock().unwrap().local_writer.clone();
                        // Release/enrollment transitions share the owner gate.
                        // Do not let a legacy fallback run after ownership was
                        // checked but before the managed row is released.
                        let owner_guard = self.owner.gate.lock().await;
                        let saved = self
                            .owner
                            .account
                            .backend
                            .preserve_alternate(self.owner.scope.key.clone(), local, writer, serial)
                            .await;
                        let refreshed = if saved.is_ok() {
                            self.owner.current().await.ok()
                        } else {
                            None
                        };
                        drop(owner_guard);
                        let mut state = self.state.lock().unwrap();
                        if saved.is_err() {
                            state.error = Some(Fault::Storage);
                        }
                        if let Some(view) = refreshed {
                            state.base = view;
                        }
                        if state.pending.as_ref().is_some_and(|(id, _)| *id == serial) {
                            state.running = false;
                            break;
                        }
                    }
                    break;
                }
            }
        }
        self.changed.notify_waiters();
    }

    /// Only receipt/control changes with unchanged paired local content and
    /// local revision may advance this editor's storage revision.
    async fn rebased(&self) -> Result<View, Fault> {
        let current = self.owner.load().await?;
        let base = self.state.lock().unwrap().base.clone();
        let previous = base.stored.as_ref().ok_or(Fault::Unmanaged)?;
        let stored = current.stored.as_ref().ok_or(Fault::Unmanaged)?;
        current.ready(stored.revision)?;
        if stored.revision != previous.revision
            && (!same_local(&current.local, &base.local)
                || stored.state.local_revision() != previous.state.local_revision())
        {
            return Err(Fault::Stale);
        }
        Ok(current)
    }

    async fn checkpoint(&self, local: LocalDraft) -> Result<View, Fault> {
        for _ in 0..4 {
            let current = self.rebased().await?;
            let revision = current.stored.as_ref().unwrap().revision;
            if same_local(&current.local, &local) {
                return Ok(current);
            }
            let direct = self.state.lock().unwrap().direct_local.clone();
            let result = if let Some(intent) = direct {
                self.owner
                    .checkpoint_direct_local(revision, local.clone(), &intent)
                    .await
            } else {
                self.owner.checkpoint(revision, local.clone()).await
            };
            match result {
                Ok(stored) => {
                    return Ok(View {
                        local,
                        stored: Some(stored),
                        blocked: None,
                        uncommitted: None,
                    });
                }
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }

    pub async fn flush(&self) -> Result<View, Fault> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.state.lock().unwrap();
                if !state.running && !state.settling {
                    return state.error.map_or_else(|| Ok(state.base.clone()), Err);
                }
            }
            changed.await;
        }
    }

    pub async fn consume(
        self: &Arc<Self>,
        expected: LocalDraft,
        kind: ConsumeKind,
    ) -> Result<Stored, Fault> {
        // The durable clear must finish updating the editor's base even when
        // the foreground waiter is canceled during SQLite I/O.
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.consume_inner(expected, kind).await })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn consume_inner(
        &self,
        expected: LocalDraft,
        kind: ConsumeKind,
    ) -> Result<Stored, Fault> {
        loop {
            self.flush().await?;
            let guard = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(guard);
                continue;
            }
            let current = self.rebased().await?;
            if !same_local(&current.local, &expected) {
                return Err(Fault::Stale);
            }
            // A newly submitted edit is pending, not part of this frozen send.
            // It waits on this gate and commits against the paired clear below.
            let stored = self
                .owner
                .consume(current.stored.unwrap().revision, kind)
                .await?;
            let mut state = self.state.lock().unwrap();
            state.direct_local = local::direct_intent(&stored);
            state.base = View {
                local: LocalDraft::default(),
                stored: Some(stored.clone()),
                blocked: None,
                uncommitted: None,
            };
            return Ok(stored);
        }
    }
}

#[derive(Default)]
pub(crate) struct Queue {
    scope: Option<(String, u64)>,
    editors: BTreeMap<DraftKey, Arc<Editor>>,
}
impl Queue {
    pub fn remove(&mut self, account: &str, epoch: u64, key: &DraftKey) {
        if self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            self.editors.remove(key);
        }
    }

    pub fn snapshots(&self, account: &str, epoch: u64) -> Vec<(DraftKey, Snapshot)> {
        if !self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            return Vec::new();
        }
        self.editors
            .iter()
            .map(|(key, editor)| (key.clone(), editor.snapshot()))
            .collect()
    }

    pub fn get(&self, account: &str, epoch: u64, key: &DraftKey) -> Option<Arc<Editor>> {
        if self
            .scope
            .as_ref()
            .is_some_and(|(owner, visit)| owner == account && *visit == epoch)
        {
            self.editors.get(key).cloned()
        } else {
            None
        }
    }

    pub fn editor(
        &mut self,
        account: &Arc<Account>,
        epoch: u64,
        key: DraftKey,
        loaded: View,
    ) -> Result<Arc<Editor>, Fault> {
        let scope = (account.backend.account().to_owned(), epoch);
        if self.scope.as_ref() != Some(&scope) {
            self.editors.clear();
            self.scope = Some(scope);
        }
        if let Some(editor) = self.editors.get(&key) {
            // A successful enrollment/load barrier starts a fresh managed
            // writer. Old detached conditional writes fail closed against it.
            if editor.snapshot().view.stored.is_some() || loaded.stored.is_none() {
                return Ok(editor.clone());
            }
        }
        let editor = Editor::new(account.owner(key.clone()), loaded)?;
        self.editors.insert(key, editor.clone());
        Ok(editor)
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;

#[path = "checkpoint_release.rs"]
mod release;
#[path = "checkpoint_return.rs"]
mod return_draft;

#[path = "checkpoint_local.rs"]
mod local;

#[path = "checkpoint_settlement.rs"]
mod settlement;
