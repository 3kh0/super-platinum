//! Retained 8dyy/wBUp management contracts. Plans do not grant edit permission or
//! transport authority. The desktop owner checks account, visit and current input
//! before sending, and retains every sent operation through cancellation.
mod activity;
pub mod capabilities;
mod editor;
mod followup;
mod list;
mod mutation;
mod name;
mod observe;
mod publication;
mod trigger;
pub use activity::{
    ActivityDetail, ActivityDetailRequest, ActivityFilters, ActivityPage, ActivityQuery,
};
pub use editor::{Editor, EditorLoad, ReadKind, ReadRequest};
pub use followup::{ConnectorApps, EditPermissions, FollowUpPhase, FollowUpPlan, FollowUpStage};
pub use list::{List, ListPage, ListQuery, ListRow, ListWorkflows, Sort};
pub use mutation::{Command, FollowUp, MutationPlan, Outcome, Slot, StepAction};
pub use name::{NameError, description_invalid, validate_name};
pub use observe::Observation;
pub use publication::Publication;
pub use trigger::TriggerUpdate;

use crate::config::WorkspaceSession;
use crate::slack::{Error, PreparedRequest, SlackClient};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct RequestPlan {
    method: &'static str,
    reason: &'static str,
    arguments: Value,
}
impl RequestPlan {
    fn new(method: &'static str, reason: &'static str, arguments: Value) -> Self {
        debug_assert!(arguments.is_object());
        Self {
            method,
            reason,
            arguments,
        }
    }
    pub fn method(&self) -> &'static str {
        self.method
    }
    pub fn reason(&self) -> &'static str {
        self.reason
    }
    /// Exact snake_case arguments before IlU1 metadata and qVKj form encoding.
    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub fn request(&self, client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
        let mut fields: Vec<(&str, String)> = self
            .arguments
            .as_object()
            .expect("closed plan")
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.as_str(), form_value(v)))
            .collect();
        fields.push(("_x_reason", self.reason.into()));
        client.rest_form(workspace, self.method, fields)
    }
}
// qVKj: primitive arrays stringify as comma-separated values; objects and arrays
// containing objects use JSON. Empty arrays are present as an empty string.
fn form_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) if items.iter().all(|v| !v.is_object() && !v.is_array()) => items
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    form_value(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        _ => value.to_string(),
    }
}
fn invalid() -> Error {
    Error::Api("workflow_management_invalid".into())
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
fn identifier(raw: &Value, field: &str) -> Result<String, Error> {
    raw.get(field)
        .and_then(Value::as_str)
        .filter(|s| id(s))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn success(raw: &Value) -> Result<(), Error> {
    if raw.is_object()
        && raw.get("ok") == Some(&Value::Bool(true))
        && raw.get("error").is_none_or(Value::is_null)
    {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn array<'a>(raw: &'a Value, field: &str) -> Result<&'a Vec<Value>, Error> {
    raw.get(field).and_then(Value::as_array).ok_or_else(invalid)
}

/// Native acquisition adapter: 2rgv reads an already hydrated D1NF store. The
/// native owner populates the same identity from the retained editor list API.
pub fn resolve_app(app: &str) -> Result<RequestPlan, Error> {
    if !id(app) {
        return Err(invalid());
    }
    Ok(RequestPlan::new(
        "workflows.triggers.list",
        "load-workflow-editor",
        json!({"app_id":app}),
    ))
}
#[derive(Clone, Debug, PartialEq)]
pub struct AppWorkflow {
    app: String,
    workflow: String,
    raw: Value,
    trigger: Value,
}
impl AppWorkflow {
    pub fn decode(app: &str, raw: Value) -> Result<Self, Error> {
        resolve_app(app)?;
        success(&raw)?;
        let trigger = array(&raw, "triggers")?
            .iter()
            .find(|t| t["workflow"]["app_id"].as_str() == Some(app))
            .ok_or_else(invalid)?
            .clone();
        identifier(&trigger, "id")?;
        let workflow = identifier(&trigger["workflow"], "workflow_id")?;
        Ok(Self {
            app: app.into(),
            workflow,
            raw,
            trigger,
        })
    }
    pub fn app_id(&self) -> &str {
        &self.app
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn trigger(&self) -> &Value {
        &self.trigger
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_v2;
