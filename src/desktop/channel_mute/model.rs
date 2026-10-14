use super::*;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Owner {
    pub account: String,
    pub team: String,
    pub user: String,
    pub channel: String,
}
/// A prefs endpoint cooldown follows authentication, never the viewed channel.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RateOwner {
    account: String,
    team: String,
    user: String,
    credential: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(super) owner: Owner,
    epoch: u64,
    visit: u64,
    transport: usize,
    credential: u64,
    pub(super) operation: Option<u64>,
}
impl Token {
    pub(super) fn rate_owner(&self) -> RateOwner {
        RateOwner {
            account: self.owner.account.clone(),
            team: self.owner.team.clone(),
            user: self.owner.user.clone(),
            credential: self.credential,
        }
    }
    pub(super) fn capture(shell: &ShellState, channel: &str) -> Option<Self> {
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team));
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        shell
            .core
            .session
            .as_ref()
            .map(|s| s.d_cookie.as_str())
            .hash(&mut hasher);
        session
            .map(|s| {
                (
                    s.token.as_str(),
                    s.user_id.as_str(),
                    s.team_id.as_str(),
                    s.url.as_str(),
                )
            })
            .hash(&mut hasher);
        let owner = Owner {
            account: shell.core.active_account.clone()?,
            team: team.clone(),
            user: workspace.self_user_id.clone(),
            channel: channel.into(),
        };
        let operation = shell
            .channel_mute
            .entries
            .get(&owner)
            .and_then(|e| e.operation.as_ref())
            .map(|o| o.id);
        Some(Self {
            owner,
            operation,
            epoch: shell.core.account_epoch,
            visit: shell.channel_generation,
            transport: shell
                .core
                .transport
                .as_ref()
                .map_or(0, |t| Arc::as_ptr(t) as usize),
            credential: hasher.finish(),
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_channel.as_deref() == Some(&self.owner.channel)
            && Self::capture(shell, &self.owner.channel).is_some_and(|mut token| {
                token.operation = self.operation;
                token == *self
            })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Preparing,
    Saving,
    AwaitingRead,
    Checking,
    Uncertain,
}
#[derive(Clone)]
pub(super) struct Operation {
    pub id: u64,
    pub change: api::ChannelMuteChange,
    pub settings: Option<Arc<api::ChannelNotificationIntent>>,
    pub phase: Phase,
    pub read_id: Option<u64>,
}
#[derive(Clone, Default)]
pub(super) struct Entry {
    pub operation: Option<Operation>,
    pub message: Option<String>,
    pub retry_at: i64,
    pub settled: Option<(api::ChannelMuteState, bool)>,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub id: u64,
    pub token: Token,
    pub desired: bool,
    pub change: api::ChannelMuteChange,
    pub settings: Option<Arc<api::ChannelNotificationIntent>>,
    pub context: Option<Arc<Context>>,
    pub baseline: Option<std::collections::BTreeMap<String, Value>>,
    pub schedule: Option<std::collections::BTreeMap<String, Value>>,
}
#[derive(Clone)]
pub(super) struct Read {
    pub id: u64,
    pub operation: u64,
    pub token: Token,
    pub context: Option<Arc<Context>>,
    pub baseline: Option<std::collections::BTreeMap<String, Value>>,
}
pub(super) fn context(shell: &ShellState, token: &Token) -> Option<Arc<Context>> {
    let context = Context::capture(shell)?;
    (context.session.user_id == token.owner.user
        && context.session.team_id == token.owner.team
        && !context.session.token.is_empty())
    .then(|| Arc::new(context))
}
pub(super) fn eligible(shell: &ShellState, channel: &str) -> bool {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .and_then(|w| w.channels.get(channel))
        .is_some_and(|c| {
            !c.is_archived
                && !c.is_im
                && !c.is_mpim
                && (c.is_channel || c.is_private || c.is_group)
                && c.extra.get("is_member").and_then(Value::as_bool) == Some(true)
        })
}
pub(super) fn target(
    shell: &ShellState,
    owner: &Owner,
) -> Option<std::collections::BTreeMap<String, Value>> {
    shell
        .core
        .workspaces
        .get(&owner.team)?
        .notifications
        .channels
        .get(&owner.channel)
        .cloned()
}
pub(super) fn cooldown(error: &Error, now: i64) -> i64 {
    let seconds = match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => Some(retry_after_secs.unwrap_or(30)),
        Error::Api(code) if matches!(code.as_str(), "ratelimited" | "rate_limited") => Some(30),
        _ => None,
    };
    seconds.map_or(0, |seconds| {
        now.saturating_add(i64::try_from(seconds.max(1)).unwrap_or(i64::MAX))
    })
}
