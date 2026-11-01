//! Original Book occurrence and authenticated connection; no indexed permission.
use super::*;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        SlackClient, Transport,
        realtime::{ReadBarrier, ReadIdentity, ReadWatch},
    },
};
pub(super) struct Applied {
    pub team: String,
    pub generation: u64,
    pub barrier: ReadBarrier,
}
pub(super) struct Owner {
    pub row: RowSource,
    pub key: registry::Key,
    pub client: SlackClient,
    pub session: WorkspaceSession,
    pub transport: Arc<Transport>,
    identity: recovery::Identity,
    connection: ReadIdentity,
    generation: u64,
    input: u64,
    shortcut: u64,
    retired: AtomicBool,
}
fn connection<'a>(
    s: &'a ShellState,
    team: &str,
) -> Option<&'a super_platinum_core::slack::realtime::Connection> {
    let super_platinum_core::state::RealtimeStatus::Connected(c) = &s.core.workspaces.get(team)?.rt
    else {
        return None;
    };
    Some(c)
}
impl Owner {
    pub(super) fn capture(s: &ShellState, row: RowSource) -> Option<(Self, Arc<ReadWatch>)> {
        let hit = row.resolve(s)?;
        if !s.signed_in || s.loading || !hit.channel_id.starts_with('C') || hit.thread_ts.is_some()
        {
            return None;
        }
        let team = s.core.active_team.clone()?;
        let session = s.core.session.as_ref()?.workspaces.get(&team)?.clone();
        let w = s.core.workspaces.get(&team)?;
        if session.user_id.is_empty() || w.self_user_id != session.user_id {
            return None;
        }
        let owner = Self {
            row,
            key: registry::Key {
                actor: registry::Actor {
                    account: s.core.active_account.clone()?,
                    team: team.clone(),
                    user: session.user_id.clone(),
                },
                channel: hit.channel_id,
                ts: hit.ts,
            },
            client: s.core.client.clone(),
            session,
            transport: s.core.transport.clone()?,
            identity: recovery::Identity::capture(s)?,
            connection: connection(s, &team)?.read_identity(),
            generation: w.rt_generation,
            input: s.search_input_revision,
            shortcut: s.shortcut_generation,
            retired: AtomicBool::new(false),
        };
        let watch = owner.arm(s)?;
        Some((owner, watch))
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let ok = s.signed_in
            && !s.loading
            && self.identity.current(s)
            && self
                .identity
                .matches_session(&self.transport, &self.session)
            && s.search_input_revision == self.input
            && s.shortcut_generation == self.shortcut
            && s.core
                .workspaces
                .get(&self.key.actor.team)
                .is_some_and(|w| w.rt_generation == self.generation)
            && connection(s, &self.key.actor.team)
                .is_some_and(|c| self.connection.is_for(c) && self.connection.open())
            && self.row.resolve(s).is_some_and(|h| {
                h.channel_id == self.key.channel && h.ts == self.key.ts && h.thread_ts.is_none()
            })
            && !known_denied(s, &self.key);
        if !ok {
            self.retired.store(true, Ordering::Release)
        }
        ok
    }
    pub(super) fn arm(&self, s: &ShellState) -> Option<Arc<ReadWatch>> {
        if !self.current(s) {
            return None;
        }
        let c = connection(s, &self.key.actor.team)?;
        let a = s
            .search_reaction_actions
            .applied
            .get(&self.key.actor.team)?;
        if a.team != self.key.actor.team
            || a.generation != self.generation
            || !a.barrier.current_for(c)
        {
            return None;
        }
        let watch = c.watch_message_read(&self.key.channel)?;
        watch.armed_after(&a.barrier).then(|| Arc::new(watch))
    }
    pub(super) fn prewrite(&self, s: &ShellState, watch: &ReadWatch) -> bool {
        self.current(s)
            && self.transport.health() != super_platinum_core::slack::transport::Health::Offline
            && watch.current()
            && !s.search_reaction_actions.registry.cooling(&self.key.actor)
    }
}
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.),
        Some(_) => true,
    }
}
fn known_denied(s: &ShellState, k: &registry::Key) -> bool {
    let Some(w) = s.core.workspaces.get(&k.actor.team) else {
        return true;
    };
    if w.channels.get(&k.channel).is_some_and(|c| {
        c.is_archived
            || c.is_im
            || c.is_mpim
            || c.is_group
            || ["is_frozen", "is_file", "isUnknown", "isNonExistent"]
                .iter()
                .any(|f| truthy(c.extra.get(*f)))
            || c.extra.get("is_member") == Some(&Value::Bool(false))
    }) {
        return true;
    }
    let denied = |m: &super_platinum_core::slack::models::Message| {
        m.ts.as_deref() == Some(&k.ts)
            && (m.subtype.as_deref().is_some_and(|v| !v.is_empty())
                || [
                    "isUnknown",
                    "isNonExistent",
                    "isEKMError",
                    "is_ephemeral",
                    "is_deleted",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                ]
                .iter()
                .any(|f| truthy(m.extra.get(*f))))
    };
    w.messages
        .get(&k.channel)
        .is_some_and(|b| b.was_deleted(&k.ts) || b.messages.iter().any(denied))
        || s.core.threads.iter().any(|((t, c, _), b)| {
            t == &k.actor.team
                && c == &k.channel
                && (b.was_deleted(&k.ts) || b.messages.iter().any(denied))
        })
}
pub(crate) fn accept_barrier(
    s: &mut ShellState,
    team: &str,
    generation: u64,
    barrier: ReadBarrier,
) {
    if s.core
        .workspaces
        .get(team)
        .is_some_and(|w| w.rt_generation == generation)
        && connection(s, team).is_some_and(|c| barrier.belongs_to(c))
    {
        s.search_reaction_actions.applied.insert(
            team.into(),
            Applied {
                team: team.into(),
                generation,
                barrier,
            },
        );
    }
}
