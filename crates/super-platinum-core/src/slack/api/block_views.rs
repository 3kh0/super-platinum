//! Captured FGXB app-view requests and explicit response interpretation.
//! Preparation and decoding never dispatch or mutate a local modal stack.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::config::WorkspaceSession;

use super::{Error, PreparedRequest, SlackClient};

pub fn block_view_get(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    view_id: &str,
) -> Result<PreparedRequest, Error> {
    validate_ids(&[view_id])?;
    Ok(client.rest_form(workspace, "views.get", vec![("view_id", view_id.into())]))
}

/// `state` is the complete `{values: ...}` envelope from the current form.
pub fn block_view_submit(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    view_id: &str,
    client_token: &str,
    state: &Value,
) -> Result<PreparedRequest, Error> {
    validate_ids(&[view_id, client_token])?;
    if !state
        .as_object()
        .is_some_and(|state| state.len() == 1 && state.get("values").is_some_and(Value::is_object))
    {
        return Err(Error::Api("block_view_state_invalid".into()));
    }
    Ok(client.rest_form(
        workspace,
        "views.submit",
        vec![
            ("client_token", client_token.into()),
            ("view_id", view_id.into()),
            ("state", state.to_string()),
        ],
    ))
}

pub fn block_view_close(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    view_id: &str,
    client_token: &str,
    root_view_id: Option<&str>,
) -> Result<PreparedRequest, Error> {
    validate_ids(&[view_id, client_token])?;
    let mut fields = vec![
        ("client_token", client_token.into()),
        ("view_id", view_id.into()),
    ];
    if let Some(root) = root_view_id {
        validate_ids(&[root])?;
        fields.push(("root_view_id", root.into()));
    }
    Ok(client.rest_form(workspace, "views.close", fields))
}

fn validate_ids(ids: &[&str]) -> Result<(), Error> {
    if ids
        .iter()
        .any(|id| id.is_empty() || id.chars().any(|c| c.is_control() || c.is_whitespace()))
    {
        return Err(Error::Api("block_view_identity_invalid".into()));
    }
    Ok(())
}

/// Accepted means an endpoint acknowledged the operation, not app workflow
/// completion. An unrecognized submission must leave the modal available.
#[derive(Clone, PartialEq)]
pub enum BlockViewResponse<T> {
    Accepted(T),
    Rejected(BlockViewRejection),
}

impl<T> std::fmt::Debug for BlockViewResponse<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted(_) => f.write_str("BlockViewResponse::Accepted(..)"),
            Self::Rejected(error) => f
                .debug_tuple("BlockViewResponse::Rejected")
                .field(error)
                .finish(),
        }
    }
}

/// App-provided content is plain feedback, not markup and not log material.
#[derive(Clone, PartialEq, Eq)]
pub struct BlockViewRejection {
    pub error: String,
    pub message: Option<String>,
    pub errors: BTreeMap<String, String>,
    pub view_error: Option<String>,
}

impl std::fmt::Debug for BlockViewRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockViewRejection")
            .field("field_error_count", &self.errors.len())
            .field("has_message", &self.message.is_some())
            .field("has_view_error", &self.view_error.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq)]
pub enum BlockViewResponseAction {
    Absent,
    Update(Value),
    Push(Value),
    Clear,
}

#[derive(Clone, PartialEq)]
pub struct BlockViewSubmission {
    pub action: BlockViewResponseAction,
    pub toast_message: Option<String>,
}

pub fn decode_block_view_get(value: &Value) -> Result<BlockViewResponse<Value>, Error> {
    if let Some(error) = rejection(value)? {
        return Ok(BlockViewResponse::Rejected(error));
    }
    Ok(BlockViewResponse::Accepted(view(value)?))
}

pub fn decode_block_view_submit(
    value: &Value,
) -> Result<BlockViewResponse<BlockViewSubmission>, Error> {
    if let Some(error) = rejection(value)? {
        return Ok(BlockViewResponse::Rejected(error));
    }
    let action = match value.get("response_action") {
        None | Some(Value::Null) => BlockViewResponseAction::Absent,
        Some(Value::String(action)) => match action.as_str() {
            "" => BlockViewResponseAction::Absent,
            "update" => BlockViewResponseAction::Update(view(value)?),
            "push" => BlockViewResponseAction::Push(view(value)?),
            "clear" => BlockViewResponseAction::Clear,
            _ => return Err(Error::Api("block_view_response_action_unsupported".into())),
        },
        _ => return Err(invalid()),
    };
    Ok(BlockViewResponse::Accepted(BlockViewSubmission {
        action,
        toast_message: optional_text(value, "toast_message")?,
    }))
}

pub fn decode_block_view_close(value: &Value) -> Result<BlockViewResponse<()>, Error> {
    Ok(match rejection(value)? {
        Some(error) => BlockViewResponse::Rejected(error),
        None => BlockViewResponse::Accepted(()),
    })
}

fn invalid() -> Error {
    Error::Api("block_view_response_invalid".into())
}

fn view(value: &Value) -> Result<Value, Error> {
    let view = value
        .get("view")
        .filter(|v| v.is_object())
        .ok_or_else(invalid)?;
    let id = view.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
    validate_ids(&[id]).map_err(|_| invalid())?;
    Ok(view.clone())
}

fn optional_text(value: &Value, key: &str) -> Result<Option<String>, Error> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => Err(invalid()),
    }
}

fn rejection(value: &Value) -> Result<Option<BlockViewRejection>, Error> {
    match value.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(None),
        Some(false) => {
            let error = value
                .get("error")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(invalid)?;
            let mut errors = BTreeMap::new();
            match value.get("errors") {
                None | Some(Value::Null) => {}
                Some(Value::Object(fields)) => {
                    for (key, text) in fields {
                        if key.is_empty() {
                            return Err(invalid());
                        }
                        errors.insert(key.clone(), text.as_str().ok_or_else(invalid)?.to_owned());
                    }
                }
                _ => return Err(invalid()),
            }
            Ok(Some(BlockViewRejection {
                error: error.into(),
                message: optional_text(value, "message")?,
                errors,
                view_error: optional_text(value, "view_error")?,
            }))
        }
        None => Err(invalid()),
    }
}

#[cfg(test)]
mod tests;
