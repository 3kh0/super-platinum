//! Capture an ordinary message button before confirmation, then resolve it again
//! against its canonical message before preparing a request. No dispatch occurs
//! here. Attachment, modal, widget and special auth actions need their own paths.

use serde_json::{Value, json};
use std::sync::Arc;

use crate::slack::models::Message;

mod radio;
pub use radio::{CapturedMessageRadio, CurrentMessageRadio};
mod revision;
mod select;
pub use select::{CapturedMessageSelect, CurrentMessageSelect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageActionOwner {
    pub account_epoch: u64,
    pub self_user_id: String,
    pub team_id: String,
    pub channel_id: String,
    pub message_ts: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageActionError {
    InvalidOwner,
    WrongMessage,
    MissingButton,
    AmbiguousButton,
    MalformedButton,
    UnsupportedButton,
    MissingServiceIdentity,
    StaleMessage,
}

#[derive(Clone)]
pub struct CapturedMessageButton {
    owner: MessageActionOwner,
    block_id: String,
    action_id: String,
    revision: Arc<Value>,
    action: Value,
    service_id: String,
    function_execution_id: Option<String>,
    is_ephemeral: bool,
    ephemeral_thread_ts: Option<String>,
}

/// A short-lived borrow produced only after checking the latest canonical source.
/// Callers must not retain it across a network wait or conversation/account change.
pub struct CurrentMessageButton<'a>(&'a CapturedMessageButton);

impl CapturedMessageButton {
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<Self, MessageActionError> {
        validate_owner(&owner, message)?;
        if block_id.trim().is_empty() || action_id.trim().is_empty() {
            return Err(MessageActionError::MissingButton);
        }
        // Widget containers have their own identity/fallback service routing
        // in captured lvR2; they are not ordinary app message buttons.
        if block_id.starts_with("widget:") || client_owned_action(block_id, action_id) {
            return Err(MessageActionError::UnsupportedButton);
        }
        let button = find_button(message, block_id, action_id)?;
        validate_button(button, &owner.self_user_id)?;
        let service_id = message
            .bot_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .ok_or(MessageActionError::MissingServiceIdentity)?
            .to_owned();
        let function_execution_id =
            optional_string(message.extra.get("function_execution_id"))?.map(str::to_owned);
        let is_ephemeral = match message.extra.get("is_ephemeral") {
            None => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(MessageActionError::MalformedButton),
        };
        let ephemeral_thread_ts = if is_ephemeral {
            if message
                .thread_ts
                .as_deref()
                .is_some_and(|ts| !server_ts(ts))
            {
                return Err(MessageActionError::InvalidOwner);
            }
            message.thread_ts.clone()
        } else {
            None
        };
        let mut action = json!({
            "type": "button", "block_id": block_id, "action_id": action_id,
            "text": button["text"],
        });
        // These are the fields emitted by Slack's ordinary ButtonElement and
        // confirmation wrapper. URL and accessibility labels are not wire actions.
        for key in ["value", "style", "confirm"] {
            if let Some(value) = button.get(key) {
                action[key] = value.clone();
            }
        }
        Ok(Self {
            owner,
            block_id: block_id.to_owned(),
            action_id: action_id.to_owned(),
            revision: Arc::new(revision::capture(message)),
            action,
            service_id,
            function_execution_id,
            is_ephemeral,
            ephemeral_thread_ts,
        })
    }

    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageButton<'a>, MessageActionError> {
        if owner != &self.owner {
            return Err(MessageActionError::WrongMessage);
        }
        validate_owner(owner, message)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        // Resolve again rather than trusting an old array position.
        validate_button(
            find_button(message, &self.block_id, &self.action_id)?,
            &owner.self_user_id,
        )?;
        Ok(CurrentMessageButton(self))
    }
}

/// Captured j1cO.onActionWrapper handles these before testing whether the
/// sender is Slackbot. Their original value is not an ordinary developer
/// action: some open local UI, and Salesforce edits dispatch only after that
/// UI supplies a modified value. Keep those flows inert until implemented.
fn client_owned_action(block_id: &str, action_id: &str) -> bool {
    matches!(
        action_id,
        "setup_slackbot_joiner_channel_preview"
            | "setup_slackbot_joiner_channel_join"
            | "wfb_ai_answer_feedback_positive"
            | "wfb_ai_answer_feedback_negative"
            | "set_up_dwa_v3_open_modal"
            | "mcp_tool_blocked_change_prefs"
    ) || (action_id == "setup_slackbot_joiner_person_profile"
        && block_id.starts_with("ai_agent_action:setup_slackbot_joiner_person_"))
        || (matches!(
            action_id,
            "salesforce_record_card_edit" | "salesforce_record_card_update_edit"
        ) && block_id.starts_with("ai_agent_action:"))
        || (action_id == "sf_view_all_files" && block_id.starts_with("sf_files_link:"))
        || (action_id.starts_with("launch_connect_apps_modal")
            && block_id.starts_with("connect_apps_nudge_actions:"))
}

impl CurrentMessageButton<'_> {
    pub fn owner(&self) -> &MessageActionOwner {
        &self.0.owner
    }
    pub fn action(&self) -> &Value {
        &self.0.action
    }
    pub fn service_id(&self) -> &str {
        &self.0.service_id
    }
    pub fn function_execution_id(&self) -> Option<&str> {
        self.0.function_execution_id.as_deref()
    }
    pub fn container(&self) -> Value {
        let mut container = json!({
            "type": "message", "message_ts": self.0.owner.message_ts,
            "channel_id": self.0.owner.channel_id, "is_ephemeral": self.0.is_ephemeral,
        });
        if let Some(ts) = &self.0.ephemeral_thread_ts {
            container["thread_ts"] = Value::String(ts.clone());
        }
        container
    }
}

