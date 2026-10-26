use super::*;
use crate::block_actions::form::{ExternalOption, validate_message_external};

impl MessageInputs {
    pub fn commit_external(
        &mut self,
        key: &InputKey,
        selected: Option<&ExternalOption>,
    ) -> Result<Value, MessageStateError> {
        let field = self
            .fields
            .get(key)
            .ok_or(MessageStateError::UnknownField)?;
        let wire = validate_message_external(field, selected)
            .map_err(|_| MessageStateError::InvalidSelection)?;
        self.initialize();
        let field = self.fields.get_mut(key).expect("checked field");
        field.value = InputValue::External(selected.cloned());
        field.dirty = true;
        field.error = None;
        self.committed
            .as_mut()
            .expect("initialized")
            .insert(key.clone(), wire.clone());
        Ok(wire)
    }

    /// Local committed null is distinct from missing state. The incoming Slack
    /// wrapper coalesces null on remount; callers must not treat it as an option.
    pub fn saved_external(&self, key: &InputKey) -> Option<Option<ExternalOption>> {
        let value = self.committed.as_ref()?.get(key)?.get("selected_option")?;
        if value.is_null() {
            Some(None)
        } else {
            ExternalOption::parse(value).ok().map(Some)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn option(value: &str) -> ExternalOption {
        ExternalOption::parse(&json!({"text":{"type":"plain_text","text":value},"value":value,"opaque":{"source":value,"retained":[1,null,false]},"url":"https://example.test/not-navigation"}))
            .unwrap()
    }
    fn message() -> Message {
        serde_json::from_value(json!({"ts":"1.000001","blocks":[{"type":"actions","block_id":"b","elements":[{"type":"external_select","action_id":"a","initial_option":option("initial").raw()},{"type":"external_select","action_id":"s","initial_option":option("sibling").raw()}]}]})).unwrap()
    }
    #[test]
    fn dynamic_commit_full_sibling_and_initial_removal_keep_wire_separate() {
        let mut message = message();
        let key = InputKey {
            block_id: "b".into(),
            action_id: "a".into(),
        };
        let mut inputs = MessageInputs::parse(&message).unwrap();
        assert!(inputs.values().unwrap().is_none());
        assert_eq!(inputs.saved_external(&key), None);
        let remote = option("remote");
        inputs.commit_external(&key, Some(&remote)).unwrap();
        let wire = inputs.values().unwrap().unwrap();
        assert_eq!(wire["b"]["a"]["selected_option"], *remote.raw());
        assert_eq!(wire["b"]["s"]["selected_option"], *option("sibling").raw());
        message.blocks[0]["elements"][0]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
        inputs.reconcile(&message).unwrap();
        assert_eq!(inputs.fields[&key].value, InputValue::External(None));
        assert_eq!(inputs.saved_external(&key), Some(Some(remote)));
        inputs.commit_external(&key, None).unwrap();
        assert_eq!(inputs.saved_external(&key), Some(None));
        inputs.reconcile(&message).unwrap();
        assert_eq!(inputs.fields[&key].value, InputValue::External(None));
        assert!(inputs.values().unwrap().unwrap()["b"]["a"]["selected_option"].is_null());
    }
}
