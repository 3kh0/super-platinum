use super::*;
use std::hash::{Hash, Hasher};

pub(super) const FIELDS: [Field; 3] = [Field::FullName, Field::DisplayName, Field::Pronouns];
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Owner {
    pub account: String,
    pub team: String,
    pub user: String,
}
impl Owner {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let account = shell.core.active_account.clone()?;
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        (!workspace.self_user_id.is_empty() && shell.self_account.user_id == workspace.self_user_id)
            .then(|| Self {
                account,
                team: team.clone(),
                user: workspace.self_user_id.clone(),
            })
    }
    pub fn values(&self, shell: &ShellState) -> Values {
        Values::from_profile(
            shell
                .core
                .workspaces
                .get(&self.team)
                .and_then(|w| w.users.get(&self.user))
                .and_then(|u| u.profile.as_ref()),
        )
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub owner: Owner,
    epoch: u64,
    profile_visit: u64,
    transport: usize,
    credential: u64,
}
impl Token {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let owner = Owner::capture(shell)?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        shell
            .core
            .session
            .as_ref()
            .map(|s| s.d_cookie.as_str())
            .hash(&mut hash);
        shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&owner.team))
            .map(|s| (&s.team_id, &s.user_id, &s.token, &s.url))
            .hash(&mut hash);
        Some(Self {
            owner,
            epoch: shell.core.account_epoch,
            profile_visit: shell.profile_generation,
            transport: shell
                .core
                .transport
                .as_ref()
                .map_or(0, |t| Arc::as_ptr(t) as usize),
            credential: hash.finish(),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).as_ref() == Some(self)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Values(pub [String; 3]);
impl Values {
    pub fn from_profile(profile: Option<&UserProfile>) -> Self {
        Self([
            profile
                .and_then(|p| p.real_name.clone())
                .unwrap_or_default(),
            profile
                .and_then(|p| p.display_name.clone())
                .unwrap_or_default(),
            profile.and_then(|p| p.pronouns.clone()).unwrap_or_default(),
        ])
    }
    pub fn get(&self, field: Field) -> &str {
        &self.0[index(field)]
    }
    pub fn patch(&self, baseline: &Self) -> Value {
        Value::Object(
            FIELDS
                .into_iter()
                .filter(|f| self.get(*f) != baseline.get(*f))
                .map(|f| (f.key().to_owned(), Value::String(self.get(f).to_owned())))
                .collect(),
        )
    }
}
pub(super) fn index(field: Field) -> usize {
    match field {
        Field::FullName => 0,
        Field::DisplayName => 1,
        Field::Pronouns => 2,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Preparing,
    Saving,
    Uncertain,
    Checking,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub id: u64,
    pub visit: u64,
    pub token: Token,
    pub baseline: Values,
    pub observed: Values,
    pub desired: Values,
    pub patch: Value,
    pub phase: Phase,
    pub write_finished: bool,
    pub read_id: Option<u64>,
    pub context: Option<Arc<crate::scheduled::Context>>,
}
#[derive(Clone, Default)]
pub(super) struct Entry {
    pub pending: Option<Attempt>,
    pub retry_at: i64,
    pub message: Option<String>,
    pub checked: bool,
}
#[derive(Clone)]
pub(super) struct Draft {
    pub token: Token,
    pub visit: u64,
    pub observed: Values,
    pub baseline: Values,
    pub values: Values,
    pub capabilities: Option<api::FreshOwnProfileCapabilities>,
    pub loading: bool,
    pub error: Option<String>,
}
pub(super) fn context(shell: &ShellState, token: &Token) -> Option<Arc<crate::scheduled::Context>> {
    let context = crate::scheduled::Context::capture(shell)?;
    (context.session.team_id == token.owner.team && context.session.user_id == token.owner.user)
        .then(|| Arc::new(context))
}
pub(super) fn cooldown(error: &Error) -> i64 {
    let secs = match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => retry_after_secs.unwrap_or(30).max(1),
        Error::Api(code) if matches!(code.as_str(), "ratelimited" | "rate_limited") => 30,
        _ => return 0,
    };
    now().saturating_add(i64::try_from(secs).unwrap_or(i64::MAX))
}
pub(super) fn uncertain(error: &Error) -> bool {
    match error {
        Error::TransportNotConfigured
        | Error::MissingWorkspace(_)
        | Error::Offline(_)
        | Error::RateLimited { .. } => false,
        Error::Api(code) => {
            matches!(
                code.as_str(),
                "internal_error" | "fatal_error" | "service_unavailable"
            ) || code.starts_with("decode_")
        }
        Error::HttpStatus { status, .. } => *status >= 500 || *status == 408,
        _ => true,
    }
}
pub(super) fn validation(error: profile_text::ValidationError) -> &'static str {
    use profile_text::ValidationError::*;
    match error {
        Required => "Enter your full name.",
        TooLong { .. } => "Use 80 characters or fewer.",
        LeadingAt => "Names cannot start with @.",
        RestrictedCharacter => "This name contains a character Slack does not allow.",
        ReservedName => "Choose a different name; this name is reserved.",
        PunctuationOnly => "Include a letter or number in this name.",
        Emoji => "Names cannot contain emoji.",
    }
}
