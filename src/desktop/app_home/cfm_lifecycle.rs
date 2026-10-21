//! Ephemeral native acquisition of shared CFM socket/version observations.
//! A connection generation identifies custody, not a successful connection count.
use super::channel_fetches::context::Context;
use crate::state::ShellState;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::slack::Error;

mod listener;
mod pause;
pub(crate) use listener::install as install_listener;
pub(crate) use pause::{dispatch, indefinitely, keypress, paused, unpause, with_pause};

type Key = (String, String, String);
pub(crate) struct Registry {
    entries: BTreeMap<Key, Entry>,
    changed: tokio::sync::watch::Sender<u64>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
}
impl Registry {
    fn notify(&self) {
        self.changed.send_modify(|n| *n = n.wrapping_add(1));
    }
}
struct Entry {
    context: Arc<Context>,
    identity: Arc<()>,
    slow_connections: u64,
    accepted_generation: Option<u64>,
    major: u64,
    major_initialized: bool,
    boot: Option<Arc<()>>,
}
pub(crate) struct Boot {
    context: Arc<Context>,
    identity: Arc<()>,
    token: Arc<()>,
}
/// Same-owner observable values; this grants no Channel publication or IO authority.
#[derive(Clone)]
pub(crate) struct Snapshot {
    context: Arc<Context>,
    identity: Arc<()>,
    pub(crate) slow_connections: u64,
    pub(crate) major: u64,
}
impl Snapshot {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.account_current(shell)
            && shell
                .app_home
                .cfm_lifecycle
                .entries
                .get(&self.context.key())
                .is_some_and(|e| {
                    Arc::ptr_eq(&e.identity, &self.identity)
                        && e.slow_connections == self.slow_connections
                        && e.major == self.major
                })
    }
}
fn ensure(shell: &mut ShellState, team: &str) -> Result<Key, Error> {
    let ctx = Context::capture(shell, team)?;
    let key = ctx.key();
    let retained = shell
        .app_home
        .cfm_lifecycle
        .entries
        .get(&key)
        .is_some_and(|e| e.context.account_current(shell));
    if !retained {
        shell.app_home.cfm_lifecycle.entries.insert(
            key.clone(),
            Entry {
                context: Arc::new(ctx),
                identity: Arc::new(()),
                slow_connections: 0,
                accepted_generation: None,
                major: 1,
                // Not positive evidence until real source init/resync action adoption.
                major_initialized: false,
                boot: None,
            },
        );
    }
    Ok(key)
}
pub(crate) fn capture(shell: &mut ShellState, team: &str) -> Result<Snapshot, Error> {
    let key = ensure(shell, team)?;
    let e = &shell.app_home.cfm_lifecycle.entries[&key];
    Ok(Snapshot {
        context: e.context.clone(),
        identity: e.identity.clone(),
        slow_connections: e.slow_connections,
        major: e.major,
    })
}
/// Positive lifecycle values only. Entry allocation/default major is never an
/// initialization observation. The real serial init/resync sink must establish
/// major_initialized after its accepted source constructor/action boundary.
pub(crate) fn observed(shell: &mut ShellState, team: &str) -> Result<Snapshot, Error> {
    let key = ensure(shell, team)?;
    if !shell.app_home.cfm_lifecycle.entries[&key].major_initialized {
        return Err(Error::Api("channel_lifecycle_unavailable".into()));
    }
    capture(shell, team)
}

/// Read an existing positive observation without allocating a lifecycle entry.
pub(crate) fn observed_readonly(shell: &ShellState, team: &str) -> Result<Snapshot, Error> {
    let context = Context::capture(shell, team)?;
    let entry = shell
        .app_home
        .cfm_lifecycle
        .entries
        .get(&context.key())
        .filter(|entry| entry.major_initialized && entry.context.account_current(shell))
        .ok_or_else(|| Error::Api("channel_lifecycle_unavailable".into()))?;
    Ok(Snapshot {
        context: entry.context.clone(),
        identity: entry.identity.clone(),
        slow_connections: entry.slow_connections,
        major: entry.major,
    })
}

