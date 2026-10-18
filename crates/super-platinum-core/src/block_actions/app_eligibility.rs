//! App-DM portion of captured FS2H.canUserPostInChannel.
//! Channel/archive/role policy stays with its existing owner. These request-bound
//! facts do not infer permission from membership or a bot's display name.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, models::User},
};
use serde_json::Value;
pub mod agents;

fn invalid() -> Error {
    Error::Api("app_posting_metadata_invalid".into())
}
fn id(s: &str) -> bool {
    // App/channel IDs are not opaque shortcut identifiers: path/query syntax
    // must not be accepted as captured metadata identity.
    !s.is_empty() && s.len() <= 256 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
fn success(raw: &Value) -> Result<(), Error> {
    if raw.get("ok") == Some(&Value::Bool(true)) && raw.to_string().len() <= 2 * 1024 * 1024 {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn flag(raw: &Value, key: &str, missing: bool) -> Result<bool, Error> {
    match raw.get(key) {
        None => Ok(missing),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(invalid()),
    }
}

/// etUg.PV uses profile.api_app_id; a user ID is never guessed to be an app ID.
pub fn app_id(user: &User) -> Option<&str> {
    // Exact id+5.isAppOrBot guard, distinct from etUg.isBotUser below.
    if !user.is_bot
        && user.extra.get("is_app_user") != Some(&Value::Bool(true))
        && user.id != "USLACK"
    {
        return None;
    }
    user.profile
        .as_ref()?
        .extra
        .get("api_app_id")?
        .as_str()
        .filter(|s| id(s))
}

/// Actual 5e4K/etUg discriminator after the caller establishes an IM and its
/// exact peer. Canonical Slackbot is excluded before bot classification; the
/// independent is_slackbot flag still classifies other IDs as app DMs.
pub fn app_dm(user: &User) -> Result<bool, Error> {
    match user.id.as_str() {
        "USLACKBOT" => Ok(false),
        "USLACK" => Err(invalid()), // FS2H refuses this system conversation.
        _ if user.is_bot => Ok(true),
        _ => match user.extra.get("is_slackbot") {
            None => Ok(false),
            Some(Value::Bool(value)) => Ok(*value),
            _ => Err(invalid()),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppProfile {
    app: String,
    interactable: Option<bool>,
}
impl AppProfile {
    /// fcgE -> ppY6.FL. The request names an exact app rather than the optional
    /// service/bot lookup arm, so the returned ID must agree.
    pub fn decode(app: &str, raw: &Value) -> Result<Self, Error> {
        success(raw)?;
        let profile = raw
            .get("app_profile")
            .filter(|v| v.is_object())
            .ok_or_else(invalid)?;
        if !id(app) || profile.get("id").and_then(Value::as_str) != Some(app) {
            return Err(invalid());
        }
        let interactable = match profile.get("is_interactable") {
            None => None,
            Some(Value::Bool(v)) => Some(*v),
            _ => return Err(invalid()),
        };
        Ok(Self {
            app: app.into(),
            interactable,
        })
    }
    pub fn app(&self) -> &str {
        &self.app
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHomePosting {
    channel: String,
    app: String,
    messages: bool,
    read_only: bool,
}
impl AppHomePosting {
    /// g5os conversations.info(return_app_home=true) hydrates wsPR/Gn9w.
    /// The actual selectors default missing enabled to true and read-only to
    /// false. A missing response/Home object is not a completed metadata read.
    pub fn decode(channel: &str, app: &str, raw: &Value) -> Result<Self, Error> {
        success(raw)?;
        if !id(channel)
            || !id(app)
            || raw.pointer("/channel/id").and_then(Value::as_str) != Some(channel)
        {
            return Err(invalid());
        }
        let home = raw
            .get("app_home")
            .filter(|v| v.is_object())
            .ok_or_else(invalid)?;
        if home.get("app_id").and_then(Value::as_str) != Some(app) {
            return Err(invalid());
        }
        for key in ["conversation_id", "channel_id"] {
            if home.get(key).is_some_and(|v| v.as_str() != Some(channel)) {
                return Err(invalid());
            }
        }
        Ok(Self {
            channel: channel.into(),
            app: app.into(),
            messages: flag(home, "messages_tab_enabled", true)?,
            read_only: flag(home, "messages_tab_read_only_enabled", false)?,
        })
    }
    pub fn allows(
        &self,
        channel: &str,
        app: &str,
        acl_enabled: bool,
        profile: Option<&AppProfile>,
    ) -> bool {
        self.channel == channel
            && self.app == app
            && self.messages
            && !self.read_only
            && (!acl_enabled
                || profile.is_some_and(|p| p.app == app && p.interactable != Some(false)))
    }
}

/// Actual 4OuQ assignment semantics; an unavailable response remains unknown.
pub fn acl_assignment(raw: &Value) -> Result<bool, Error> {
    success(raw)?;
    let assignments = raw
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    match assignments.get("app_acls_for_all_apps") {
        None => Ok(false),
        Some(v) => v
            .get("group")
            .and_then(Value::as_str)
            .map(|s| s == "on")
            .ok_or_else(invalid),
    }
}

pub fn profile_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    app: &str,
) -> Result<PreparedRequest, Error> {
    if !id(app) {
        return Err(invalid());
    }
    Ok(client.rest_form(
        workspace,
        "apps.profile.get",
        vec![
            ("app", app.into()),
            ("_x_reason", "fetch-all-apps-by-id".into()),
        ],
    ))
}

#[cfg(test)]
mod tests;
