//! Request-bound facts for the captured member picker's includeAgents branch.
//! An app association alone is not an agent grant. Callers retain their own
//! member exclusions, account ownership and query/adoption fences.
use super::{app_id, flag, id, invalid, success};
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, models::User},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub mod slackbot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    app: String,
    bot: Option<String>,
    agent: bool,
    agentforce: bool,
}
impl Profile {
    pub fn decode(app: &str, response: &Value) -> Result<Self, Error> {
        success(response)?;
        Self::raw(app, response.get("app_profile").ok_or_else(invalid)?)
    }
    fn raw(app: &str, raw: &Value) -> Result<Self, Error> {
        if !id(app) || raw.get("id").and_then(Value::as_str) != Some(app) {
            return Err(invalid());
        }
        let bot = match raw.get("bot_user") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|v| id(v))
                    .ok_or_else(invalid)?
                    .to_owned(),
            ),
        };
        let agentforce = match raw.get("agentforce") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => false,
            Some(value) if value.is_object() => true,
            _ => return Err(invalid()),
        };
        Ok(Self {
            app: app.into(),
            bot,
            agent: flag(raw, "is_agent_app", false)?,
            agentforce,
        })
    }
    pub fn app(&self) -> &str {
        &self.app
    }
    pub fn agent_for(&self, user: &User) -> bool {
        self.agent && app_id(user) == Some(self.app.as_str())
    }
}

/// VLYq gates the full list request, independently from profile lookup. A
/// skipped list does not invent accessible IDs; the default-agent exception
/// still requires the exact loaded profile and settings below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMode {
    Fetch,
    Skip,
}
pub fn list_mode(assignments: &Value, current_user: &User) -> Result<ListMode, Error> {
    success(assignments)?;
    let rows = assignments
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let enabled = |key: &str| -> Result<bool, Error> {
        match rows.get(key) {
            None => Ok(false),
            Some(v) => v
                .get("group")
                .and_then(Value::as_str)
                .map(|v| v == "on")
                .ok_or_else(invalid),
        }
    };
    Ok(
        if current_user.is_restricted
            || enabled("ai_apps_list_fe_killswitch")?
            || enabled("support_lots_of_agents")?
        {
            ListMode::Skip
        } else {
            ListMode::Fetch
        },
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    disabled: BTreeSet<String>,
    default: Option<String>,
}
impl Settings {
    /// Actual o1P2 chooses locked organization preferences for an enterprise,
    /// otherwise the current team's preferences. Default agent always comes
    /// from that current team's preference (QwsR.wL), not org/unlocked prefs.
    pub fn decode(team_prefs: &Value, enterprise_locked: Option<&Value>) -> Result<Self, Error> {
        if !team_prefs.is_object() || enterprise_locked.is_some_and(|v| !v.is_object()) {
            return Err(invalid());
        }
        let prefs = enterprise_locked.unwrap_or(team_prefs);
        let mut disabled = BTreeSet::new();
        if let Some(settings) = prefs.get("ai_apps_settings") {
            if !settings.is_null() {
                let rows = settings
                    .get("apps")
                    .and_then(Value::as_array)
                    .ok_or_else(invalid)?;
                if rows.len() > 4096 {
                    return Err(invalid());
                }
                // Source builds an app-id map; later duplicate entries win.
                let mut by_id = BTreeMap::new();
                for row in rows {
                    let app = row
                        .get("app_id")
                        .and_then(Value::as_str)
                        .filter(|v| id(v))
                        .ok_or_else(invalid)?;
                    by_id.insert(app.to_owned(), flag(row, "is_ai_enabled", true)?);
                }
                disabled.extend(
                    by_id
                        .into_iter()
                        .filter_map(|(app, enabled)| (!enabled).then_some(app)),
                );
            }
        }
        let default = match team_prefs.get("default_channel_knowledge_agent") {
            None | Some(Value::Null) => None,
            Some(v) => match v.get("app_id") {
                None => None,
                Some(Value::String(v)) if v.is_empty() => None,
                Some(Value::String(v)) if id(v) => Some(v.clone()),
                _ => return Err(invalid()),
            },
        };
        Ok(Self { disabled, default })
    }
}

/// A completed source list, or its explicit source-gated empty accessibility
/// state. No partial page is an authoritative complete list. Profiles obtained
/// independently can supply the source's default-agent exception.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    accessible: BTreeSet<String>,
    settings: Settings,
    profiles: BTreeMap<String, Profile>,
}
impl Access {
    pub fn new(
        settings: Settings,
        accessible: BTreeSet<String>,
        profiles: BTreeMap<String, Profile>,
    ) -> Self {
        Self {
            settings,
            accessible,
            profiles,
        }
    }
    pub fn profile(&self, app: &str) -> Option<&Profile> {
        self.profiles.get(app)
    }
    pub fn allows(&self, user: &User, additional: Option<&Profile>) -> bool {
        self.profiles.values().chain(additional).any(|profile| {
            profile.agentforce
                && profile.bot.as_deref() == Some(user.id.as_str())
                && !self.settings.disabled.contains(&profile.app)
                && (self.accessible.contains(&profile.app)
                    || self.settings.default.as_deref() == Some(profile.app.as_str()))
        })
    }
}

