//! Closed finite adapter at the production acquisition await, never a marker injector.
use super::*;
use tokio::sync::oneshot;
#[derive(Clone)]
pub(crate) struct Pending {
    pub(crate) serial: u64,
    pub(crate) method: String,
    pub(crate) ids: Vec<String>,
    pub(crate) versions: BTreeMap<String, u64>,
}
struct Held {
    pending: Pending,
    reply: oneshot::Sender<Result<Value, Error>>,
}
#[derive(Default)]
pub(super) struct Model {
    serial: u64,
    held: BTreeMap<u64, Held>,
}
pub(crate) fn enabled() -> bool {
    crate::profile_edit::sections::fixture::missing_channel::named()
}
pub(super) fn installed(_shell: &ShellState) -> bool {
    enabled()
}
pub(crate) fn pending(shell: &ShellState) -> Vec<Pending> {
    if !enabled() {
        return Vec::new();
    }
    shell
        .app_home
        .channel_fetches
        .unknown
        .fixture
        .held
        .values()
        .map(|h| h.pending.clone())
        .collect()
}
pub(crate) fn release(shell: &mut ShellState, serial: u64, response: Result<Value, Error>) -> bool {
    enabled()
        && shell
            .app_home
            .channel_fetches
            .unknown
            .fixture
            .held
            .remove(&serial)
            .is_some_and(|h| h.reply.send(response).is_ok())
}
pub(super) fn hold(
    shell: &mut ShellState,
    a: &Attempt,
    method: &str,
    ids: &[String],
    versions: BTreeMap<String, u64>,
) -> Result<oneshot::Receiver<Result<Value, Error>>, Error> {
    if !enabled() || !a.admit(shell) || ids.iter().any(|id| !a.flight.ids().contains(id)) {
        return Err(unavailable());
    }
    let m = &mut shell.app_home.channel_fetches.unknown.fixture;
    m.serial = m.serial.wrapping_add(1);
    let pending = Pending {
        serial: m.serial,
        method: method.into(),
        ids: ids.to_vec(),
        versions,
    };
    let (tx, rx) = oneshot::channel();
    m.held.insert(m.serial, Held { pending, reply: tx });
    Ok(rx)
}
/// Retirement only: all held replies/workers/drains must finish, independently
/// of an untouched replacement account's intentionally queued Boot source.
pub(crate) fn quiescent(shell: &ShellState) -> bool {
    enabled()
        && crate::profile_edit::sections::fixture::missing_channel::case() == "retirement"
        && shell
            .app_home
            .channel_fetches
            .unknown
            .fixture
            .held
            .is_empty()
        && shell
            .app_home
            .channel_fetches
            .unknown
            .slots
            .values()
            .all(|r| r.worker.is_none())
        && shell
            .app_home
            .channel_fetches
            .slots
            .values()
            .all(|s| s.drain.is_none())
}
pub(crate) fn idle(shell: &ShellState) -> bool {
    enabled()
        && shell
            .app_home
            .channel_fetches
            .unknown
            .fixture
            .held
            .is_empty()
        && shell
            .app_home
            .channel_fetches
            .unknown
            .slots
            .values()
            .all(|r| r.worker.is_none())
        && shell
            .app_home
            .channel_fetches
            .slots
            .values()
            .all(|s| s.drain.is_none())
        && shell
            .core
            .workspaces
            .values()
            .all(|w| !w.directory_channels.has_pending())
}
pub(crate) fn status(shell: &ShellState) -> Value {
    if !enabled() {
        return Value::Null;
    }
    let managers=shell.app_home.channel_fetches.unknown.slots.values().map(|r|serde_json::json!({
        "current":r.owner.current(shell),"worker":r.worker.is_some(),"active":r.active,"pendingIds":r.manager.pending()
    })).collect::<Vec<_>>();
    serde_json::json!({"requestCount":shell.app_home.channel_fetches.unknown.fixture.serial,
        "held":shell.app_home.channel_fetches.unknown.fixture.held.len(),"managers":managers,"idle":idle(shell),"networkCalls":0})
}
