//! o5++ / QjSI / cxJV Home parent state, not directory authority.
use super::*;
use std::collections::BTreeSet;
mod multiple;

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationParent {
    owner: Value,
    key: InputKey,
    multiple: bool,
}

impl HomeForm {
    pub(in crate::block_actions::form::home) fn initialize_conversation_wire(&mut self) {
        for (key, field) in &self.fields {
            if !matches!(
                field.kind,
                InputKind::Conversation | InputKind::MultiConversation { .. }
            ) {
                continue;
            }
            let has_saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some();
            // Pristine cxJV state has selection only, never the callback bit.
            // Existing raw saved hydration is deliberately left unchanged.
            if !has_saved {
                if let Some(wire) = self.wire.get_mut(key).and_then(Value::as_object_mut) {
                    wire.remove("response_url_enabled");
                    if matches!(field.value, InputValue::Conversation(None)) {
                        wire.remove("selected_conversation");
                    }
                    if matches!(&field.value, InputValue::Conversations(v) if v.is_empty())
                        && field
                            .raw
                            .get("initial_conversations")
                            .is_none_or(Value::is_null)
                    {
                        wire.remove("selected_conversations");
                    }
                }
            }
        }
    }

    pub(in crate::block_actions::form::home) fn conversation_state_valid(
        &self,
        key: &InputKey,
    ) -> bool {
        let Ok(field) = self.conversation_field(key) else {
            return false;
        };
        if !valid_state_value(field) {
            return false;
        }
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let expected = match (&field.kind, &field.value) {
            (InputKind::Conversation, InputValue::Conversation(selected)) => {
                let mut expected = json!({"type":"conversations_select"});
                if selected.is_some() || wire.get("selected_conversation").is_some() {
                    expected["selected_conversation"] = json!(selected);
                }
                // Prop changes do not rewrite the single callback-time response bit.
                if wire.get("response_url_enabled") == Some(&Value::Bool(true)) {
                    expected["response_url_enabled"] = json!(true);
                }
                expected
            }
            (InputKind::MultiConversation { .. }, InputValue::Conversations(selected)) => {
                let mut expected = json!({"type":"multi_conversations_select"});
                if !selected.is_empty() || wire.get("selected_conversations").is_some() {
                    expected["selected_conversations"] = json!(selected);
                }
                expected
            }
            _ => return false,
        };
        wire == &expected
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(in crate::block_actions::form::home) fn conversation_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| {
                    b["type"] == "input"
                        && matches!(
                            b["element"]["type"].as_str(),
                            Some("conversations_select" | "multi_conversations_select")
                        )
                })
                .all(|b| {
                    self.conversation_state_valid(&InputKey {
                        block_id: b["block_id"].as_str().unwrap_or_default().into(),
                        action_id: b["element"]["action_id"]
                            .as_str()
                            .unwrap_or_default()
                            .into(),
                    })
                })
        }) && self
            .fields
            .iter()
            .filter(|(_, f)| {
                matches!(
                    f.kind,
                    InputKind::Conversation | InputKind::MultiConversation { .. }
                )
            })
            .all(|(key, _)| self.conversation_state_valid(key))
    }

    /// Actual iGIl nullish projection then o5++ constructor, for a fresh widget.
    /// A null/omitted parent falls back to current initial without committing it.
    pub fn conversation_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.conversation_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::Conversation)
            || self.native_errors.contains_key(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.wire[key]
            .get("selected_conversation")
            .and_then(Value::as_str)
            .or_else(|| {
                self.fields[key].raw["initial_conversation"]
                    .as_str()
                    .filter(|s| !s.is_empty())
            })
            .map(str::to_owned))
    }

    pub fn conversation_parent(&self, key: &InputKey) -> Result<ConversationParent, FormError> {
        self.conversation_seed(key)?;
        Ok(ConversationParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: false,
        })
    }

    pub(in crate::block_actions::form::home) fn conversation_parent_keys(
        &self,
        next: &Self,
        parents: &[ConversationParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.conversation_state_valid(&parent.key)
                || !next.conversation_state_valid(&parent.key)
                || matches!(
                    self.fields[&parent.key].kind,
                    InputKind::MultiConversation { .. }
                ) != parent.multiple
                || matches!(
                    next.fields[&parent.key].kind,
                    InputKind::MultiConversation { .. }
                ) != parent.multiple
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// Admit a caller-owned conversation/member ID or explicit clear. Native
    /// directory/filter/account eligibility remains the caller's responsibility.
    /// Required and quiet callbacks commit their parent before suppressing action.
    pub fn conversation_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.conversation_state_valid(key)
            || !matches!(self.fields[key].kind, InputKind::Conversation)
        {
            return Err(FormError::ConflictingUpdate);
        }
        if selected
            .as_deref()
            .is_some_and(|id| !valid_message_conversation(id))
        {
            return Err(invalid());
        }
        let initial = self.fields[key].raw["initial_conversation"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let response = self.fields[key].raw.get("response_url_enabled") == Some(&Value::Bool(true));
        self.set(key, InputValue::Conversation(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::conversation(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
            response,
        )))
    }
}

#[cfg(test)]
mod tests;
