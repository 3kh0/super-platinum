//! Captured GQES/XCcf About read and JYNE/JnPn user connection contracts.
//! These values carry no account, navigation, experiment or modal authority.
use super::*;
mod links;
mod management;
mod policy;
pub use management::WorkflowEntry;
pub use policy::{Flags, Profile};

/// fZcZ.G uses the actual apps_connection_status assignment, exactly `on`.
/// A complete response with no assignment is off; an unavailable response is
/// unknown and cannot admit a connection action.
pub fn connection_assignment(raw: &Value) -> Result<bool, Error> {
    if raw.get("ok") != Some(&json!(true)) || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(invalid());
    }
    let assignments = raw
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    match assignments.get("apps_connection_status") {
        None => Ok(false),
        Some(v) => v
            .get("group")
            .and_then(Value::as_str)
            .map(|group| group == "on")
            .ok_or_else(invalid),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    app: String,
    raw: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionGroup {
    pub heading: String,
    pub descriptions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionBlock {
    pub heading: String,
    pub groups: Vec<PermissionGroup>,
}

impl Info {
    pub fn request(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        app: &str,
    ) -> Result<PreparedRequest, Error> {
        if !id(app) {
            return Err(invalid());
        }
        Ok(client.rest_form(
            workspace,
            "apps.about.info",
            vec![
                ("app_id", app.into()),
                ("_x_reason", "fetch-unknown-apps-about-info".into()),
            ],
        ))
    }

    pub fn decode(app: &str, raw: Value) -> Result<Self, Error> {
        if !id(app)
            || raw.get("ok") != Some(&json!(true))
            || raw.get("app_id").and_then(Value::as_str) != Some(app)
            || serde_json::to_vec(&raw).map_or(true, |bytes| bytes.len() > 1024 * 1024)
        {
            return Err(invalid());
        }
        for key in [
            "name",
            "auth_full_name",
            "long_description",
            "assistant_description",
            "support_url",
            "home_url",
            "support_email",
            "privacy_link",
            "youtube_url",
        ] {
            if raw.get(key).is_some_and(|v| !v.is_null() && !v.is_string()) {
                return Err(invalid());
            }
        }
        for key in [
            "is_ai_app",
            "is_agent_app",
            "is_calls_app",
            "is_org_ready_app",
        ] {
            if raw
                .get(key)
                .is_some_and(|v| !v.is_null() && !v.is_boolean())
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            app: app.into(),
            raw,
        })
    }

    pub fn app(&self) -> &str {
        &self.app
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn text(&self, key: &str) -> Option<&str> {
        self.raw
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// PW6x: fixed secondary group order, separate required/optional blocks.
    /// The wire is snake_case; XCcf applies recursive camel casing in the web
    /// store. Read wire keys directly here without changing retained raw JSON.
    pub fn permissions(&self, kind: &str, optional: bool) -> Option<PermissionBlock> {
        if !matches!(kind, "content" | "actions") {
            return None;
        }
        let primary = self.raw.get("scope_info")?.get(kind)?;
        let secondary = primary.get("secondary_groups")?.as_object()?;
        let mut groups = Vec::new();
        for key in [
            "admin_actions",
            "channel_actions",
            "channel_content",
            "user_actions",
            "user_content",
            "workspace_actions",
            "workspace_content",
        ] {
            let Some(group) = secondary.get(key) else {
                continue;
            };
            let Some(scopes) = group.get("scopes_info").and_then(Value::as_array) else {
                continue;
            };
            let descriptions: Vec<_> = scopes
                .iter()
                .filter(|scope| {
                    scope
                        .get("is_optional")
                        .is_none_or(|v| v.is_null() || v.is_boolean())
                })
                .filter(|scope| {
                    scope
                        .get("is_optional")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        == optional
                })
                .filter_map(|scope| {
                    scope
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect();
            if !descriptions.is_empty() {
                groups.push(PermissionGroup {
                    heading: group
                        .get("heading")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    descriptions,
                });
            }
        }
        (!groups.is_empty()).then(|| PermissionBlock {
            heading: primary
                .get("heading")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            groups,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    app: String,
    raw: Value,
}
impl Connection {
    pub fn request(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        app: &str,
    ) -> Result<PreparedRequest, Error> {
        if !id(app) {
            return Err(invalid());
        }
        Ok(client.rest_form(
            workspace,
            "apps.user.connection.status",
            vec![
                ("app_ids", app.into()),
                (
                    "_x_reason",
                    "CFM fetch app connection status (app_about_tab)".into(),
                ),
            ],
        ))
    }
    pub fn decode(app: &str, raw: Value) -> Result<Self, Error> {
        if serde_json::to_vec(&raw).map_or(true, |v| v.len() > 1024 * 1024)
            || !id(app)
            || raw.get("ok") != Some(&json!(true))
        {
            return Err(invalid());
        }
        let entries = raw
            .get("connections")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        if let Some(entry) = entries.get(app) {
            if !entry.is_object() || entry.get("status").and_then(Value::as_str).is_none() {
                return Err(invalid());
            }
        }
        Ok(Self {
            app: app.into(),
            raw,
        })
    }
    pub fn status(&self) -> &str {
        self.raw["connections"][&self.app]["status"]
            .as_str()
            .unwrap_or("unknown")
    }
    pub fn approval(&self) -> Option<&str> {
        self.raw["connections"][&self.app]["approval"]["state"].as_str()
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    /// Caller supplies actual experiment observation, never an inferred default.
    pub fn can_connect(&self, enabled: bool) -> bool {
        enabled && self.status() == "disconnected"
    }
    pub fn can_manage(&self, enabled: bool) -> bool {
        enabled && self.status() == "connected"
    }

    /// +FNr registers the owned client token before this one-use dispatch.
    /// Both visible Connect and Manage use type=connect and service_id=0.
    pub fn submit(
        &self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
        client_token: &str,
        enabled: bool,
    ) -> Result<PreparedRequest, Error> {
        if (!self.can_connect(enabled) && !self.can_manage(enabled))
            || client_token.is_empty()
            || client_token.len() > 255
            || client_token.bytes().any(|b| b.is_ascii_control())
        {
            return Err(invalid());
        }
        Ok(client.rest_form(
            workspace,
            "apps.user.connection.submit",
            vec![
                ("app_id", self.app.clone()),
                ("service_id", "0".into()),
                ("type", "connect".into()),
                ("client_token", client_token.into()),
                ("_x_reason", "submit-user-connection-event-to-app".into()),
            ],
        ))
    }
}

#[cfg(test)]
mod tests;
