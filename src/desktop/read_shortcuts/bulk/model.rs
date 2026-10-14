use super::*;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Owner {
    pub account: String,
    pub team: String,
    pub user: String,
}
impl Owner {
    pub fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.as_ref()?;
        Some(Self {
            account: s.core.active_account.clone()?,
            team: team.clone(),
            user: s.core.workspaces.get(team)?.self_user_id.clone(),
        })
    }
}
pub(crate) struct Context {
    pub owner: Owner,
    pub epoch: u64,
    pub rt_generation: u64,
    pub session: WorkspaceSession,
    pub client: SlackClient,
    pub transport: Option<Arc<Transport>>,
    pub api_owner: api::ReadShortcutOwner,
    pub rate_owner: crate::channel_mute::PrefsCooldownToken,
    cursor: Option<Arc<crate::realtime::activity_counts::CursorOwner>>,
    cookie: Option<String>,
    channel_generation: u64,
    main_view: MainView,
}
impl Context {
    pub fn capture(s: &ShellState) -> Result<Self, Error> {
        let owner = Owner::capture(s).ok_or(Error::TransportNotConfigured)?;
        let isolated = fixture::enabled(s);
        if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() && !isolated {
            return Err(Error::TransportNotConfigured);
        }
        let session = if isolated {
            fixture::session(s)
        } else {
            s.core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&owner.team))
                .cloned()
                .ok_or(Error::TransportNotConfigured)?
        };
        if session.user_id != owner.user || session.team_id != owner.team {
            return Err(Error::TransportNotConfigured);
        }
        let transport = s.core.transport.clone();
        let cursor = transport
            .as_ref()
            .and_then(|t| {
                crate::realtime::activity_counts::CursorOwner::capture(&s.core, t, &session)
            })
            .map(Arc::new);
        if !isolated && cursor.is_none() {
            return Err(Error::TransportNotConfigured);
        }
        Ok(Self {
            rate_owner: crate::channel_mute::capture_prefs_cooldown(s)
                .ok_or(Error::TransportNotConfigured)?,
            api_owner: api::ReadShortcutOwner::capture(s.core.account_epoch, &session)?,
            owner: owner.clone(),
            epoch: s.core.account_epoch,
            rt_generation: s.core.workspaces[&owner.team].rt_generation,
            session,
            client: s.core.client.clone(),
            transport,
            cursor,
            cookie: s.core.session.as_ref().map(|s| s.d_cookie.clone()),
            channel_generation: s.channel_generation,
            main_view: s.main_view,
        })
    }
    pub fn current(&self, s: &ShellState) -> bool {
        if Owner::capture(s).as_ref() != Some(&self.owner)
            || s.core.account_epoch != self.epoch
            || s.core
                .workspaces
                .get(&self.owner.team)
                .is_none_or(|w| w.rt_generation != self.rt_generation)
        {
            return false;
        }
        if self.transport.is_none() {
            return fixture::enabled(s)
                && self
                    .api_owner
                    .current(s.core.account_epoch, &fixture::session(s));
        }
        self.cursor.as_ref().is_some_and(|c| c.current(&s.core))
            && s.core.session.as_ref().is_some_and(|session| {
                Some(&session.d_cookie) == self.cookie.as_ref()
                    && session.workspaces.get(&self.owner.team).is_some_and(|w| {
                        self.api_owner.current(s.core.account_epoch, w)
                            && w.token == self.session.token
                            && w.enterprise_id == self.session.enterprise_id
                    })
            })
    }
    pub fn visit_current(&self, s: &ShellState) -> bool {
        self.current(s)
            && s.channel_generation == self.channel_generation
            && s.main_view == self.main_view
            && s.overlay.is_none()
            && s.app_modal.is_none()
            && s.signed_in
            && !s.loading
            && s.profile_user.is_none()
            && s.group_panel.is_none()
            && s.profile_hover.is_none()
    }
}
pub(super) enum Next {
    Preference(PrefRead),
    Bulk(Work),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Clear,
    Undo,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Sending,
    Unknown,
}
pub(super) struct Operation {
    pub id: u64,
    pub context: Arc<Context>,
    pub kind: Kind,
    pub phase: Phase,
}
pub(crate) struct Work {
    pub id: u64,
    pub context: Arc<Context>,
    pub kind: Kind,
    pub request: PreparedRequest,
    pub baseline: Arc<reconcile::Baseline>,
}
#[derive(Default)]
pub(super) struct Entry {
    pub operation: Option<Operation>,
    pub preference: Option<PrefOperation>,
    pub notice: Option<Notice>,
    pub message: Option<String>,
    pub hidden: bool,
    pub reconciling: Option<u64>,
    pub refresh_failed: bool,
    pub acknowledged: bool,
    pub preference_message: Option<String>,
}
pub(super) struct Notice {
    pub context: Arc<Context>,
    pub receipt: Option<api::BulkReadReceipt>,
    pub started: Instant,
    pub duration: Duration,
    pub undone: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogPhase {
    Loading,
    Confirm,
    Unavailable,
}
pub(super) struct Dialog {
    pub id: u64,
    pub context: Arc<Context>,
    pub preference: ClearUnreadsPreference,
    pub phase: DialogPhase,
    pub dont_ask: bool,
    pub error: Option<String>,
}
#[derive(Clone)]
pub(crate) struct PrefRead {
    pub id: u64,
    pub context: Arc<Context>,
    pub observed: ClearUnreadsPreference,
}
#[derive(Clone)]
pub(crate) struct PrefWork {
    pub id: u64,
    pub context: Arc<Context>,
    pub baseline: ClearUnreadsPreference,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PrefPhase {
    Preparing,
    Sending,
    Unknown,
}
pub(super) struct PrefOperation {
    pub id: u64,
    pub phase: PrefPhase,
    pub message: Option<String>,
}
pub(super) fn observed(s: &ShellState, c: &Context) -> ClearUnreadsPreference {
    s.core.workspaces[&c.owner.team]
        .clear_unreads_preference
        .clone()
}
pub(super) fn preference_pending(s: &ShellState, owner: &Owner) -> bool {
    s.bulk_read
        .entries
        .get(owner)
        .is_some_and(|e| e.preference.is_some())
}
pub(super) fn cursor_busy(s: &ShellState, team: &str) -> bool {
    s.bulk_read.individuals.values().any(|(account, t, user)| {
        account == &s.core.active_account
            && t == team
            && s.core
                .workspaces
                .get(team)
                .is_some_and(|w| &w.self_user_id == user)
    }) || s.core.pending_marks.iter().any(|(target, _)| match target {
        super_platinum_core::domain::ReadTarget::Conversation { team: t, .. }
        | super_platinum_core::domain::ReadTarget::Thread { team: t, .. } => t == team,
    }) || crate::unread::busy_workspace(s, s.core.account_epoch, team)
        || crate::thread_unread::busy_workspace(s, s.core.account_epoch, team)
        || crate::read_shortcuts::busy_workspace(s, team)
}
