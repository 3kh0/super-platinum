//! Closed offline adapter. Synthetic attempts never attest an HTTP poll.
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
    installed: bool,
    serial: u64,
    held: BTreeMap<u64, Held>,
    offset_ms: u64,
    topic_time: Option<Instant>,
    hold_drain: bool,
}
pub(in crate::app_home) fn enabled() -> bool {
    if crate::profile_edit::sections::fixture::channel_title::named()
        || crate::profile_edit::sections::fixture::missing_channel::named()
    {
        return true;
    }
    if crate::slack_lists::comments::fixture::plain_channels::named() {
        return true;
    }
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-channel-recovery" | "profile-channel-recovery-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    if enabled() {
        shell.app_home.channel_fetches.conditional.fixture.installed = true;
    }
}
pub(super) fn installed(shell: &ShellState) -> bool {
    enabled() && shell.app_home.channel_fetches.conditional.fixture.installed
}
pub(crate) fn pending(shell: &ShellState) -> Vec<Pending> {
    if !installed(shell) {
        return Vec::new();
    }
    shell
        .app_home
        .channel_fetches
        .conditional
        .fixture
        .held
        .values()
        .map(|h| h.pending.clone())
        .collect()
}
/// Synthetic-fixture diagnostics only; no credentials or payloads are exposed.
pub(crate) fn status(shell: &ShellState) -> Value {
    if !enabled() {
        return Value::Null;
    }
    let pause = crate::app_home::cfm_lifecycle::with_pause(|pause| pause.paused());
    let managers = shell.app_home.channel_fetches.conditional.slots.values().map(|runtime| {
        serde_json::json!({
            "accountCurrent": runtime.context.account_current(shell),
            "worker": runtime.worker.is_some(),
            "active": runtime.active,
            "pendingIds": runtime.manager.pending(),
            "nextWake": runtime.manager.next_wake(),
            "freshIds": now(shell).ok().and_then(|now| runtime.manager.fresh_ids(&runtime.scope, now).ok())
        })
    }).collect::<Vec<_>>();
    let queues = shell.app_home.channel_fetches.slots.values().filter_map(|slot| {
        let owner = Owner { context: slot.context.clone(), token: slot.token.clone() };
        if !owner.publication_current(shell) { return None; }
        let observed = observe(shell, &owner.context).ok()?;
        Some(serde_json::json!({
            "pending": shell.core.workspaces[&owner.context.session.team_id].directory_channels.has_pending(),
            "incoming": observed.incoming(), "rows": observed.rows()
        }))
    }).collect::<Vec<_>>();
    serde_json::json!({
        "installed": installed(shell),
        "listenerReady": pause.is_ok(),
        "sourcePaused": pause.ok(),
        "paused": crate::app_home::cfm_lifecycle::paused(),
        "managers": managers,
        "drainHeld": drain_held(shell), "queues": queues,
        "requestCount": shell.app_home.channel_fetches.conditional.fixture.serial,
        "held": shell.app_home.channel_fetches.conditional.fixture.held.len()
    })
}
pub(crate) fn release(shell: &mut ShellState, serial: u64, response: Result<Value, Error>) -> bool {
    if !installed(shell) {
        return false;
    }
    shell
        .app_home
        .channel_fetches
        .conditional
        .fixture
        .held
        .remove(&serial)
        .is_some_and(|h| h.reply.send(response).is_ok())
}
pub(crate) fn idle(shell: &ShellState) -> bool {
    installed(shell)
        && shell
            .app_home
            .channel_fetches
            .conditional
            .fixture
            .held
            .is_empty()
        && shell
            .app_home
            .channel_fetches
            .conditional
            .slots
            .values()
            .all(|r| r.worker.is_none())
        && shell
            .app_home
            .channel_fetches
            .slots
            .values()
            .all(|s| s.drain.is_none())
        && !shell
            .app_home
            .channel_fetches
            .conditional
            .slots
            .values()
            .any(|runtime| {
                runtime.context.account_current(shell)
                    && shell.core.workspaces[&runtime.context.session.team_id]
                        .directory_channels
                        .has_pending()
            })
}
/// Hold only the real renderer drain; source enqueue and freshness stay unchanged.
pub(crate) fn hold_drain(shell: &mut ShellState) -> Result<(), Error> {
    if !installed(shell) || !idle(shell) || drain_held(shell) {
        return Err(unavailable());
    }
    shell
        .app_home
        .channel_fetches
        .conditional
        .fixture
        .hold_drain = true;
    Ok(())
}
pub(in crate::app_home::channel_fetches) fn drain_held(shell: &ShellState) -> bool {
    installed(shell)
        && shell
            .app_home
            .channel_fetches
            .conditional
            .fixture
            .hold_drain
}
pub(crate) fn release_drain(mut state: Signal<ShellState>) -> Result<(), Error> {
    let owners = {
        let mut shell = state.write();
        if !drain_held(&shell) {
            return Err(unavailable());
        }
        shell
            .app_home
            .channel_fetches
            .conditional
            .fixture
            .hold_drain = false;
        shell
            .app_home
            .channel_fetches
            .slots
            .values()
            .filter_map(|slot| {
                let owner = Owner {
                    context: slot.context.clone(),
                    token: slot.token.clone(),
                };
                owner.publication_current(&shell).then_some(owner)
            })
            .collect::<Vec<_>>()
    };
    for owner in owners {
        drain::kick(state, owner);
    }
    Ok(())
}
pub(super) fn hold(
    shell: &mut ShellState,
    attempt: &Attempt,
    method: &str,
    ids: &[String],
    versions: BTreeMap<String, u64>,
) -> Result<oneshot::Receiver<Result<Value, Error>>, Error> {
    if !installed(shell) || !attempt.admit(shell) {
        return Err(Error::Offline("closed channel fixture unavailable".into()));
    }
    let m = &mut shell.app_home.channel_fetches.conditional.fixture;
    m.serial = m.serial.checked_add(1).ok_or_else(unavailable)?;
    let (reply, receive) = oneshot::channel();
    m.held.insert(
        m.serial,
        Held {
            pending: Pending {
                serial: m.serial,
                method: method.into(),
                ids: ids.to_vec(),
                versions,
            },
            reply,
        },
    );
    Ok(receive)
}
pub(super) fn offset(shell: &ShellState) -> u64 {
    if installed(shell) {
        shell.app_home.channel_fetches.conditional.fixture.offset_ms
    } else {
        0
    }
}
pub(in crate::app_home::channel_fetches) fn topic_now(shell: &ShellState) -> Instant {
    if installed(shell) {
        shell
            .app_home
            .channel_fetches
            .conditional
            .fixture
            .topic_time
            .unwrap_or_else(Instant::now)
    } else {
        Instant::now()
    }
}
pub(crate) fn resynced(mut state: Signal<ShellState>, ids: Vec<String>) -> Result<(), Error> {
    let work = {
        let mut s = state.write();
        if !installed(&s) {
            return Err(unavailable());
        }
        let m = &mut s.app_home.channel_fetches.conditional.fixture;
        m.offset_ms = m.offset_ms.checked_add(361_000).ok_or_else(unavailable)?;
        m.topic_time = Some(m.topic_time.unwrap_or_else(Instant::now) + Duration::from_secs(361));
        let team = s.core.active_team.clone().ok_or_else(unavailable)?;
        let generation = s.core.workspaces[&team].rt_generation;
        let raw = serde_json::from_value(serde_json::json!({"type":"resync","channel_ids":ids}))
            .map_err(|_| unavailable())?;
        let event = super_platinum_core::slack::events::RtEvent::Unknown(raw);
        let before = events::before_event(&s, &team, generation, &event).ok_or_else(unavailable)?;
        events::after_event(&mut s, Some(before)).ok_or_else(unavailable)?
    };
    events::resume(state, Some(work));
    Ok(())
}
