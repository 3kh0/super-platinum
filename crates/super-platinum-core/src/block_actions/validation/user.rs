//! Captured I8j+ users_select actions in ordinary message containers.
//! Capturing is pure: confirmation cancellation simply discards this value.
use super::*;
use crate::block_actions::form;

#[derive(Clone)]
pub struct CapturedMessageUser {
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

/// A revalidated, immutable candidate. Dispatch must also snapshot the current
/// message-container state after committing this confirmed selection locally.
pub struct CurrentMessageUser<'a>(&'a CapturedMessageUser);

impl CapturedMessageUser {
    /// Borrow the unique canonical users_select after checking its message and action
    /// metadata. Callers must separately validate the full input model and users;
    /// this presentation helper neither captures state nor authorizes dispatch.
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        validate_owner(owner, message)?;
        if block_id.trim().is_empty() || action_id.trim().is_empty() {
            return Err(MessageActionError::MissingButton);
        }
        if block_id.starts_with("widget:") || client_owned_action(block_id, action_id) {
            return Err(MessageActionError::UnsupportedButton);
        }
        let element = find_button(message, block_id, action_id)?;
        validate_user(element, &owner.self_user_id)?;
        let service_id = message
            .bot_id
            .as_deref()
            .filter(|id| !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c.is_control()))
            .ok_or(MessageActionError::MissingServiceIdentity)?;
        if service_id == "BSLACKBOT" {
            return Err(MessageActionError::UnsupportedButton);
        }
        optional_string(message.extra.get("function_execution_id"))?;
        match message.extra.get("is_ephemeral") {
            None | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => {
                if message
                    .thread_ts
                    .as_deref()
                    .is_some_and(|ts| !server_ts(ts))
                {
                    return Err(MessageActionError::InvalidOwner);
                }
            }
            _ => return Err(MessageActionError::MalformedButton),
        }
        Ok(element)
    }
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected_user: Option<&str>,
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field =
            form::parse_message_user(element).map_err(|_| MessageActionError::MalformedButton)?;
        form::validate_message_user(&field, selected_user)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let service_id = message
            .bot_id
            .as_deref()
            .filter(|id| !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c.is_control()))
            .ok_or(MessageActionError::MissingServiceIdentity)?;
        if service_id == "BSLACKBOT" {
            return Err(MessageActionError::UnsupportedButton);
        }
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
            "type":"users_select", "block_id":block_id, "action_id":action_id,
            "selected_user":selected_user,
        });
        // I8j+ emits truthy initialUser. BlockElementShell adds confirm.
        if let Some(value) = element
            .get("initial_user")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        {
            action["initial_user"] = json!(value);
        }
        if let Some(confirm) = element.get("confirm").filter(|v| !v.is_null()) {
            action["confirm"] = confirm.clone();
        }
        Ok(Self {
            owner,
            block_id: block_id.into(),
            action_id: action_id.into(),
            revision: Arc::new(revision::capture(message)),
            action,
            service_id: service_id.into(),
            function_execution_id,
            is_ephemeral,
            ephemeral_thread_ts,
        })
    }

    pub fn confirmation(&self) -> Option<&Value> {
        self.action.get("confirm")
    }
    pub fn selected_user(&self) -> Option<&str> {
        self.action.get("selected_user").and_then(Value::as_str)
    }

    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageUser<'a>, MessageActionError> {
        if owner != &self.owner {
            return Err(MessageActionError::WrongMessage);
        }
        validate_owner(owner, message)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        let element = find_button(message, &self.block_id, &self.action_id)?;
        validate_user(element, &owner.self_user_id)?;
        // Exact frozen revision preserves the validated initial and action metadata.
        Ok(CurrentMessageUser(self))
    }
}

fn validate_user(element: &Value, viewer: &str) -> Result<(), MessageActionError> {
    if element.get("type").and_then(Value::as_str) != Some("users_select") {
        return Err(MessageActionError::UnsupportedButton);
    }
    validate_recipient(element, viewer)?;
    validate_confirmation(element)?;
    if [
        "third_party_auth",
        "thirdPartyAuth",
        "agent_prompt",
        "agentPrompt",
        "agent_prompt_display",
    ]
    .iter()
    .any(|key| element.get(key).is_some())
    {
        return Err(MessageActionError::UnsupportedButton);
    }
    Ok(())
}

impl CurrentMessageUser<'_> {
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
        let mut value = json!({"type":"message", "message_ts":self.0.owner.message_ts,
            "channel_id":self.0.owner.channel_id, "is_ephemeral":self.0.is_ephemeral});
        if let Some(ts) = &self.0.ephemeral_thread_ts {
            value["thread_ts"] = json!(ts);
        }
        value
    }
}

#[cfg(test)]
mod tests;
