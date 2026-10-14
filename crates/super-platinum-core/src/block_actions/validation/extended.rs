//! Captured hGZA multi-user and UVRG overflow actions. No dispatch or URL open.
use super::*;
use crate::block_actions::form;

#[derive(Clone)]
struct CapturedElement {
    owner: MessageActionOwner,
    block_id: String,
    action_id: String,
    revision: Arc<Value>,
    action: Value,
    service_id: String,
    function_execution_id: Option<String>,
    is_ephemeral: bool,
    ephemeral_thread_ts: Option<String>,
    url: Option<String>,
}

impl CapturedElement {
    fn validate<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
        kind: &str,
    ) -> Result<&'a Value, MessageActionError> {
        validate_owner(owner, message)?;
        if block_id.trim().is_empty() || action_id.trim().is_empty() {
            return Err(MessageActionError::MissingButton);
        }
        if block_id.starts_with("widget:") || client_owned_action(block_id, action_id) {
            return Err(MessageActionError::UnsupportedButton);
        }
        let element = find_button(message, block_id, action_id)?;
        if element.get("type").and_then(Value::as_str) != Some(kind) {
            return Err(MessageActionError::UnsupportedButton);
        }
        validate_recipient(element, &owner.self_user_id)?;
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
        let service = message
            .bot_id
            .as_deref()
            .filter(|id| !id.is_empty() && !id.chars().any(|c| c.is_whitespace() || c.is_control()))
            .ok_or(MessageActionError::MissingServiceIdentity)?;
        if service == "BSLACKBOT" {
            return Err(MessageActionError::UnsupportedButton);
        }
        optional_string(message.extra.get("function_execution_id"))?;
        match message.extra.get("is_ephemeral") {
            None | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) if message.thread_ts.as_deref().is_none_or(server_ts) => {}
            Some(Value::Bool(true)) => return Err(MessageActionError::InvalidOwner),
            _ => return Err(MessageActionError::MalformedButton),
        }
        Ok(element)
    }

    fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        mut action: Value,
        element: &Value,
        url: Option<String>,
    ) -> Self {
        if let Some(confirm) = element.get("confirm") {
            action["confirm"] = confirm.clone();
        }
        let is_ephemeral = message
            .extra
            .get("is_ephemeral")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self {
            owner,
            block_id: block_id.into(),
            action_id: action_id.into(),
            revision: Arc::new(revision::capture(message)),
            action,
            service_id: message.bot_id.clone().expect("validated service"),
            function_execution_id: message
                .extra
                .get("function_execution_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            is_ephemeral,
            ephemeral_thread_ts: if is_ephemeral {
                message.thread_ts.clone()
            } else {
                None
            },
            url,
        }
    }

    fn revalidate(
        &self,
        owner: &MessageActionOwner,
        message: &Message,
        kind: &str,
    ) -> Result<(), MessageActionError> {
        if owner != &self.owner {
            return Err(MessageActionError::WrongMessage);
        }
        validate_owner(owner, message)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        Self::validate(owner, message, &self.block_id, &self.action_id, kind)?;
        Ok(())
    }

    fn container(&self) -> Value {
        let mut container = json!({"type":"message", "message_ts":self.owner.message_ts,
            "channel_id":self.owner.channel_id, "is_ephemeral":self.is_ephemeral});
        if let Some(ts) = &self.ephemeral_thread_ts {
            container["thread_ts"] = json!(ts);
        }
        container
    }
}

#[derive(Clone)]
pub struct CapturedMessageUsers(CapturedElement);
pub struct CurrentMessageUsers<'a>(&'a CapturedElement);

impl CapturedMessageUsers {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element =
            CapturedElement::validate(owner, message, block_id, action_id, "multi_users_select")?;
        form::parse_message_users(element).map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }

    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected: &[String],
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field =
            form::parse_message_users(element).map_err(|_| MessageActionError::MalformedButton)?;
        form::validate_message_users(&field, selected)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let mut action = json!({"type":"multi_users_select", "block_id":block_id,
            "action_id":action_id, "selected_users":selected});
        // hGZA emits initialUsers when supplied, including an explicit empty array.
        if let Some(initial) = element.get("initial_users").filter(|v| !v.is_null()) {
            action["initial_users"] = initial.clone();
        }
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, None,
        )))
    }

    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn selected_users(&self) -> Vec<String> {
        self.0.action["selected_users"]
            .as_array()
            .expect("captured users")
            .iter()
            .map(|v| v.as_str().expect("validated user").to_owned())
            .collect()
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageUsers<'a>, MessageActionError> {
        self.0.revalidate(owner, message, "multi_users_select")?;
        Ok(CurrentMessageUsers(&self.0))
    }
}

