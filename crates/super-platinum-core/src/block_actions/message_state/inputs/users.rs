use super::*;
use crate::block_actions::form::validate_message_users;

impl MessageInputs {
    /// Commit only after Apply and any confirmation. Cancel discards staging.
    pub fn commit_users(
        &mut self,
        key: &InputKey,
        selected: &[String],
    ) -> Result<Value, MessageStateError> {
        let field = self
            .fields
            .get(key)
            .ok_or(MessageStateError::UnknownField)?;
        let wire = validate_message_users(field, selected)
            .map_err(|_| MessageStateError::InvalidSelection)?;
        self.initialize();
        let field = self.fields.get_mut(key).expect("checked field");
        field.value = InputValue::Users(selected.to_vec());
        field.dirty = true;
        field.error = None;
        self.committed
            .as_mut()
            .expect("initialized")
            .insert(key.clone(), wire.clone());
        Ok(wire)
    }

    /// An explicitly saved empty array outranks initial_users on remount.
    pub fn saved_users(&self, key: &InputKey) -> Option<Vec<String>> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_users")?
            .as_array()?
            .iter()
            .map(|user| user.as_str().map(str::to_owned))
            .collect()
    }
}

#[cfg(test)]
mod tests;
