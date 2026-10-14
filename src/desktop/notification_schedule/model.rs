use super::*;
use std::hash::{Hash, Hasher};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Owner {
    pub account: String,
    pub team: String,
    pub user: String,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub owner: Owner,
    epoch: u64,
    pub transport: usize,
    pub credential: u64,
    pub rate_owner: crate::channel_mute::PrefsCooldownToken,
}
impl Token {
    pub fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.as_ref()?;
        let w = s.core.workspaces.get(team)?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        s.core
            .session
            .as_ref()
            .map(|s| s.d_cookie.as_str())
            .hash(&mut hash);
        s.core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team))
            .map(|w| (&w.team_id, &w.user_id, &w.token, &w.url))
            .hash(&mut hash);
        Some(Self {
            owner: Owner {
                account: s.core.active_account.clone()?,
                team: team.clone(),
                user: w.self_user_id.clone(),
            },
            epoch: s.core.account_epoch,
            transport: s
                .core
                .transport
                .as_ref()
                .map_or(0, |t| Arc::as_ptr(t) as usize),
            credential: hash.finish(),
            rate_owner: crate::channel_mute::capture_prefs_cooldown(s)?,
        })
    }
    pub fn current(&self, s: &ShellState) -> bool {
        Self::capture(s).as_ref() == Some(self)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Preparing,
    Saving,
    Checking,
    Uncertain,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub id: u64,
    pub visit: u64,
    pub token: Token,
    pub intent: Arc<api::NotificationScheduleIntent>,
    pub observed: BTreeMap<String, serde_json::Value>,
    pub phase: Phase,
    pub read: Option<u64>,
    pub context: Option<Arc<crate::scheduled::Context>>,
}
#[derive(Clone, Default)]
pub(super) struct Entry {
    pub pending: Option<Attempt>,
    pub message: Option<String>,
}
#[derive(Clone)]
pub(super) struct View {
    pub visit: u64,
    pub token: Token,
    pub snapshot: Option<api::NotificationSchedule>,
    pub loading: bool,
    pub error: Option<String>,
    pub read: u64,
}
#[derive(Clone)]
pub(super) struct Read {
    pub id: u64,
    pub visit: u64,
    pub token: Token,
    pub operation: Option<u64>,
    pub observed: BTreeMap<String, serde_json::Value>,
    pub context: Option<Arc<crate::scheduled::Context>>,
}
pub(super) fn context(s: &ShellState, t: &Token) -> Option<Arc<crate::scheduled::Context>> {
    let c = crate::scheduled::Context::capture(s)?;
    (c.session.team_id == t.owner.team && c.session.user_id == t.owner.user).then(|| Arc::new(c))
}
pub(super) fn session(
    s: &ShellState,
    t: &Token,
) -> Option<super_platinum_core::config::WorkspaceSession> {
    context(s, t).map(|c| c.session.clone()).or_else(|| {
        (fixture::enabled(s) || cfg!(test)).then(|| super_platinum_core::config::WorkspaceSession {
            team_id: t.owner.team.clone(),
            user_id: t.owner.user.clone(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-token".into(),
        })
    })
}
pub(super) fn uncertain(e: &Error) -> bool {
    match e {
        Error::Offline(_)
        | Error::TransportNotConfigured
        | Error::MissingWorkspace(_)
        | Error::RateLimited { .. } => false,
        Error::Api(c) => {
            c.starts_with("decode_")
                || matches!(
                    c.as_str(),
                    "internal_error" | "fatal_error" | "service_unavailable"
                )
        }
        Error::HttpStatus { status, .. } => *status >= 500 || *status == 408,
        _ => true,
    }
}