#[derive(Clone)]
pub struct CapturedMessageChannels(CapturedElement);
pub struct CurrentMessageChannels<'a>(&'a CapturedElement);

impl CapturedMessageChannels {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element = CapturedElement::validate(
            owner,
            message,
            block_id,
            action_id,
            "multi_channels_select",
        )?;
        form::parse_message_channels(element).map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }

    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected: &[String],
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field = form::parse_message_channels(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        form::validate_message_channels(&field, selected)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let mut action = json!({"type":"multi_channels_select", "block_id":block_id,
            "action_id":action_id, "selected_channels":selected});
        // z+fW emits initialChannels when supplied, including an explicit empty array.
        if let Some(initial) = element.get("initial_channels").filter(|v| !v.is_null()) {
            action["initial_channels"] = initial.clone();
        }
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, None,
        )))
    }

    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn selected_channels(&self) -> Vec<String> {
        self.0.action["selected_channels"]
            .as_array()
            .expect("captured channels")
            .iter()
            .map(|v| v.as_str().expect("validated channel").to_owned())
            .collect()
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageChannels<'a>, MessageActionError> {
        self.0.revalidate(owner, message, "multi_channels_select")?;
        Ok(CurrentMessageChannels(&self.0))
    }
}

#[derive(Clone)]
pub struct CapturedMessageConversations(CapturedElement);
pub struct CurrentMessageConversations<'a>(&'a CapturedElement);

impl CapturedMessageConversations {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element = CapturedElement::validate(
            owner,
            message,
            block_id,
            action_id,
            "multi_conversations_select",
        )?;
        form::parse_message_conversations(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }

    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected: &[String],
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field = form::parse_message_conversations(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        form::validate_message_conversations(&field, selected)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let mut action = json!({"type":"multi_conversations_select", "block_id":block_id,
            "action_id":action_id, "selected_conversations":selected});
        // QjSI emits initialConversations when supplied, including an explicit empty array.
        if let Some(initial) = element
            .get("initial_conversations")
            .filter(|v| !v.is_null())
        {
            action["initial_conversations"] = initial.clone();
        }
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, None,
        )))
    }

    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn selected_conversations(&self) -> Vec<String> {
        self.0.action["selected_conversations"]
            .as_array()
            .expect("captured conversations")
            .iter()
            .map(|v| v.as_str().expect("validated conversation").to_owned())
            .collect()
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageConversations<'a>, MessageActionError> {
        self.0
            .revalidate(owner, message, "multi_conversations_select")?;
        Ok(CurrentMessageConversations(&self.0))
    }
}

#[derive(Clone)]
pub struct CapturedMessageOverflow(CapturedElement);
pub struct CurrentMessageOverflow<'a>(&'a CapturedElement);

impl CapturedMessageOverflow {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element = CapturedElement::validate(owner, message, block_id, action_id, "overflow")?;
        form::parse_message_overflow(element).map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected_value: &str,
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let options = form::parse_message_overflow(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let selected = options
            .iter()
            .find(|option| option.value == selected_value)
            .ok_or(MessageActionError::MalformedButton)?;
        let action = json!({"type":"overflow", "block_id":block_id, "action_id":action_id,
            "selected_option":{"text":selected.raw["text"], "value":selected.value}});
        let url = selected
            .raw
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, url,
        )))
    }
    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn url(&self) -> Option<&str> {
        self.0.url.as_deref()
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageOverflow<'a>, MessageActionError> {
        self.0.revalidate(owner, message, "overflow")?;
        Ok(CurrentMessageOverflow(&self.0))
    }
}

macro_rules! current_accessors {
    ($name:ident) => {
        impl $name<'_> {
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
                self.0.container()
            }
        }
    };
}
current_accessors!(CurrentMessageUsers);
current_accessors!(CurrentMessageChannels);
current_accessors!(CurrentMessageConversations);
current_accessors!(CurrentMessageOverflow);

impl CurrentMessageOverflow<'_> {
    pub fn url(&self) -> Option<&str> {
        self.0.url.as_deref()
    }
}

#[cfg(test)]
mod tests;
