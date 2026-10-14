use super::*;
use crate::block_actions::form::{ExternalOption, validate_message_external_options};

impl MessageInputs {
    pub fn commit_external_options(
        &mut self,
        key: &InputKey,
        selected: &[ExternalOption],
    ) -> Result<Value, MessageStateError> {
        let field = self
            .fields
            .get(key)
            .ok_or(MessageStateError::UnknownField)?;
        let wire = validate_message_external_options(field, selected)
            .map_err(|_| MessageStateError::InvalidSelection)?;
        self.initialize();
        let field = self.fields.get_mut(key).expect("checked field");
        field.value = InputValue::ExternalOptions(selected.to_vec());
        field.dirty = true;
        field.error = None;
        self.committed
            .as_mut()
            .expect("initialized")
            .insert(key.clone(), wire.clone());
        Ok(wire)
    }

    /// Missing/null is absent. An explicit ordered [] is a committed clear.
    pub fn saved_external_options(&self, key: &InputKey) -> Option<Vec<ExternalOption>> {
        let wire = self.committed.as_ref()?.get(key)?;
        if wire.get("type")?.as_str()? != "multi_external_select" {
            return None;
        }
        wire.get("selected_options")?
            .as_array()?
            .iter()
            .map(ExternalOption::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    }
}

#[cfg(test)]
mod tests;
