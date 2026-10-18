//! AjOk/DUhY CSV initiation. ACK is not a download, file, or export completion.
use super::*;

pub const ACTIVITY_EXPORT_EXPERIMENT: &str = "wfb_export_activity_log_as_csv";

/// A non-Clone, take-once request holder. The controller still owns exact
/// account/visit/query custody and its existing single-attempt transport.
#[derive(Debug)]
pub struct ActivityExport {
    workflow: String,
    arguments: Value,
    request: Option<RequestPlan>,
}
impl ActivityExport {
    /// `assignment` is the actual current experiment value, not an inferred
    /// capability from successful Activity reads. Missing/off values refuse.
    pub fn new(query: &ActivityQuery, assignment: Option<&Value>) -> Result<Self, Error> {
        if assignment.and_then(Value::as_str) != Some("on") {
            return Err(invalid());
        }
        let observed = query.filters();
        let mut filters = serde_json::Map::new();
        if let Some(status) = observed
            .status
            .as_deref()
            .filter(|value| matches!(*value, "success" | "error" | "in_progress" | "cancelled"))
        {
            filters.insert("workflow_instance_status".into(), json!(status));
        }
        if let Some(user) = &observed.user_id {
            filters.insert("user_id".into(), json!(user));
        }
        if let Some(value) = observed.from_timestamp {
            filters.insert("from_timestamp".into(), json!(value));
        }
        if let Some(value) = observed.to_timestamp {
            filters.insert("to_timestamp".into(), json!(value));
        }
        // The source export does not send the table's cursor, page size or
        // parent execution. Do not accidentally turn this into page-only export.
        let workflow = query.workflow_id().to_owned();
        let arguments = json!({"workflow_id":workflow,"filters":filters});
        let request = RequestPlan::new(
            "workflows.activity.export",
            "export_workflow_activity_csv",
            arguments.clone(),
        );
        Ok(Self {
            workflow,
            arguments,
            request: Some(request),
        })
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub fn take_request(&mut self) -> Option<RequestPlan> {
        self.request.take()
    }
    /// Only a positive response to the taken request can become initiation ACK.
    /// A transport/unknown outcome is retained by the owner, never auto-retried.
    pub fn acknowledge(&self, raw: Value) -> Result<ActivityExportInitiated, Error> {
        if self.request.is_some() {
            return Err(invalid());
        }
        success(&raw)?;
        Ok(ActivityExportInitiated {
            workflow: self.workflow.clone(),
            arguments: self.arguments.clone(),
            raw,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityExportInitiated {
    workflow: String,
    arguments: Value,
    raw: Value,
}
impl ActivityExportInitiated {
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

#[cfg(test)]
mod tests;