#[derive(Clone, Debug)]
pub struct Page {
    pub profiles: Vec<Profile>,
    pub accessible: BTreeSet<String>,
    pub cursor: Option<String>,
}
impl Page {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        success(raw)?;
        let apps = raw
            .get("apps")
            .and_then(Value::as_array)
            .filter(|v| v.len() <= 1000)
            .ok_or_else(invalid)?;
        let mut profiles = Vec::with_capacity(apps.len());
        let mut accessible = BTreeSet::new();
        for raw in apps {
            let app = raw.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
            let profile = Profile::raw(app, raw)?;
            if profile.agentforce {
                accessible.insert(app.to_owned());
            }
            profiles.push(profile);
        }
        let cursor = match raw.pointer("/response_metadata/next_cursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(v)) if v.is_empty() => None,
            Some(Value::String(v)) if v.len() <= 4096 && !v.chars().any(char::is_control) => {
                Some(v.clone())
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            profiles,
            accessible,
            cursor,
        })
    }
}

pub fn list_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    cursor: Option<&str>,
) -> Result<PreparedRequest, Error> {
    if cursor.is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control)) {
        return Err(invalid());
    }
    let mut form = vec![
        ("limit", "20".into()),
        ("_x_reason", "native-calendar-attendee-agents".into()),
    ];
    if let Some(cursor) = cursor {
        form.push(("cursor", cursor.into()));
    }
    Ok(client.rest_form(workspace, "aiApps.list", form))
}
pub fn boot_request(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "client.init",
        vec![
            ("include_relevant_onboarding", "false".into()),
            ("_x_reason", "native-calendar-attendee-agents".into()),
        ],
    )
}
pub fn preferences_request(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "client.extras",
        vec![
            ("extras", json!(["team_prefs"]).to_string()),
            ("_x_reason", "native-calendar-attendee-agents".into()),
        ],
    )
}
pub fn enterprise_request(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "enterprise.prefs.get",
        vec![("_x_reason", "native-calendar-attendee-agents".into())],
    )
}

/// Captured messaging boot supplies exact self/team identity. This is the
/// enterprise settings scope, not the independent unified-user feature flag.
pub fn boot(workspace: &WorkspaceSession, raw: &Value) -> Result<(User, bool), Error> {
    success(raw)?;
    if raw.pointer("/self/id").and_then(Value::as_str) != Some(&workspace.user_id)
        || raw.pointer("/team/id").and_then(Value::as_str) != Some(&workspace.team_id)
    {
        return Err(invalid());
    }
    let user = serde_json::from_value(raw.get("self").cloned().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let enterprise = workspace.team_id.starts_with('E')
        || match raw.pointer("/team/enterprise_id") {
            None | Some(Value::Null) => false,
            Some(Value::String(v)) if v.is_empty() => false,
            Some(Value::String(v)) if id(v) => true,
            _ => return Err(invalid()),
        };
    Ok((user, enterprise))
}
pub fn settings(extras: &Value, enterprise: Option<&Value>) -> Result<Settings, Error> {
    success(extras)?;
    let team = extras
        .pointer("/team_prefs/toplevel_prefs")
        .ok_or_else(invalid)?;
    let locked = if let Some(raw) = enterprise {
        success(raw)?;
        Some(raw.get("locked_prefs").ok_or_else(invalid)?)
    } else {
        None
    };
    Settings::decode(team, locked)
}

#[cfg(test)]
mod tests;