fn validate_owner(owner: &MessageActionOwner, message: &Message) -> Result<(), MessageActionError> {
    if owner.self_user_id.trim().is_empty()
        || owner.team_id.trim().is_empty()
        || owner.channel_id.trim().is_empty()
        || !server_ts(&owner.message_ts)
    {
        return Err(MessageActionError::InvalidOwner);
    }
    if message.ts.as_deref() != Some(owner.message_ts.as_str())
        || message
            .channel
            .as_deref()
            .is_some_and(|id| id != owner.channel_id)
    {
        return Err(MessageActionError::WrongMessage);
    }
    // Message.team may identify an external Slack Connect author. The workspace
    // owner is supplied by the exact canonical bag, never inferred from the author.
    Ok(())
}

fn server_ts(ts: &str) -> bool {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && seconds.parse::<u64>().is_ok_and(|s| s > 0)
        && !fraction.is_empty()
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

fn find_button<'a>(
    message: &'a Message,
    block_id: &str,
    action_id: &str,
) -> Result<&'a Value, MessageActionError> {
    let mut blocks = message
        .blocks
        .iter()
        .filter(|b| b.get("block_id").and_then(Value::as_str) == Some(block_id));
    let block = blocks.next().ok_or(MessageActionError::MissingButton)?;
    if blocks.next().is_some() {
        return Err(MessageActionError::AmbiguousButton);
    }
    let candidates: Vec<&Value> = match block.get("type").and_then(Value::as_str) {
        Some("actions") => block
            .get("elements")
            .and_then(Value::as_array)
            .map(|elements| elements.iter().collect())
            .unwrap_or_default(),
        Some("section") => block.get("accessory").into_iter().collect(),
        _ => return Err(MessageActionError::UnsupportedButton),
    };
    let mut matches = candidates
        .into_iter()
        .filter(|b| b.get("action_id").and_then(Value::as_str) == Some(action_id));
    let button = matches.next().ok_or(MessageActionError::MissingButton)?;
    if matches.next().is_some() {
        return Err(MessageActionError::AmbiguousButton);
    }
    Ok(button)
}

fn optional_string(value: Option<&Value>) -> Result<Option<&str>, MessageActionError> {
    value
        .map(|v| v.as_str().ok_or(MessageActionError::MalformedButton))
        .transpose()
}

fn text_object(value: Option<&Value>, allow_mrkdwn: bool) -> bool {
    let Some(value) = value else {
        return false;
    };
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("plain_text")
    )
    .then_some(())
    .or_else(|| {
        (allow_mrkdwn && value.get("type").and_then(Value::as_str) == Some("mrkdwn")).then_some(())
    })
    .is_some()
        && value
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
        && ["emoji", "verbatim"]
            .iter()
            .all(|k| value.get(k).is_none_or(Value::is_boolean))
}

fn validate_button(button: &Value, self_user_id: &str) -> Result<(), MessageActionError> {
    if button.get("type").and_then(Value::as_str) != Some("button") {
        return Err(MessageActionError::UnsupportedButton);
    }
    if [
        "third_party_auth",
        "thirdPartyAuth",
        "agent_prompt",
        "agentPrompt",
        "agent_prompt_display",
    ]
    .iter()
    .any(|key| button.get(key).is_some())
    {
        return Err(MessageActionError::UnsupportedButton);
    }
    // Captured urb3/maybeGetCalendarCreateEventAction intercepts these values
    // before blocks.actions for any service. They open a client calendar flow.
    if button
        .get("value")
        .and_then(Value::as_str)
        .is_some_and(|value| {
            [
                "ADZ494LHY_CREATE_EVENT",
                "AFV5ECLBZ_CREATE_EVENT",
                "A01NK8UJRGC_CREATE_EVENT",
            ]
            .iter()
            .any(|prefix| value.starts_with(prefix))
        })
    {
        return Err(MessageActionError::UnsupportedButton);
    }
    validate_recipient(button, self_user_id)?;
    if !text_object(button.get("text"), false)
        || optional_string(button.get("value")).is_err()
        || optional_string(button.get("url")).is_err()
        || optional_string(button.get("accessibility_label")).is_err()
        || button
            .get("style")
            .is_some_and(|s| !matches!(s.as_str(), Some("primary" | "danger")))
    {
        return Err(MessageActionError::MalformedButton);
    }
    validate_confirmation(button)
}

fn validate_recipient(element: &Value, self_user_id: &str) -> Result<(), MessageActionError> {
    if let Some(recipients) = element.get("visible_to_user_ids") {
        let Some(recipients) = recipients.as_array() else {
            return Err(MessageActionError::MalformedButton);
        };
        if recipients
            .iter()
            .any(|id| id.as_str().is_none_or(|id| id.trim().is_empty()))
        {
            return Err(MessageActionError::MalformedButton);
        }
        if !recipients
            .iter()
            .any(|id| id.as_str() == Some(self_user_id))
        {
            return Err(MessageActionError::UnsupportedButton);
        }
    }
    Ok(())
}

fn validate_confirmation(element: &Value) -> Result<(), MessageActionError> {
    if let Some(confirm) = element.get("confirm") {
        if !["title", "confirm", "deny"]
            .iter()
            .all(|key| text_object(confirm.get(key), false))
            || !text_object(confirm.get("text"), true)
            || confirm
                .get("style")
                .is_some_and(|s| !matches!(s.as_str(), Some("primary" | "danger")))
        {
            return Err(MessageActionError::MalformedButton);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
