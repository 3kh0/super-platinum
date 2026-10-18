//! Ordinary installed-app actions from HjDz/ZbBS, not Slash or workflow links.
pub mod calendar;
pub mod grid;
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Global,
    Message,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub app: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub app_name: String,
    pub kind: Kind,
    pub teams: Option<Vec<String>>,
    pub workspace_agnostic: bool,
    /// Exact catalog records survive projection and are compared at admission.
    pub raw_app: Arc<Value>,
    pub raw_action: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub actions: Vec<Action>,
    pub raw: Value,
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}
fn invalid() -> Error {
    Error::Api("app_shortcut_invalid".into())
}
fn text(raw: &Value, key: &str) -> Option<String> {
    let s = raw.get(key)?.as_str()?;
    (s.len() <= 4096 && !s.chars().any(|c| c.is_control() && c != '\n')).then(|| s.to_owned())
}
impl Catalog {
    pub fn decode(raw: Value) -> Result<Self, Error> {
        if raw.get("ok") != Some(&Value::Bool(true)) || raw.to_string().len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let apps = raw
            .get("app_actions")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if apps.len() > 2048 {
            return Err(invalid());
        }
        let mut actions = Vec::new();
        let mut seen = BTreeSet::new();
        for app in apps {
            let id = text(app, "app_id")
                .filter(|s| identifier(s))
                .ok_or_else(invalid)?;
            let teams = match app.get("teams") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|v| v.len() <= 128)
                        .ok_or_else(invalid)?
                        .iter()
                        .map(|t| {
                            t.as_str()
                                .filter(|s| identifier(s))
                                .map(str::to_owned)
                                .ok_or_else(invalid)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };
            let rows = app
                .get("actions")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?;
            let raw_app = Arc::new(app.clone());
            for row in rows {
                let kind = match row.get("type").and_then(Value::as_str) {
                    Some("global_action") => Kind::Global,
                    Some("message_action") => Kind::Message,
                    _ => continue,
                };
                let action_id = text(row, "action_id")
                    .filter(|s| identifier(s))
                    .ok_or_else(invalid)?;
                if !seen.insert((id.clone(), action_id.clone(), kind == Kind::Global))
                    || actions.len() >= 4096
                {
                    return Err(invalid());
                }
                let name = text(row, "name")
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(invalid)?;
                actions.push(Action {
                    app: id.clone(),
                    id: action_id,
                    name,
                    description: text(row, "description").unwrap_or_default(),
                    app_name: text(app, "app_name").unwrap_or_else(|| id.clone()),
                    kind,
                    teams: teams.clone(),
                    workspace_agnostic: app.get("is_workspace_agnostic")
                        == Some(&Value::Bool(true)),
                    raw_app: raw_app.clone(),
                    raw_action: row.clone(),
                });
            }
        }
        Ok(Self { actions, raw })
    }
}
impl Action {
    pub fn available_in_chooser(&self, related: &[String]) -> bool {
        self.eligible(related) || calendar::Provider::for_action(self).is_some()
    }
    /// Message actions use qGxq's intersection. Global tB4h returns no explicit
    /// target for workspace-agnostic actions; this is not a fabricated team ID.
    pub fn applicable_teams(&self, related: &[String]) -> Vec<String> {
        if self.kind == Kind::Global && self.workspace_agnostic {
            return vec![];
        }
        self.teams.as_ref().map_or_else(Vec::new, |teams| {
            teams
                .iter()
                .filter(|t| related.contains(t))
                .cloned()
                .collect()
        })
    }
    pub fn eligible(&self, related: &[String]) -> bool {
        // I5zS routes these first-party calendars to a separate createEvent flow.
        // They must never be sent to the ordinary v2 endpoint.
        let calendar = self.kind == Kind::Global
            && matches!(
                self.app.as_str(),
                "A0CNP2EP5"
                    | "ADZ494LHY"
                    | "A07H429LK"
                    | "AFV5ECLBZ"
                    | "A0608DQ3P"
                    | "A1S0QJ0SQ"
                    | "A012Z6404M8"
                    | "A01NK8UJRGC"
            );
        !calendar
            && (self.kind == Kind::Global
                || self.teams.is_none()
                || !self.applicable_teams(related).is_empty())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub channel: Option<String>,
    pub message_ts: Option<String>,
}
impl Context {
    pub fn validate(&self, kind: Kind) -> bool {
        self.channel.as_deref().is_none_or(identifier)
            && match kind {
                Kind::Global => self.message_ts.is_none(),
                Kind::Message => {
                    self.channel.is_some()
                        && self.message_ts.as_deref().is_some_and(|s| {
                            let Some((a, b)) = s.split_once('.') else {
                                return false;
                            };
                            !a.is_empty()
                                && !b.is_empty()
                                && a.bytes().chain(b.bytes()).all(|b| b.is_ascii_digit())
                        })
                }
            }
    }
}
pub fn discovery(client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        session,
        "client.appCommands",
        vec![("_x_reason", "app-commands-conditional-fetching".into())],
    )
}
/// Only a still-identical catalog choice can build the exact v2 request.
pub fn prepare(
    client: &SlackClient,
    session: &WorkspaceSession,
    catalog: &Catalog,
    action: &Action,
    context: &Context,
    related: &[String],
    team: Option<&str>,
    token: &str,
) -> Result<PreparedRequest, Error> {
    if !catalog.actions.contains(action)
        || !action.eligible(related)
        || !context.validate(action.kind)
        || !identifier(token)
    {
        return Err(invalid());
    }
    let teams = action.applicable_teams(related);
    if team.is_some_and(|t| !teams.iter().any(|s| s == t))
        || (action.kind == Kind::Global && !teams.is_empty() && team.is_none())
    {
        return Err(invalid());
    }
    // Message caller does not forward applicableTeamId. Global caller does.
    if action.kind == Kind::Message && team.is_some() {
        return Err(invalid());
    }
    let mut ctx = serde_json::Map::new();
    if let Some(v) = &context.channel {
        ctx.insert("channel_id".into(), json!(v));
    }
    if let Some(v) = &context.message_ts {
        ctx.insert("message_ts".into(), json!(v));
    }
    let mut fields = vec![
        ("action_id", action.id.clone()),
        ("app_id", action.app.clone()),
        ("context", Value::Object(ctx).to_string()),
        ("client_token", token.into()),
        (
            "_x_reason",
            "app-global-actions-api/execute-app-action".into(),
        ),
    ];
    if let Some(team) = team {
        fields.push(("team_id", team.into()));
    }
    Ok(client.rest_form(session, "apps.actions.v2.execute", fields))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Acknowledged,
    Restricted,
    Rejected,
    Unknown,
}
pub fn decode(raw: &Value) -> Outcome {
    match raw.get("ok").and_then(Value::as_bool) {
        Some(true) => Outcome::Acknowledged,
        Some(false)
            if raw.get("error").and_then(Value::as_str) == Some("app_access_restricted") =>
        {
            Outcome::Restricted
        }
        Some(false)
            if raw
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|e| e != "dispatch_failed" && !e.is_empty()) =>
        {
            Outcome::Rejected
        }
        _ => Outcome::Unknown,
    }
}

#[cfg(test)]
mod tests;
