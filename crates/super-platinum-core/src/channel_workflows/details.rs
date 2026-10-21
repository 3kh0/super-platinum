//! Cold Workflow details reads and wcWH metadata projection. No execution,
//! editor permission, channel navigation or owner-currentness is implied.
use super::*;

mod projection;
pub use projection::Projection;
#[cfg(test)]
mod tests;

impl Read {
    pub fn trigger_info(id: &str) -> Result<Self, Error> {
        if !identifier(id) {
            return Err(invalid());
        }
        Ok(Self {
            method: "workflows.triggers.info",
            reason: "hermes_triggers_ufm",
            arguments: json!({"trigger_id":id,"include_workflow_details":true}),
        })
    }
    pub fn workflow(id: &str) -> Result<Self, Error> {
        if !identifier(id) {
            return Err(invalid());
        }
        Ok(Self {
            method: "functions.workflows.get",
            reason: "hermes_workflows_store_unknown_fetching",
            arguments: json!({"workflow_id":id}),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NotFound,
    AccessDenied,
    Other(String),
    Malformed,
}
impl FailureKind {
    /// H5iv only gives trigger_not_found and access_denied dedicated views.
    pub fn from_code(code: &str) -> Self {
        match code {
            "trigger_not_found" => Self::NotFound,
            "access_denied" => Self::AccessDenied,
            "" => Self::Malformed,
            other => Self::Other(other.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    kind: FailureKind,
    raw: Value,
}
impl Failure {
    pub fn kind(&self) -> &FailureKind {
        &self.kind
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    fn malformed(raw: &Value) -> Self {
        Self {
            kind: FailureKind::Malformed,
            raw: raw.clone(),
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Do not format opaque server fields or URLs into user-facing errors.
        write!(f, "workflow details metadata unavailable")
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    id: String,
    raw: Value,
}
impl Trigger {
    pub fn decode(requested: &str, raw: Value) -> Result<Self, Failure> {
        envelope(&raw)?;
        let Some(trigger) = raw.get("trigger").filter(|v| v.is_object()) else {
            return Err(Failure::malformed(&raw));
        };
        entity(&raw, trigger, requested)?;
        let valid = (|| {
            for key in [
                "name",
                "description",
                "type",
                "owning_team_id",
                "shortcut_url",
                "share_url",
            ] {
                string_field(trigger, key)?;
            }
            if let Some(workflow) = object_field(trigger, "workflow")? {
                for key in ["workflow_id", "title", "description", "app_id"] {
                    string_field(workflow, key)?;
                }
                if let Some(id) = workflow
                    .get("workflow_id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    if !identifier(id) {
                        return Err(());
                    }
                }
                object_field(workflow, "app")?;
            }
            if let Some(details) = object_field(trigger, "workflow_details")? {
                details_fields(details)?;
            }
            Ok(())
        })();
        if valid.is_err() {
            return Err(Failure::malformed(&raw));
        }
        Ok(Self {
            id: requested.into(),
            raw,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn trigger(&self) -> &Value {
        &self.raw["trigger"]
    }
    pub fn workflow_id(&self) -> Option<&str> {
        self.trigger()
            .pointer("/workflow/workflow_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Workflow {
    id: String,
    raw: Value,
}
impl Workflow {
    pub fn decode(requested: &str, raw: Value) -> Result<Self, Failure> {
        envelope(&raw)?;
        let Some(workflow) = raw.get("workflow").filter(|v| v.is_object()) else {
            return Err(Failure::malformed(&raw));
        };
        entity(&raw, workflow, requested)?;
        if details_fields(workflow).is_err() {
            return Err(Failure::malformed(&raw));
        }
        Ok(Self {
            id: requested.into(),
            raw,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn workflow(&self) -> &Value {
        &self.raw["workflow"]
    }
}

fn response_error(envelope: &Value, row: &Value) -> Result<(), Failure> {
    match row.get("error") {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(code)) => Err(Failure {
            kind: FailureKind::from_code(code),
            raw: envelope.clone(),
        }),
        _ => Err(Failure::malformed(envelope)),
    }
}
fn envelope(raw: &Value) -> Result<(), Failure> {
    if !raw.is_object() || !raw.get("ok").is_some_and(Value::is_boolean) {
        return Err(Failure::malformed(raw));
    }
    response_error(raw, raw)?;
    if raw["ok"] != true {
        return Err(Failure::malformed(raw));
    }
    Ok(())
}
fn entity(raw: &Value, row: &Value, requested: &str) -> Result<(), Failure> {
    if !identifier(requested) || row["id"] != requested {
        return Err(Failure::malformed(raw));
    }
    response_error(raw, row)?;
    for key in ["isUnknown", "isNonExistent"] {
        if row.get(key).is_some_and(|v| v != false) {
            return Err(Failure::malformed(raw));
        }
    }
    Ok(())
}
fn string_field(raw: &Value, key: &str) -> Result<(), ()> {
    if raw.get(key).is_some_and(|v| !v.is_null() && !v.is_string()) {
        Err(())
    } else {
        Ok(())
    }
}
fn object_field<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a Value>, ()> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) if v.is_object() => Ok(Some(v)),
        _ => Err(()),
    }
}
fn details_fields(raw: &Value) -> Result<(), ()> {
    for key in ["title", "description", "bot_user_id"] {
        string_field(raw, key)?;
    }
    for key in ["collaborators", "steps"] {
        if raw.get(key).is_some_and(|v| !v.is_null() && !v.is_array()) {
            return Err(());
        }
    }
    Ok(())
}
