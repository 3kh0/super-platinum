//! List-owned ordinary channel completion. Shared acquisition produces private
//! ranked receipts; only this original native draft may consume one.
use super::*;
use crate::profile_edit::sections::rich::search::{self, ChannelBatch, MemberBatch, Preferences};
use serde::Serialize;
use tokio::sync::watch;
mod host;
mod members;
pub(crate) use members::run_members;
mod member_context;
#[cfg(test)]
mod tests;
const MAX_SAFE: u64 = 9_007_199_254_740_991;

#[derive(Clone, Default)]
pub(super) struct State {
    member_context: member_context::State,
    query: Option<Query>,
    preferences: Option<Arc<Preferences>>,
    retry: Option<(String, String, i64)>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Channel,
    Member,
}
#[derive(Clone)]
enum Batch {
    Channel(ChannelBatch),
    Member(MemberBatch),
}
impl Batch {
    fn kind(&self) -> Kind {
        match self {
            Self::Channel(_) => Kind::Channel,
            Self::Member(_) => Kind::Member,
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Channel(b) => b.current(shell),
            Self::Member(b) => b.current(shell),
        }
    }
    fn choices(&self, shell: &ShellState) -> Option<Vec<(Value, String)>> {
        match self {
            Self::Channel(b) => b.choices(shell),
            Self::Member(b) => b.choices(shell),
        }
    }
}
#[derive(Clone)]
struct Query {
    lease: Lease,
    cancel: watch::Sender<bool>,
    consumed: bool,
    started: bool,
    revision: u64,
    batch: Option<Batch>,
    kind: Option<Kind>,
    member_context: Option<Arc<member_context::Observation>>,
    policy: Option<Arc<Preferences>>,
    values: Vec<Completion>,
}
#[derive(Clone)]
pub(crate) struct Lease {
    binding: draft::Binding,
    mount: String,
    sequence: u64,
    identity: Arc<()>,
    account: String,
    team: String,
    socket: u64,
    cancelled: watch::Receiver<bool>,
}
#[derive(Clone, Serialize)]
pub(crate) struct Completion {
    value: Value,
    label: String,
}
#[derive(Clone, Serialize)]
pub(crate) struct Presentation {
    pub sequence: u64,
    pub revision: u64,
    pub labels: Vec<String>,
}
impl Lease {
    fn original_mount(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
            && shell.core.active_account.as_deref() == Some(self.account.as_str())
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.rt_generation == self.socket)
            && draft::native(shell, &self.binding)
                .is_some_and(|native| native.matches(&self.binding) && native.mount() == self.mount)
    }
    fn mounted(&self, shell: &ShellState) -> bool {
        self.original_mount(shell) && native_ready(shell, &self.binding, &self.mount)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.mounted(shell)
            && shell
                .slack_lists
                .comments
                .completion
                .query
                .as_ref()
                .is_some_and(|q| {
                    !q.consumed
                        && Arc::ptr_eq(&q.lease.identity, &self.identity)
                        && q.policy.as_ref().is_none_or(|p| p.current(shell))
                        && q.member_context.as_ref().is_none_or(|c| c.current(shell))
                })
    }
    async fn cancelled(&self) {
        let mut cancel = self.cancelled.clone();
        loop {
            if *cancel.borrow_and_update() {
                return;
            }
            if cancel.changed().await.is_err() {
                return;
            }
        }
    }
}
/// Preference observations belong to the original visible native mount even
/// between caret queries. A consumed query may still own reusable preferences;
/// it grants no selection authority, but accepted events must retire that cache.
pub(crate) fn observes_preferences(shell: &ShellState, team: &str, generation: u64) -> bool {
    shell
        .slack_lists
        .comments
        .completion
        .query
        .as_ref()
        .is_some_and(|query| {
            query.lease.team == team
                && query.lease.socket == generation
                // Busy/composition gates insertion, never receipt invalidation.
                && query.lease.original_mount(shell)
        })
}
fn native_ready(shell: &ShellState, binding: &draft::Binding, mount: &str) -> bool {
    draft::entry(shell, binding.id()).is_some_and(|d| {
        !d.busy
            && !d.loading
            && !d.composing
            && !d.capture_failed
            && d.native
                .as_ref()
                .is_some_and(|n| n.matches(binding) && n.mount() == mount && !n.failed())
    })
}
/// `sequence` is the original native caret ticket. Renderer changes retire the
/// old lease before asking for another; no displayed label contains authority.
pub(crate) fn begin(
    shell: &mut ShellState,
    binding: &draft::Binding,
    mount: &str,
    sequence: u64,
) -> Option<Lease> {
    if sequence == 0
        || sequence >= MAX_SAFE
        || mount.is_empty()
        || !binding.current(shell)
        || !native_ready(shell, binding, mount)
    {
        return None;
    }
    let account = shell.core.active_account.clone()?;
    let team = binding.owner.scope.team.clone();
    let socket = shell.core.workspaces.get(&team)?.rt_generation;
    let state = &mut shell.slack_lists.comments.completion;
    if state
        .query
        .as_ref()
        .is_some_and(|q| q.lease.mount == mount && sequence <= q.lease.sequence)
    {
        return None;
    }
    if let Some(old) = state.query.take() {
        old.cancel.send_replace(true);
    }
    let (cancel, cancelled) = watch::channel(false);
    let lease = Lease {
        binding: binding.clone(),
        mount: mount.into(),
        sequence,
        identity: Arc::new(()),
        account,
        team,
        socket,
        cancelled,
    };
    state.query = Some(Query {
        lease: lease.clone(),
        cancel,
        consumed: false,
        started: false,
        revision: 0,
        batch: None,
        kind: None,
        member_context: None,
        policy: None,
        values: Vec::new(),
    });
    Some(lease)
}
pub(crate) fn close(shell: &mut ShellState, lease: &Lease) -> bool {
    let Some(q) = shell
        .slack_lists
        .comments
        .completion
        .query
        .as_mut()
        .filter(|q| Arc::ptr_eq(&q.lease.identity, &lease.identity))
    else {
        return false;
    };
    q.consumed = true;
    q.values.clear();
    q.batch = None;
    q.cancel.send_replace(true);
    true
}
pub(super) fn retire(shell: &mut ShellState) {
    if let Some(q) = shell.slack_lists.comments.completion.query.take() {
        q.cancel.send_replace(true);
    }
    shell.slack_lists.comments.completion.preferences = None;
}
pub(crate) async fn policy(mut state: Signal<ShellState>, lease: &Lease) -> Result<bool, Error> {
    let search = search::Search::capture(state, lease.clone())?;
    let friendly = tokio::select! {
        biased;
        _ = retired(state, lease) => { close(&mut state.write(),lease); return Err(unavailable()); },
        result = search.friendly_policy(state) => result?,
    };
    let mut shell = state.write();
    if !lease.current(&shell) {
        return Err(unavailable());
    }
    let preferences = shell
        .slack_lists
        .comments
        .completion
        .preferences
        .clone()
        .ok_or_else(unavailable)?;
    if !preferences.current(&shell) {
        return Err(unavailable());
    }
    shell
        .slack_lists
        .comments
        .completion
        .query
        .as_mut()
        .unwrap()
        .policy = Some(preferences);
    Ok(friendly)
}
pub(crate) async fn run(
    mut state: Signal<ShellState>,
    lease: Lease,
    raw_query: String,
    on_results: impl Fn(Presentation) + 'static,
) -> Result<(), Error> {
    {
        let mut shell = state.write();
        if !lease.current(&shell) || !raw_query.starts_with(['#', '\u{ff03}']) {
            return Err(unavailable());
        }
        let q = shell
            .slack_lists
            .comments
            .completion
            .query
            .as_mut()
            .unwrap();
        if !start(q, Kind::Channel) {
            return Err(unavailable());
        }
    }
    let publish_lease = lease.clone();
    // The single request-scoped watch ends also when the host is retired without
    // a DOM event (account, target or socket changes). No always-on task.
    let operation =
        search::run_channels(state, lease.clone(), raw_query, move |mut state, batch| {
            let presentation = publish(&mut state.write(), &publish_lease, Batch::Channel(batch))
                .ok_or_else(unavailable)?;
            on_results(presentation);
            Ok(())
        });
    let result = tokio::select! {
        biased;
        _ = retired(state, &lease) => { close(&mut state.write(),&lease); Err(unavailable()) },
        result = operation => result,
    };
    if result.is_err() {
        let mut shell = state.write();
        if lease.current(&shell) {
            let q = shell
                .slack_lists
                .comments
                .completion
                .query
                .as_mut()
                .unwrap();
            q.batch = None;
            q.values.clear();
        } else {
            close(&mut shell, &lease);
        }
    }
    result
}
fn start(q: &mut Query, kind: Kind) -> bool {
    if q.started || q.kind.is_some() || q.policy.is_none() {
        return false;
    }
    q.kind = Some(kind);
    q.started = true;
    true
}
async fn retired(state: Signal<ShellState>, lease: &Lease) {
    loop {
        if !lease.current(&state.peek()) {
            return;
        }
        tokio::select! {
            _ = lease.cancelled() => return,
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
        }
    }
}
fn publish(shell: &mut ShellState, lease: &Lease, batch: Batch) -> Option<Presentation> {
    if !lease.current(shell)
        || shell.slack_lists.comments.completion.query.as_ref()?.kind != Some(batch.kind())
    {
        return None;
    }
    let values = batch
        .choices(shell)?
        .into_iter()
        .map(|(value, label)| Completion { value, label })
        .collect::<Vec<_>>();
    if values.len() > 25 {
        return None;
    }
    let q = shell.slack_lists.comments.completion.query.as_mut()?;
    let revision = q.revision.checked_add(1).filter(|r| *r < MAX_SAFE)?;
    let labels = values.iter().map(|c| c.label.clone()).collect();
    q.revision = revision;
    q.batch = Some(batch);
    q.values = values;
    Some(Presentation {
        sequence: lease.sequence,
        revision,
        labels,
    })
}
/// Native code still rechecks the original caret before insertion. A result
/// index is valid only against this exact current private source/ranking batch.
pub(crate) fn choose(
    shell: &mut ShellState,
    lease: &Lease,
    revision: u64,
    index: usize,
) -> Option<Completion> {
    if !lease.current(shell) {
        return None;
    }
    let q = shell.slack_lists.comments.completion.query.as_ref()?;
    if revision == 0 || q.revision != revision || !q.batch.as_ref()?.current(shell) {
        return None;
    }
    let value = q.values.get(index)?.clone();
    close(shell, lease);
    Some(value)
}

/// Bounded passive diagnostics; no credentials, request bodies or native tickets.
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let state = &shell.slack_lists.comments.completion;
    let Some(q) = &state.query else {
        return Value::Null;
    };
    serde_json::json!({"sequence":q.lease.sequence,"revision":q.revision,
        "labels":q.values.iter().map(|v|v.label.clone()).collect::<Vec<_>>(),
        "values":q.values,
        "current":q.lease.current(shell),"started":q.started,"consumed":q.consumed,
        "batchCurrent":q.batch.as_ref().is_some_and(|batch| batch.current(shell)),
        "settledBatch":matches!(&q.batch, Some(Batch::Member(batch)) if batch.settled && batch.current(shell)),
        "friendly":q.policy.as_ref().and_then(|p|p.friendly_channels().ok())})
}
