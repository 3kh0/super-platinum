//! Captured g5os/quL6/NGtd App Home identity; no renderer or network authority.
pub mod about;
pub mod calendar;
pub mod navigation;
pub mod workflow;
use crate::config::WorkspaceSession;
use crate::slack::{Error, PreparedRequest, SlackClient};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    pub channel: String,
    pub app: String,
    pub installed_team: String,
    pub enabled: bool,
    pub messages: bool,
    pub view_id: Option<String>,
}
#[derive(Clone, PartialEq)]
pub struct View {
    pub home: Home,
    raw: Value,
}
#[derive(Clone, PartialEq)]
pub struct Button {
    user: String,
    view: View,
    block: String,
    action_id: String,
    raw: Value,
    action: Value,
    state: Option<Value>,
}
fn invalid() -> Error {
    Error::Api("app_home_invalid".into())
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
fn text(v: &Value, k: &str) -> Result<String, Error> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| id(s))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn optional(v: &Value, k: &str) -> Result<Option<String>, Error> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => text(v, k).map(Some),
    }
}
impl Home {
    /// `channel` is the exact selected bot conversation, never taken from a button.
    pub fn decode(channel: &str, response: &Value) -> Result<(Self, Option<View>), Error> {
        if !id(channel) || response.get("ok") != Some(&json!(true)) {
            return Err(invalid());
        }
        let m = response
            .get("app_home")
            .filter(|v| v.is_object())
            .ok_or_else(invalid)?;
        for key in ["channel_id", "conversation_id"] {
            if let Some(v) = m.get(key) {
                if v.as_str() != Some(channel) {
                    return Err(invalid());
                }
            }
        }
        if let Some(c) = response.get("channel") {
            if c.get("id").and_then(Value::as_str) != Some(channel) {
                return Err(invalid());
            }
        }
        let home = Self {
            channel: channel.into(),
            app: text(m, "app_id")?,
            installed_team: text(m, "app_installed_team_id")?,
            enabled: m
                .get("home_tab_enabled")
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?,
            messages: m
                .get("messages_tab_enabled")
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?,
            view_id: optional(m, "home_view_id")?,
        };
        let view = match response.get("home_view") {
            None | Some(Value::Null) => None,
            Some(v) => Some(View::decode(&home, v.clone())?),
        };
        Ok((home, view))
    }
}
impl View {
    pub fn decode(home: &Home, raw: Value) -> Result<Self, Error> {
        if !home.enabled
            || raw.get("type").and_then(Value::as_str) != Some("home")
            || raw.get("id").and_then(Value::as_str) != home.view_id.as_deref()
            || raw.get("app_id").and_then(Value::as_str) != Some(&home.app)
            || raw.get("app_installed_team_id").and_then(Value::as_str)
                != Some(&home.installed_team)
            || raw
                .get("blocks")
                .and_then(Value::as_array)
                .is_none_or(|b| b.len() > 100)
            || optional(&raw, "bot_id").is_err()
        {
            return Err(invalid());
        }
        text(&raw, "id")?;
        Ok(Self {
            home: home.clone(),
            raw,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    /// Home inputs remain explicit unsupported content in this button-entry slice.
    /// Never dispatch a button with omitted state from an unimplemented control.
    pub fn has_controls(&self) -> bool {
        if self.raw.get("state").is_some_and(|s| {
            s.get("values")
                .and_then(Value::as_object)
                .is_none_or(|v| !v.is_empty())
        }) {
            return true;
        }
        self.raw["blocks"].as_array().unwrap().iter().any(|b| {
            if !matches!(
                b["type"].as_str(),
                Some(
                    "header"
                        | "section"
                        | "context"
                        | "image"
                        | "divider"
                        | "rich_text"
                        | "actions"
                )
            ) {
                return true;
            }
            let elems: Vec<&Value> = if b["type"] == "actions" {
                b["elements"]
                    .as_array()
                    .map(|a| a.iter().collect())
                    .unwrap_or_default()
            } else {
                b.get("accessory").into_iter().collect()
            };
            elems
                .iter()
                .any(|e| !matches!(e["type"].as_str(), Some("button" | "image")))
        })
    }
    pub fn button(&self, block: &str, action: &str, user: &str) -> Result<Button, Error> {
        self.capture_button(block, action, user, None)
    }
    pub fn button_with_form(
        &self,
        block: &str,
        action: &str,
        user: &str,
        form: &super::form::home::HomeForm,
    ) -> Result<Button, Error> {
        if form.raw_view() != &self.raw {
            return Err(invalid());
        }
        let state = form.state().map_err(|_| invalid())?;
        self.capture_button(block, action, user, Some(state))
    }
    fn capture_button(
        &self,
        block: &str,
        action: &str,
        user: &str,
        state: Option<Value>,
    ) -> Result<Button, Error> {
        if (state.is_none() && self.has_controls())
            || block.trim().is_empty()
            || action.trim().is_empty()
            || block.starts_with("widget:")
            || action == "calendar_hermes_connect"
        {
            return Err(invalid());
        }
        let mut blocks = self.raw["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["block_id"].as_str() == Some(block));
        let b = blocks.next().ok_or_else(invalid)?;
        if blocks.next().is_some() {
            return Err(invalid());
        }
        let elements: Vec<&Value> = match b["type"].as_str() {
            Some("actions") => b["elements"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .collect(),
            Some("section") => b.get("accessory").into_iter().collect(),
            _ => return Err(invalid()),
        };
        let mut found = elements
            .into_iter()
            .filter(|e| e["action_id"].as_str() == Some(action));
        let raw = found.next().ok_or_else(invalid)?;
        if found.next().is_some() {
            return Err(invalid());
        }
        super::validation::validate_button(raw, user).map_err(|_| invalid())?;
        let mut wire =
            json!({"type":"button","block_id":block,"action_id":action,"text":raw["text"]});
        for k in ["value", "style", "confirm"] {
            if let Some(v) = raw.get(k) {
                wire[k] = v.clone()
            }
        }
        Ok(Button {
            user: user.into(),
            view: self.clone(),
            block: block.into(),
            action_id: action.into(),
            raw: raw.clone(),
            action: wire,
            state,
        })
    }
}
impl Button {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn app_id(&self) -> &str {
        &self.view.home.app
    }
    pub fn action(&self) -> &Value {
        &self.action
    }
    pub fn same_action(&self, other: &Self) -> bool {
        self.user == other.user
            && self.view == other.view
            && self.block == other.block
            && self.action_id == other.action_id
    }
    pub fn same_form(&self, form: &super::form::home::HomeForm) -> bool {
        self.state.as_ref().is_none_or(|state| {
            form.raw_view() == &self.view.raw && form.state().as_ref().is_ok_and(|v| v == state)
        })
    }
    pub fn same_view(&self, v: &View) -> bool {
        &self.view == v
    }
    pub fn key(&self) -> (&str, &str) {
        (&self.block, &self.action_id)
    }
    pub fn prepare(
        &self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
        current: &View,
        token: &str,
    ) -> Result<PreparedRequest, Error> {
        if workspace.user_id != self.user
            || !self.same_view(current)
            || token.is_empty()
            || token.len() > 255
            || token.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(invalid());
        }
        let mut fields = vec![
            ("app_id", self.view.home.app.clone()),
            ("service_team_id", self.view.home.installed_team.clone()),
            (
                "container",
                json!({"type":"view","view_id":self.view.home.view_id}).to_string(),
            ),
            ("actions", json!([self.action]).to_string()),
            ("client_token", token.into()),
            ("_x_reason", "dispatch_action_to_developer".into()),
        ];
        if let Some(state) = &self.state {
            fields.push(("state", state.to_string()));
        }
        if let Some(bot) = optional(&self.view.raw, "bot_id")? {
            fields.push(("service_id", bot))
        }
        Ok(client.rest_form(workspace, "blocks.actions", fields))
    }
}
pub fn fetch(
    client: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
) -> Result<PreparedRequest, Error> {
    if !id(channel) {
        return Err(invalid());
    }
    Ok(client.rest_form(
        w,
        "conversations.info",
        vec![
            ("channel", channel.into()),
            ("return_app_home", "true".into()),
            ("_x_reason", "app-home-open-api".into()),
        ],
    ))
}
pub fn opened(
    client: &SlackClient,
    w: &WorkspaceSession,
    home: &Home,
) -> Result<PreparedRequest, Error> {
    if !home.enabled
        || ![
            home.channel.as_str(),
            home.app.as_str(),
            home.installed_team.as_str(),
        ]
        .into_iter()
        .all(id)
    {
        return Err(invalid());
    }
    Ok(client.rest_form(
        w,
        "apps.home.dispatchOpenEvent",
        vec![
            ("id", home.channel.clone()),
            ("type", "home".into()),
            ("_x_reason", "app-home-logging".into()),
            ("service_team_id", home.installed_team.clone()),
        ],
    ))
}
#[cfg(test)]
mod tests;
