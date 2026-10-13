//! Pure preparation for the captured ordinary-message `blocks.actions` path.
//!
//! This module does not dispatch requests or imply app/modal support. Callers
//! must supply current, revalidated message identity and resolved app metadata.

use serde_json::{Value, json};

use crate::block_actions::validation::{
    CurrentMessageButton, CurrentMessageRadio, CurrentMessageSelect, MessageActionOwner,
};
use crate::config::WorkspaceSession;

use super::{Error, PreparedRequest, SlackClient};

/// Prepare one button action using the captured MessageBlocks -> sendActions
/// contract. `state`, when supplied, is the container's existing values object.
pub fn block_action_button(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    button: &CurrentMessageButton<'_>,
    app_id: &str,
    service_team_id: &str,
    client_token: &str,
    state: Option<&Value>,
) -> Result<PreparedRequest, Error> {
    block_action(
        client,
        workspace,
        WireAction {
            owner: button.owner(),
            action: button.action(),
            service_id: button.service_id(),
            function_execution_id: button.function_execution_id(),
            container: button.container(),
        },
        app_id,
        service_team_id,
        client_token,
        state,
    )
}

/// Prepare the captured static-select action after local state is committed.
/// `state` is the complete current message values map, not `{ "values": ... }`.
pub fn block_action_select(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    select: &CurrentMessageSelect<'_>,
    app_id: &str,
    service_team_id: &str,
    client_token: &str,
    state: Option<&Value>,
) -> Result<PreparedRequest, Error> {
    let action = select.action();
    let selected = state
        .and_then(|values| values.get(action["block_id"].as_str()?))
        .and_then(|block| block.get(action["action_id"].as_str()?));
    if selected.is_none_or(|value| {
        value.get("type").and_then(Value::as_str) != Some("static_select")
            || value.get("selected_option") != action.get("selected_option")
    }) {
        return Err(Error::Api("block_action_state_invalid".into()));
    }
    block_action(
        client,
        workspace,
        WireAction {
            owner: select.owner(),
            action,
            service_id: select.service_id(),
            function_execution_id: select.function_execution_id(),
            container: select.container(),
        },
        app_id,
        service_team_id,
        client_token,
        state,
    )
}

/// Prepare the captured radio-button action after local state is committed.
/// `state` is the complete current message values map, not `{ "values": ... }`.
pub fn block_action_radio(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    select: &CurrentMessageRadio<'_>,
    app_id: &str,
    service_team_id: &str,
    client_token: &str,
    state: Option<&Value>,
) -> Result<PreparedRequest, Error> {
    let action = select.action();
    let selected = state
        .and_then(|values| values.get(action["block_id"].as_str()?))
        .and_then(|block| block.get(action["action_id"].as_str()?));
    if selected.is_none_or(|value| {
        value.get("type").and_then(Value::as_str) != Some("radio_buttons")
            || value.get("selected_option") != action.get("selected_option")
    }) {
        return Err(Error::Api("block_action_state_invalid".into()));
    }
    block_action(
        client,
        workspace,
        WireAction {
            owner: select.owner(),
            action,
            service_id: select.service_id(),
            function_execution_id: select.function_execution_id(),
            container: select.container(),
        },
        app_id,
        service_team_id,
        client_token,
        state,
    )
}

struct WireAction<'a> {
    owner: &'a MessageActionOwner,
    action: &'a Value,
    service_id: &'a str,
    function_execution_id: Option<&'a str>,
    container: Value,
}

fn block_action(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    action: WireAction<'_>,
    app_id: &str,
    service_team_id: &str,
    client_token: &str,
    state: Option<&Value>,
) -> Result<PreparedRequest, Error> {
    if workspace.team_id != action.owner.team_id || workspace.user_id != action.owner.self_user_id {
        return Err(Error::Api("block_action_workspace_mismatch".into()));
    }
    if !valid_identifier(app_id)
        || !valid_identifier(service_team_id)
        || !valid_identifier(client_token)
    {
        return Err(Error::Api("block_action_metadata_unavailable".into()));
    }
    if state.is_some_and(|values| !values.is_object()) {
        return Err(Error::Api("block_action_state_invalid".into()));
    }

    let mut fields = vec![
        ("service_id", action.service_id.to_owned()),
        ("app_id", app_id.to_owned()),
        ("service_team_id", service_team_id.to_owned()),
        ("actions", json!([action.action]).to_string()),
        ("container", action.container.to_string()),
        ("client_token", client_token.to_owned()),
    ];
    if let Some(id) = action.function_execution_id {
        fields.push(("function_execution_id", id.to_owned()));
    }
    if let Some(values) = state {
        fields.push(("state", json!({"values": values}).to_string()));
    }
    Ok(client.rest_form(workspace, "blocks.actions", fields))
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Acknowledgement does not imply a message update or a completed app workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockActionResponse {
    Accepted,
    Rejected(BlockActionRejection),
}

/// Only the endpoint's error code and plain feedback text are retained. The
/// caller must render `message` as text, never markup, and should not log it.
#[derive(Clone, PartialEq, Eq)]
pub struct BlockActionRejection {
    pub error: String,
    pub message: Option<String>,
}

impl std::fmt::Debug for BlockActionRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BlockActionRejection")
            .field("has_message", &self.message.is_some())
            .finish_non_exhaustive()
    }
}

/// Decode before generic transport error conversion if a future dispatcher
/// needs app-provided feedback. No generic transport behavior changes here.
pub fn decode_block_action_response(value: &Value) -> Result<BlockActionResponse, Error> {
    let invalid = || Error::Api("block_action_response_invalid".into());
    let ok = value
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    if ok {
        return Ok(BlockActionResponse::Accepted);
    }
    let error = value
        .get("error")
        .and_then(Value::as_str)
        .filter(|code| !code.trim().is_empty())
        .ok_or_else(invalid)?;
    let message = match value.get("message") {
        None | Some(Value::Null) => None,
        Some(Value::String(message)) => Some(message.clone()),
        _ => return Err(invalid()),
    };
    Ok(BlockActionResponse::Rejected(BlockActionRejection {
        error: error.to_owned(),
        message,
    }))
}

#[cfg(test)]
mod tests;
