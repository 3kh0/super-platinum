//! Captured urb3/lebZ suggestions routing. No retry or result-membership authority.
use super::{Error, PreparedRequest, SlackClient};
use crate::block_actions::{
    external::{ExternalSuggestions, MAX_QUERY_UNITS},
    form::{InputKey, parse_message_external, parse_message_external_options},
    modal::{ModalScope, ModalView},
    validation::{CapturedMessageExternal, CapturedMessageExternalOptions, MessageActionOwner},
};
use crate::config::WorkspaceSession;
use crate::slack::models::Message;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestionsContext {
    team: String,
    user: String,
    service_id: String,
    app_id: String,
    service_team_id: String,
    function_execution_id: Option<String>,
    container: Value,
    key: InputKey,
}
fn invalid() -> Error {
    Error::Api("block_suggestions_context_invalid".into())
}
fn identity(value: &str) -> Result<String, Error> {
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        Err(invalid())
    } else {
        Ok(value.into())
    }
}
impl SuggestionsContext {
    pub fn message(
        owner: &MessageActionOwner,
        message: &Message,
        key: &InputKey,
        app_id: &str,
        service_team_id: &str,
    ) -> Result<Self, Error> {
        identity(&key.block_id)?;
        identity(&key.action_id)?;
        CapturedMessageExternal::validate_element(owner, message, &key.block_id, &key.action_id)
            .or_else(|_| {
                CapturedMessageExternalOptions::validate_element(
                    owner,
                    message,
                    &key.block_id,
                    &key.action_id,
                )
            })
            .map_err(|_| invalid())?;
        let ephemeral = message
            .extra
            .get("is_ephemeral")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut container = json!({"type":"message","message_ts":owner.message_ts,"channel_id":owner.channel_id,"is_ephemeral":ephemeral});
        if ephemeral {
            if let Some(ts) = &message.thread_ts {
                container["thread_ts"] = json!(ts);
            }
        }
        Ok(Self {
            team: identity(&owner.team_id)?,
            user: identity(&owner.self_user_id)?,
            service_id: identity(message.bot_id.as_deref().ok_or_else(invalid)?)?,
            app_id: identity(app_id)?,
            service_team_id: identity(service_team_id)?,
            function_execution_id: message
                .extra
                .get("function_execution_id")
                .and_then(Value::as_str)
                .map(identity)
                .transpose()?,
            container,
            key: key.clone(),
        })
    }
    pub fn modal(
        scope: &ModalScope,
        self_user_id: &str,
        view: &ModalView,
        key: &InputKey,
    ) -> Result<Self, Error> {
        identity(&key.block_id)?;
        identity(&key.action_id)?;
        let payload = view
            .payload
            .as_ref()
            .filter(|_| !view.needs_content && view.kind == "modal")
            .ok_or_else(invalid)?;
        if payload.get("id").and_then(Value::as_str) != Some(view.id.as_str())
            || payload.get("app_id").and_then(Value::as_str) != Some(view.app_id.as_str())
        {
            return Err(invalid());
        }
        if payload.get("type").and_then(Value::as_str) != Some("modal") {
            return Err(invalid());
        }
        let blocks = payload
            .get("blocks")
            .and_then(Value::as_array)
            .filter(|blocks| blocks.len() <= 100)
            .ok_or_else(invalid)?;
        let mut matches = blocks.iter().filter(|block| {
            block.get("block_id").and_then(Value::as_str) == Some(key.block_id.as_str())
        });
        let block = matches
            .next()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("input"))
            .ok_or_else(invalid)?;
        if matches.next().is_some()
            || block
                .get("dispatch_action")
                .is_some_and(|v| v != &Value::Bool(false))
        {
            return Err(invalid());
        }
        let element = block
            .get("element")
            .filter(|element| {
                element.get("action_id").and_then(Value::as_str) == Some(key.action_id.as_str())
            })
            .ok_or_else(invalid)?;
        if ["confirm", "dispatch_action_config", "visible_to_user_ids"]
            .iter()
            .any(|name| element.get(*name).is_some())
        {
            return Err(invalid());
        }
        match element.get("type").and_then(Value::as_str) {
            Some("external_select") => parse_message_external(element),
            Some("multi_external_select") => parse_message_external_options(element),
            _ => return Err(invalid()),
        }
        .map_err(|_| invalid())?;
        // 5Fwt normal-modal caller omits viewContextTeamId. gEn+ therefore uses
        // raw app_installed_team_id, shallow-normalized by ijf8; never active team.
        let route = |name| {
            payload
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(invalid)
                .and_then(identity)
        };
        Ok(Self {
            team: identity(&scope.team)?,
            user: identity(self_user_id)?,
            service_id: route("bot_id")?,
            app_id: identity(&view.app_id)?,
            service_team_id: route("app_installed_team_id")?,
            function_execution_id: None,
            container: json!({"type":"view","view_id":identity(&view.id)?}),
            key: key.clone(),
        })
    }
}
pub fn block_suggestions(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    context: &SuggestionsContext,
    query: &str,
) -> Result<PreparedRequest, Error> {
    if workspace.team_id != context.team
        || workspace.user_id != context.user
        || query.encode_utf16().count() > MAX_QUERY_UNITS
    {
        return Err(invalid());
    }
    let mut fields = vec![
        ("service_id", context.service_id.clone()),
        ("app_id", context.app_id.clone()),
        ("service_team_id", context.service_team_id.clone()),
        ("container", context.container.to_string()),
        ("action_id", context.key.action_id.clone()),
        ("block_id", context.key.block_id.clone()),
        ("value", query.to_owned()),
    ];
    if let Some(id) = &context.function_execution_id {
        fields.push(("function_execution_id", id.clone()));
    }
    Ok(client.rest_form(workspace, "blocks.suggestions", fields))
}
pub fn decode_external_suggestions(value: &Value) -> Result<ExternalSuggestions, Error> {
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(Error::Api("block_suggestions_rejected_or_invalid".into()));
    }
    ExternalSuggestions::parse(value)
        .map_err(|_| Error::Api("block_suggestions_response_invalid".into()))
}

#[cfg(test)]
mod tests;
