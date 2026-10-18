//! Captured lAui -> Fesx -> pie7 workflow trigger path. Not blocks.actions.
use super::{View, id};
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
mod authentication;
mod canvas;
mod directory;
mod directory_channels;
pub mod directory_local;

pub mod directory_navigation;
pub mod directory_observations;
pub mod directory_ranking;
mod directory_results;
pub mod directory_selectors;
mod directory_users;
pub mod meeting_events;
pub mod preflight;
pub mod salesforce;
pub use crate::block_actions::directory_endpoint::DirectoryEndpoint;
pub use authentication::Metadata;
pub use canvas::CanvasChoice;
pub use directory::{Directory, DirectoryPage};
pub use directory_channels::{ConversationKind, DirectoryConversation};
pub use directory_results::DirectoryResults;
pub use directory_users::DirectoryUser;
pub use preflight::*;
#[derive(Clone, PartialEq)]
pub struct Workflow {
    view: View,
    user: String,
    block: String,
    action: String,
    raw: Value,
    url: String,
    trigger: String,
}
fn invalid() -> Error {
    Error::Api("home_workflow_invalid".into())
}
fn bounded(v: &Value, key: &str, max: usize) -> Result<String, Error> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn trigger_id(url: &str) -> Option<String> {
    // Exact TWYS first Ft + eight-or-more ASCII alphanumeric match.
    let bytes = url.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i..].starts_with(b"Ft") {
            let n = bytes[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count();
            if n >= 8 {
                return Some(url[i..i + 2 + n].into());
            }
        }
    }
    None
}
impl Workflow {
    pub fn capture(view: &View, block: &str, action: &str, user: &str) -> Result<Self, Error> {
        if !id(user)
            || block.is_empty()
            || block.len() > 255
            || action.is_empty()
            || action.len() > 255
        {
            return Err(invalid());
        }
        let mut blocks = view.raw()["blocks"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .filter(|b| b["block_id"].as_str() == Some(block));
        let parent = blocks.next().ok_or_else(invalid)?;
        if blocks.next().is_some() {
            return Err(invalid());
        }
        let elements: Vec<&Value> = match parent["type"].as_str() {
            Some("actions") => parent["elements"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .collect(),
            Some("section") => parent.get("accessory").into_iter().collect(),
            _ => return Err(invalid()),
        };
        let mut found = elements
            .into_iter()
            .filter(|e| e["action_id"].as_str() == Some(action));
        let raw = found.next().ok_or_else(invalid)?;
        if found.next().is_some()
            || raw["type"] != "workflow_button"
            || raw["text"]["type"] != "plain_text"
            || raw.get("confirm").is_some()
            || raw.get("visible_to_user_ids").is_some()
        {
            return Err(invalid());
        }
        bounded(&raw["text"], "text", 3000)?;
        let url = bounded(&raw["workflow"]["trigger"], "url", 8192)?;
        let parsed = url::Url::parse(&url).map_err(|_| invalid())?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(invalid());
        }
        let trigger = trigger_id(&url).ok_or_else(invalid)?;
        Ok(Self {
            view: view.clone(),
            user: user.into(),
            block: block.into(),
            action: action.into(),
            raw: raw.clone(),
            url,
            trigger,
        })
    }
    pub fn label(&self) -> &str {
        self.raw["text"]["text"].as_str().unwrap()
    }
    pub fn trigger(&self) -> &str {
        &self.trigger
    }
    pub fn same_view(&self, view: &View) -> bool {
        &self.view == view
    }
    pub fn same_action(&self, other: &Self) -> bool {
        self == other
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    // gEn+ supplies the view ID; 6LzE leaves channelId undefined for Home.
    // Do not invent the selected bot conversation as a workflow execution input.
    fn context(&self) -> Value {
        json!({"action_id":self.action,"block_id":self.block,"location":"button","message_ts":self.view.home.view_id})
    }
    fn request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        token: &str,
        context: Value,
        precheck: bool,
    ) -> Result<PreparedRequest, Error> {
        if self.user != w.user_id
            || !self.same_view(current)
            || token.is_empty()
            || token.len() > 255
            || token.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(invalid());
        }
        let mut fields = vec![
            ("client_token", token.into()),
            ("context", context.to_string()),
            ("url", self.url.clone()),
            ("_x_reason", "trip-trigger-from-bk-workflow-button".into()),
        ];
        // The actual acknowledged callback omits runPrecheck; it does not send false.
        if precheck {
            fields.push(("run_precheck", "true".into()));
        }
        Ok(client.rest_form(w, "workflows.triggers.trip", fields))
    }
    pub fn precheck(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        token: &str,
    ) -> Result<PreparedRequest, Error> {
        self.request(client, w, current, token, self.context(), true)
    }
    pub fn response(&self, response: Value) -> Result<TripResult, Error> {
        if response.to_string().len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        if response["ok"] == true {
            return Ok(TripResult::Accepted {
                slow: response
                    .get("is_slow_workflow")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                contains_ai: response
                    .get("contains_ai_steps")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
        if response["ok"] == false && response["error"] == "pretrip_ack_required" {
            let info = &response["pre_trip_info"];
            // Fesx: Home does not set the search-only showSpeedbump override.
            let shown = info["third_party_auths"]["is_satisfied"] == false
                || info["missing_context_parameters"]["is_shown"] == true
                || info["missing_customizable_input_parameters"]["is_shown"] == true
                || info["scopes_acknowledgements"]["is_shown"] == true
                || info["contains_ai_steps"] == true;
            if !shown {
                return Ok(TripResult::Rejected);
            }
            return Preflight::decode(self.clone(), info.clone()).map(TripResult::Review);
        }
        if response["ok"] == false {
            return Ok(TripResult::Rejected);
        }
        Err(invalid())
    }
}
#[derive(Clone, PartialEq)]
pub enum TripResult {
    Accepted { slow: bool, contains_ai: bool },
    Review(Preflight),
    Rejected,
}
#[cfg(test)]
mod tests;
