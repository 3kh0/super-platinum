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
    pub epoch: u64,
    pub transport: usize,
    pub credential: u64,
    credentials: Credentials,
    pub rate_owner: crate::channel_mute::PrefsCooldownToken,
}
// Retained only in memory, never Debug/Serialize. The hash is a render key;
// admission compares the exact credential fields, not their hash alone.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Credentials {
    cookie: Option<String>,
    workspace: Option<(String, String, Option<String>, String, String)>,
}
impl Token {
    pub fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.as_ref()?;
        let w = s.core.workspaces.get(team)?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let credentials = Credentials {
            cookie: s.core.session.as_ref().map(|s| s.d_cookie.clone()),
            workspace: s
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(team))
                .map(|w| {
                    (
                        w.team_id.clone(),
                        w.user_id.clone(),
                        w.enterprise_id.clone(),
                        w.token.clone(),
                        w.url.clone(),
                    )
                }),
        };
        credentials.hash(&mut hash);
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
            credentials,
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
    pub intent: Arc<api::ReadPreferenceIntent>,
    pub observed: Option<ReadPreferences>,
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
    pub snapshot: Option<ReadPreferences>,
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
    pub observed: Option<ReadPreferences>,
    pub context: Option<Arc<crate::scheduled::Context>>,
}
pub(super) fn context(s: &ShellState, t: &Token) -> Option<Arc<crate::scheduled::Context>> {
    // Every offline fixture is isolated, even if a real session exists elsewhere.
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return None;
    }
    let c = crate::scheduled::Context::capture(s)?;
    (t.current(s) && c.session.team_id == t.owner.team && c.session.user_id == t.owner.user)
        .then(|| Arc::new(c))
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
// Only known unsent outcomes or narrowly understood rejections release the lock.
// Unknown API codes, malformed acknowledgements and post-dispatch failures retain it.
pub(super) fn definitely_rejected(e: &Error) -> bool {
    matches!(
        e,
        Error::Offline(_)
            | Error::TransportNotConfigured
            | Error::MissingWorkspace(_)
            | Error::RateLimited { .. }
    ) || matches!(e, Error::Api(c) if matches!(c.as_str(), "invalid_auth" | "not_authed" | "token_revoked" | "token_expired" | "account_inactive" | "missing_scope" | "invalid_arguments" | "invalid_arg_name"))
}
