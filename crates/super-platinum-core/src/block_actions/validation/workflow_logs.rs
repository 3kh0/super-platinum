//! j1cO's client-owned View logs action. This captures navigation intent, never
//! a developer action or a request. The desktop owns entry and draft custody.
use super::*;

pub const WORKFLOW_LOGS_ACTION: &str = "workflow_error_view_logs";

#[derive(Clone)]
pub struct CapturedWorkflowLogs {
    owner: MessageActionOwner,
    block_id: String,
    revision: Arc<Value>,
    button: Value,
    workflow: String,
    execution: Option<String>,
}

/// Valid only for the revalidated canonical message. The caller must recheck
/// its account, transport, source and destination after any asynchronous work.
pub struct CurrentWorkflowLogs<'a>(&'a CapturedWorkflowLogs);

impl CapturedWorkflowLogs {
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<Self, MessageActionError> {
        validate_owner(&owner, message)?;
        if action_id != WORKFLOW_LOGS_ACTION || block_id.starts_with("widget:") {
            return Err(MessageActionError::UnsupportedButton);
        }
        if block_id.trim().is_empty() {
            return Err(MessageActionError::MissingButton);
        }
        let button = find_button(message, block_id, action_id)?;
        validate_button(button, &owner.self_user_id)?;
        let (workflow, execution) = target(button)?;
        Ok(Self {
            owner,
            block_id: block_id.into(),
            revision: Arc::new(revision::capture(message)),
            button: button.clone(),
            workflow,
            execution,
        })
    }

    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentWorkflowLogs<'a>, MessageActionError> {
        if owner != &self.owner {
            return Err(MessageActionError::WrongMessage);
        }
        validate_owner(owner, message)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        let button = find_button(message, &self.block_id, WORKFLOW_LOGS_ACTION)?;
        validate_button(button, &owner.self_user_id)?;
        Ok(CurrentWorkflowLogs(self))
    }

    pub fn owner(&self) -> &MessageActionOwner {
        &self.owner
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn execution_id(&self) -> Option<&str> {
        self.execution.as_deref()
    }
    /// Complete canonical button, including opaque fields and original value.
    pub fn button(&self) -> &Value {
        &self.button
    }
    pub fn confirmation(&self) -> Option<&Value> {
        self.button.get("confirm")
    }
}

impl CurrentWorkflowLogs<'_> {
    pub fn owner(&self) -> &MessageActionOwner {
        self.0.owner()
    }
    pub fn workflow_id(&self) -> &str {
        self.0.workflow_id()
    }
    pub fn execution_id(&self) -> Option<&str> {
        self.0.execution_id()
    }
    pub fn button(&self) -> &Value {
        self.0.button()
    }
}

fn target(button: &Value) -> Result<(String, Option<String>), MessageActionError> {
    let value = button["value"]
        .as_str()
        .ok_or(MessageActionError::MalformedButton)?;
    let value: Value =
        serde_json::from_str(value).map_err(|_| MessageActionError::MalformedButton)?;
    if value["type"].as_str() != Some("wfb") {
        return Err(MessageActionError::UnsupportedButton);
    }
    // Source uses truthiness; this native boundary additionally requires string
    // identifiers. Preserve strings exactly rather than trimming or coercing.
    let workflow = value["wf_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or(MessageActionError::MalformedButton)?;
    let execution = match value.get("execution_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.is_empty() => None,
        Some(Value::String(id)) => Some(id.clone()),
        Some(_) => return Err(MessageActionError::MalformedButton),
    };
    Ok((workflow.into(), execution))
}

#[cfg(test)]
mod tests;
