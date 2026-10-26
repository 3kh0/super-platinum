//! QjSI ordered callbacks and iGIl projected parent seed; no directory authority.
use super::*;

impl HomeForm {
    /// Explicit parent [] wins; an omitted parent falls back to current initial.
    /// Existing raw saved null ingestion remains the generic parser's policy.
    pub fn multi_conversation_seed(&self, key: &InputKey) -> Result<Vec<String>, FormError> {
        if !self.conversation_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiConversation { .. })
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        match self.wire[key].get("selected_conversations") {
            Some(_) => match &self.fields[key].value {
                InputValue::Conversations(v) => Ok(v.clone()),
                _ => Err(invalid()),
            },
            None => match parse_message_conversations(&self.fields[key].raw)?.value {
                InputValue::Conversations(v) => Ok(v),
                _ => Err(invalid()),
            },
        }
    }

    pub fn multi_conversation_parent(
        &self,
        key: &InputKey,
    ) -> Result<ConversationParent, FormError> {
        self.multi_conversation_seed(key)?;
        Ok(ConversationParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: true,
        })
    }

    /// Structural identity is validated before mutation; required/overmax edits
    /// commit before suppressing a new event. The renderer owns directory/filter eligibility.
    pub fn multi_conversation_input(
        &mut self,
        key: &InputKey,
        selected: Vec<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.conversation_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::MultiConversation { .. })
        {
            return Err(FormError::ConflictingUpdate);
        }
        let mut candidate = self.fields[key].clone();
        candidate.value = InputValue::Conversations(selected.clone());
        if !valid_state_value(&candidate) {
            return Err(invalid());
        }
        let initial = self.fields[key]
            .raw
            .get("initial_conversations")
            .filter(|v| v.is_array())
            .cloned();
        self.set(key, InputValue::Conversations(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::multi_conversation(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}

#[cfg(test)]
mod tests;
