//! Original Search namespace plus independently accepted native socket barrier.
use super::*;
use super_platinum_core::slack::realtime::{Connection, ReadIdentity};
pub(super) fn connection<'a>(s: &'a ShellState, team: &str) -> Option<&'a Connection> {
    let super_platinum_core::state::RealtimeStatus::Connected(c) = &s.core.workspaces.get(team)?.rt
    else {
        return None;
    };
    Some(c)
}
#[derive(Clone)]
pub(super) struct Context {
    pub frame: Arc<read::Frame>,
    activation: Arc<()>,
    pub page: u32,
    cursor: recovery::Cursor,
    read: Arc<crate::bootstrap::ReadOwner>,
    client: super_platinum_core::slack::SlackClient,
    connection: ReadIdentity,
    generation: u64,
    epoch: Arc<()>,
}
impl Context {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        let b = s.search_message_pages.as_ref()?;
        if !s.signed_in
            || s.loading
            || !b.current(s)
            || b.pending.is_some()
            || b.failed
            || s.overlay != Some(Overlay::Search)
            || s.search_history_open
            || s.search_filters.is_some()
            || crate::fixture::is_fixture()
        {
            return None;
        }
        let f = b.frame.clone();
        if f.session.enterprise_id.is_some() || !member_id(&f.session.user_id) {
            return None;
        }
        let c = connection(s, &f.team)?;
        let v = Self {
            frame: f.clone(),
            activation: b.activation.clone(),
            page: b.shown,
            cursor: recovery::Cursor::capture(s),
            read: Arc::new(crate::bootstrap::ReadOwner::capture(
                &s.core,
                &f.transport,
                &f.session,
            )?),
            client: s.core.client.clone(),
            connection: c.read_identity(),
            generation: s.core.workspaces.get(&f.team)?.rt_generation,
            epoch: s.search_author_profiles.epoch.clone(),
        };
        v.current(s).then_some(v)
    }
    pub(super) fn base(&self, s: &ShellState) -> bool {
        self.serial(s) && self.connection.open()
    }
    /// Only serial Shell fields and immutable identities. The background socket
    /// close bit must not interrupt an already claimed synchronous handoff.
    pub(super) fn serial(&self, s: &ShellState) -> bool {
        s.signed_in
            && !s.loading
            && self.frame.current(s)
            && self.read.current(&s.core)
            && self.client.same_configuration(&s.core.client)
            && self.cursor.current(s)
            && s.core
                .workspaces
                .get(&self.frame.team)
                .is_some_and(|w| w.rt_generation == self.generation)
            && connection(s, &self.frame.team).is_some_and(|c| self.connection.is_for(c))
            && s.search_message_pages.as_ref().is_some_and(|b| {
                b.current(s)
                    && b.pending.is_none()
                    && !b.failed
                    && Arc::ptr_eq(&b.frame, &self.frame)
                    && Arc::ptr_eq(&b.activation, &self.activation)
                    && b.shown == self.page
            })
    }
    pub(super) fn policy_current(&self, s: &ShellState) -> bool {
        Arc::ptr_eq(&self.epoch, &s.search_author_profiles.epoch)
    }
    pub(super) fn caught_up(&self, s: &ShellState) -> bool {
        s.search_author_profiles
            .applied
            .get(&self.frame.team)
            .is_some_and(|(g, b)| {
                *g == self.generation
                    && connection(s, &self.frame.team).is_some_and(|c| b.current_for(c))
            })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.base(s) && self.policy_current(s) && self.caught_up(s)
    }
    pub(super) fn watch(&self, s: &ShellState, channel: &str) -> Option<ReadWatch> {
        if !self.current(s) {
            return None;
        }
        let c = connection(s, &self.frame.team)?;
        let (_, a) = s.search_author_profiles.applied.get(&self.frame.team)?;
        let w = c.watch_message_read(channel)?;
        w.armed_after(a).then_some(w)
    }
    pub(super) fn cooling(&self, s: &ShellState) -> bool {
        s.search_author_profiles
            .cooldowns
            .get(&self.frame.team)
            .is_some_and(|(owner, at, blocked)| {
                owner.current(&s.core)
                    && (*blocked || at.is_some_and(|t| t > std::time::Instant::now()))
            })
    }
    pub(super) fn note_failure(
        &self,
        s: &mut ShellState,
        error: &super_platinum_core::slack::Error,
    ) {
        use super_platinum_core::slack::Error;
        let seconds = match error {
            Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
            Error::HttpStatus {
                retry_after_secs, ..
            } => *retry_after_secs,
            _ => None,
        };
        if !self.read.current(&s.core) {
            return;
        }
        if let Some(n) = seconds {
            let mut at = std::time::Instant::now().checked_add(std::time::Duration::from_secs(n));
            let mut blocked = at.is_none();
            if let Some((old_owner, old_at, old_blocked)) =
                s.search_author_profiles.cooldowns.get(&self.frame.team)
                && old_owner.current(&s.core)
            {
                blocked |= *old_blocked;
                at = at.max(*old_at);
            }
            s.search_author_profiles
                .cooldowns
                .insert(self.frame.team.clone(), (self.read.clone(), at, blocked));
        }
    }
    pub(super) fn refreshed(&self, s: &ShellState) -> Option<Self> {
        if !self.base(s) || !self.caught_up(s) {
            return None;
        }
        let mut c = self.clone();
        c.epoch = s.search_author_profiles.epoch.clone();
        Some(c)
    }
}
