use super::*;
use super_platinum_core::block_actions::form::{datetime_local_value, parse_datetime_local};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Expiration {
    ThirtyMinutes,
    OneHour,
    FourHours,
    Today,
    Never,
    Custom,
}
impl Expiration {
    pub const ALL: [Self; 6] = [
        Self::ThirtyMinutes,
        Self::OneHour,
        Self::FourHours,
        Self::Today,
        Self::Never,
        Self::Custom,
    ];
    pub fn token(self) -> &'static str {
        match self {
            Self::ThirtyMinutes => "30_minutes",
            Self::OneHour => "1_hour",
            Self::FourHours => "4_hours",
            Self::Today => "all_day",
            Self::Never => "no_expiration",
            Self::Custom => "custom",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ThirtyMinutes => "30 minutes",
            Self::OneHour => "1 hour",
            Self::FourHours => "4 hours",
            Self::Today => "Today",
            Self::Never => "Don't clear",
            Self::Custom => "Custom",
        }
    }
    pub fn from_token(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.token() == value)
    }
    pub(super) fn timestamp(self, custom: &str, now: i64) -> Result<i64, &'static str> {
        let value = match self {
            Self::Never => return Ok(0),
            Self::ThirtyMinutes => now.checked_add(1800),
            Self::OneHour => now.checked_add(3600),
            Self::FourHours => now.checked_add(14400),
            Self::Today => {
                // Captured getDurationPresetTs uses local moment().endOf('day').
                // The existing local-time parser rejects nonexistent/ambiguous times.
                let local = datetime_local_value(Some(now));
                let day = local.get(..10).ok_or("Could not determine today's date.")?;
                parse_datetime_local(&format!("{day}T23:59"))
                    .ok()
                    .flatten()
                    .and_then(|value| value.checked_add(59))
            }
            Self::Custom => parse_datetime_local(custom)
                .map_err(|_| "Choose a valid, unambiguous local date and time.")?,
        }
        .ok_or("Choose a valid date and time.")?;
        (value > now)
            .then_some(value)
            .ok_or("Choose a future date and time.")
    }
}

#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(Debug))]
pub(super) struct Status {
    pub text: String,
    pub emoji: String,
    pub expiration: i64,
}
impl Status {
    pub fn from_profile(profile: Option<&UserProfile>) -> Self {
        Self {
            text: profile
                .and_then(|p| p.status_text.clone())
                .unwrap_or_default(),
            emoji: profile
                .and_then(|p| p.status_emoji.clone())
                .unwrap_or_default(),
            expiration: profile
                .and_then(|p| p.status_expiration)
                .unwrap_or_default(),
        }
    }
    pub fn effective(mut self, now: i64) -> Self {
        if self.expiration > 0 && self.expiration <= now {
            self = Self::default();
        }
        self
    }
    pub fn merge(&self, profile: &mut UserProfile) {
        profile.status_text = Some(self.text.clone());
        profile.status_emoji = Some(self.emoji.clone());
        profile.status_expiration = Some(self.expiration);
    }
}

#[derive(Clone)]
pub(super) struct Scope {
    epoch: u64,
    team: String,
    user: String,
    transport: Option<Arc<super_platinum_core::slack::Transport>>,
    session: Option<super_platinum_core::config::WorkspaceSession>,
}
impl Scope {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        if workspace.self_user_id.is_empty() || workspace.self_user_id != shell.self_account.user_id
        {
            return None;
        }
        Some(Self {
            epoch: shell.core.account_epoch,
            team: team.clone(),
            user: workspace.self_user_id.clone(),
            transport: shell.core.transport.clone(),
            session: shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(team))
                .cloned(),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&self.team));
        self.epoch == shell.core.account_epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.self_account.user_id == self.user
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.team_id == self.team && w.self_user_id == self.user)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&self.session, session) {
                (Some(a), Some(b)) => {
                    a.team_id == b.team_id
                        && a.user_id == b.user_id
                        && a.token == b.token
                        && a.url == b.url
                }
                (None, None) => true,
                _ => false,
            }
    }
    pub fn status(&self, shell: &ShellState) -> Status {
        Status::from_profile(
            shell
                .core
                .workspaces
                .get(&self.team)
                .and_then(|w| w.users.get(&self.user))
                .and_then(|u| u.profile.as_ref()),
        )
    }
    pub fn merge(&self, shell: &mut ShellState, status: &Status) {
        if let Some(user) = shell
            .core
            .workspaces
            .get_mut(&self.team)
            .and_then(|w| w.users.get_mut(&self.user))
        {
            status.merge(user.profile.get_or_insert_with(Default::default));
        }
    }
}

#[derive(Clone)]
pub(super) struct Draft {
    pub scope: Scope,
    pub revision: u64,
    pub text: String,
    pub emoji: String,
    pub expiration: Expiration,
    pub custom_datetime: String,
    pub original_expiration: Option<i64>,
    pub error: Option<String>,
    pub phase: Phase,
    pub retry_at: i64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Editing,
    Saving,
    Uncertain,
    Checking,
}
impl Draft {
    pub fn open(shell: &ShellState, now: i64) -> Option<Self> {
        let scope = Scope::capture(shell)?;
        let status = scope.status(shell).effective(now);
        let has_status = !status.text.is_empty() || !status.emoji.is_empty();
        let expiration = if status.expiration > now {
            Expiration::Custom
        } else if has_status {
            Expiration::Never
        } else {
            Expiration::Today
        };
        Some(Self {
            scope,
            revision: next_id(),
            text: status.text,
            emoji: status.emoji,
            expiration,
            custom_datetime: datetime_local_value(Some(if status.expiration > now {
                status.expiration
            } else {
                now + 3600
            })),
            original_expiration: (status.expiration > now).then_some(status.expiration),
            error: None,
            phase: Phase::Editing,
            retry_at: 0,
        })
    }
    pub fn profile(&self, now: i64) -> Result<(Value, Status), &'static str> {
        if self.text.encode_utf16().count() > 100 {
            return Err("Status text must be 100 characters or fewer.");
        }
        if self.text.contains(['\n', '\r']) {
            return Err("Use a single line for your status.");
        }
        let emoji = self.emoji.trim();
        if emoji.len() > 256
            || emoji.chars().any(|c| c.is_whitespace() || c.is_control())
            || (!emoji.is_empty()
                && (!emoji.starts_with(':') || !emoji.ends_with(':') || emoji.len() < 3))
        {
            return Err("Choose an emoji from the picker.");
        }
        let blank = self.text.is_empty() && emoji.is_empty();
        let expiration = if blank {
            0
        } else if self.expiration == Expiration::Custom {
            self.original_expiration
                .filter(|value| {
                    datetime_local_value(Some(*value)) == self.custom_datetime && *value > now
                })
                .map(Ok)
                .unwrap_or_else(|| self.expiration.timestamp(&self.custom_datetime, now))?
        } else {
            self.expiration.timestamp(&self.custom_datetime, now)?
        };
        let status = Status {
            text: self.text.clone(),
            emoji: if emoji.is_empty() && !blank {
                ":speech_balloon:".into()
            } else {
                emoji.into()
            },
            expiration,
        };
        // Captured updateCustomStatus clears stale canonical text on a typed edit.
        let profile = json!({"status_text":status.text,"status_emoji":status.emoji,"status_expiration":status.expiration,
            "status_text_canonical":"","status_duration":if blank {"no_expiration"}else{self.expiration.token()}});
        Ok((profile, status))
    }
}