/// Called only by the accepted split initializer after its synchronous source
/// payload sink succeeds. A userBoot success or entry allocation cannot call it.
pub(super) fn initialized(shell: &mut ShellState, context: &Context) -> bool {
    if !context.current(shell) {
        return false;
    }
    let Ok(key) = ensure(shell, &context.session.team_id) else {
        return false;
    };
    let entry = shell.app_home.cfm_lifecycle.entries.get_mut(&key).unwrap();
    if entry.major_initialized {
        return false;
    }
    entry.major_initialized = true;
    shell.app_home.cfm_lifecycle.notify();
    true
}

/// Resync version effects do not manufacture initialization authority.
pub(super) fn advance_major(shell: &mut ShellState, context: &Context) -> bool {
    if !context.current(shell) {
        return false;
    }
    let Ok(key) = ensure(shell, &context.session.team_id) else {
        return false;
    };
    let entry = shell.app_home.cfm_lifecycle.entries.get_mut(&key).unwrap();
    let Some(major) = entry.major.checked_add(1) else {
        return false;
    };
    entry.major = major;
    shell.app_home.cfm_lifecycle.notify();
    true
}
/// Subscribe before attempting observed() so unavailable initialization can
/// later wake the caller without a poll loop. Notifications convey no values.
pub(crate) struct Subscription {
    context: Arc<Context>,
    changed: tokio::sync::watch::Receiver<u64>,
}
impl Subscription {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.account_current(shell)
    }
    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.clone()
    }
}
pub(crate) fn subscribe(shell: &ShellState, team: &str) -> Result<Subscription, Error> {
    Ok(Subscription {
        context: Arc::new(Context::capture(shell, team)?),
        changed: shell.app_home.cfm_lifecycle.changed.subscribe(),
    })
}

/// Called only after the real worker has accepted its socket with original
/// account/Transport/generation guards. Native connect performs a full WebSocket
/// connection; it has no Slack fast-reconnect resume path.
pub(crate) fn connected(shell: &mut ShellState, team: &str, generation: u64) {
    if shell
        .core
        .workspaces
        .get(team)
        .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    let Ok(key) = ensure(shell, team) else { return };
    let e = shell.app_home.cfm_lifecycle.entries.get_mut(&key).unwrap();
    if e.accepted_generation.is_some_and(|old| old >= generation) {
        return;
    }
    let Some(count) = e.slow_connections.checked_add(1) else {
        return;
    };
    e.accepted_generation = Some(generation);
    e.slow_connections = count;
    shell.app_home.cfm_lifecycle.notify();
}

/// Paired with actual BootRead::begin_full, never picker metadata reads.
pub(crate) fn boot_started(shell: &mut ShellState, team: &str) -> Option<Boot> {
    let key = ensure(shell, team).ok()?;
    let entry = shell.app_home.cfm_lifecycle.entries.get_mut(&key)?;
    let token = Arc::new(());
    entry.boot = Some(token.clone());
    indefinitely();
    Some(Boot {
        context: entry.context.clone(),
        identity: entry.identity.clone(),
        token,
    })
}
/// Caller first proves the same BootRead settlement; this additional token
/// cannot unpause a newer full-bootstrap attempt or replacement account.
pub(crate) fn boot_settled(shell: &mut ShellState, boot: Option<Boot>) {
    let Some(boot) = boot else { return };
    if !boot.context.account_current(shell) {
        return;
    }
    let Some(entry) = shell
        .app_home
        .cfm_lifecycle
        .entries
        .get_mut(&boot.context.key())
    else {
        return;
    };
    if !Arc::ptr_eq(&entry.identity, &boot.identity)
        || entry
            .boot
            .as_ref()
            .is_none_or(|token| !Arc::ptr_eq(token, &boot.token))
    {
        return;
    }
    entry.boot = None;
    unpause();
}

#[cfg(test)]
mod tests;
