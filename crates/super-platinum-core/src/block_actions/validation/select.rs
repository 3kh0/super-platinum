//! Captured z3Rn static-select actions in ordinary message containers.
//! Capturing is pure: confirmation cancellation simply discards this value.
use super::*;
use crate::block_actions::form;

#[derive(Clone)]
pub struct CapturedMessageSelect {
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
pub struct CurrentMessageSelect<'a>(&'a CapturedMessageSelect);

impl CapturedMessageSelect {
    /// Borrow the unique canonical select after checking its message and action
    /// metadata. Callers must separately validate the full input model/options;
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
        validate_select(element, &owner.self_user_id)?;
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
        selected_option: Option<&Value>,
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        // This shared parser checks both option sources, duplicate identities,
        // and exact initial-option membership while retaining raw metadata.
        let options = form::static_select_options(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        if let Some(initial) = element
            .get("initial_option")
            .filter(|value| !value.is_null())
        {
            if !options.iter().any(|option| &option.raw == initial) {
                return Err(MessageActionError::MalformedButton);
            }
        }
        if selected_option
            .is_some_and(|selected| !options.iter().any(|option| &option.raw == selected))
        {
            return Err(MessageActionError::MalformedButton);
        }
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
            "type":"static_select", "block_id":block_id, "action_id":action_id,
            "selected_option":selected_option,
        });
        // Captured z3Rn includes these composition objects verbatim. A null
        // initial option is absent, matching its truthiness check.
        for key in ["initial_option", "placeholder", "confirm"] {
            if let Some(value) = element.get(key).filter(|value| !value.is_null()) {
                action[key] = value.clone();
            }
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
    pub fn selected_option(&self) -> Option<&Value> {
        self.action
            .get("selected_option")
            .filter(|value| !value.is_null())
    }

    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageSelect<'a>, MessageActionError> {
        if owner != &self.owner {
            return Err(MessageActionError::WrongMessage);
        }
        validate_owner(owner, message)?;
        if !revision::matches(message, self.revision.as_ref()) {
            return Err(MessageActionError::StaleMessage);
        }
        let element = find_button(message, &self.block_id, &self.action_id)?;
        validate_select(element, &owner.self_user_id)?;
        // The frozen revision already proves the option list's structure and
        // uniqueness. Re-resolve the exact raw selected option without cloning.
        if let Some(selected) = self.selected_option() {
            let direct = element
                .get("options")
                .and_then(Value::as_array)
                .is_some_and(|options| options.contains(selected));
            let grouped = element
                .get("option_groups")
                .and_then(Value::as_array)
                .is_some_and(|groups| {
                    groups.iter().any(|group| {
                        group
                            .get("options")
                            .and_then(Value::as_array)
                            .is_some_and(|options| options.contains(selected))
                    })
                });
            if !direct && !grouped {
                return Err(MessageActionError::StaleMessage);
            }
        }
        Ok(CurrentMessageSelect(self))
    }
}

fn validate_select(element: &Value, viewer: &str) -> Result<(), MessageActionError> {
    if element.get("type").and_then(Value::as_str) != Some("static_select") {
        return Err(MessageActionError::UnsupportedButton);
    }
    validate_recipient(element, viewer)?;
    validate_confirmation(element)?;
    if element
        .get("placeholder")
        .is_some_and(|value| !text_object(Some(value), false))
    {
        return Err(MessageActionError::MalformedButton);
    }
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

impl CurrentMessageSelect<'_> {
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

#[cfg(test)]
mod eligibility_tests {
    use super::*;

    fn fixture() -> (MessageActionOwner, Message) {
        let owner = MessageActionOwner {
            account_epoch: 1,
            self_user_id: "U1".into(),
            team_id: "T1".into(),
            channel_id: "C1".into(),
            message_ts: "1719800000.000001".into(),
        };
        let message = serde_json::from_value(json!({"channel":"C1","ts":owner.message_ts,
            "bot_id":"B1","blocks":[{"type":"actions","block_id":"choose","elements":[
                {"type":"static_select","action_id":"pick","options":[
                    {"text":{"type":"plain_text","text":"A"},"value":"a"}]}]}]}))
        .unwrap();
        (owner, message)
    }

    #[test]
    fn metadata_eligibility_borrows_the_exact_canonical_element() {
        let (owner, message) = fixture();
        let raw =
            CapturedMessageSelect::validate_element(&owner, &message, "choose", "pick").unwrap();
        assert!(std::ptr::eq(raw, &message.blocks[0]["elements"][0]));
        assert!(CapturedMessageSelect::capture(owner, &message, "choose", "pick", None).is_ok());
    }

    #[test]
    fn metadata_eligibility_preserves_identity_recipient_and_context_gates() {
        for case in 0..9 {
            let (owner, mut message) = fixture();
            match case {
                0 => message.blocks[0]["elements"][0]["visible_to_user_ids"] = json!(["U2"]),
                1 => message.blocks[0]["elements"][0]["confirm"] = Value::Null,
                2 => message.blocks[0]["elements"][0]["third_party_auth"] = json!({}),
                3 => message
                    .extra
                    .insert("is_ephemeral".into(), json!("true"))
                    .map(|_| ())
                    .unwrap_or(()),
                4 => {
                    message.extra.insert("is_ephemeral".into(), json!(true));
                    message.thread_ts = Some("bad".into());
                }
                5 => message.bot_id = Some("BSLACKBOT".into()),
                6 => message
                    .extra
                    .insert("function_execution_id".into(), json!(false))
                    .map(|_| ())
                    .unwrap_or(()),
                7 => {
                    let duplicate = message.blocks[0].clone();
                    message.blocks.push(duplicate);
                }
                8 => message.channel = Some("C2".into()),
                _ => unreachable!(),
            }
            let eligibility =
                CapturedMessageSelect::validate_element(&owner, &message, "choose", "pick");
            let capture = CapturedMessageSelect::capture(owner, &message, "choose", "pick", None);
            assert!(eligibility.is_err(), "case {case}");
            assert_eq!(eligibility.err(), capture.err(), "case {case}");
        }
    }
}
