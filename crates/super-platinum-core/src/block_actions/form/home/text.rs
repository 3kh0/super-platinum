//! VaQU callback semantics. Native event provenance and debounce belong to the renderer.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTrigger {
    Change,
    Enter,
}

/// Validated callback snapshot, independent of later edits to the same Home form.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEvent {
    view: Value,
    key: InputKey,
    action: Value,
}
impl TextEvent {
    pub(super) fn email(view: Value, key: InputKey, action: Value) -> Self {
        Self { view, key, action }
    }
    pub(super) fn number(view: Value, key: InputKey, action: Value) -> Self {
        Self { view, key, action }
    }
    pub fn action(&self) -> &Value {
        &self.action
    }
    pub fn key(&self) -> &InputKey {
        &self.key
    }
    pub fn same_view(&self, view: &Value) -> bool {
        &self.view == view
    }
}

pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && matches!(kind, InputKind::Text { .. })
        && element["type"] == "plain_text_input"
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|k| element.get(*k).is_none())
}

impl HomeForm {
    pub fn text_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.view["blocks"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|block| block["block_id"] == key.block_id)
            })
            .ok_or_else(invalid)?;
        if !supported(block, &field.raw, &field.kind)
            || field.raw != block["element"]
            || field.raw["action_id"] != key.action_id
            || parse::input(&field.raw, true, None, None)?.0 != field.kind
        {
            return Err(invalid());
        }
        Ok(field)
    }

    /// Updates callback state before validation. `None` means no developer action;
    /// it does not cancel an earlier queued valid event.
    pub fn text_input(
        &mut self,
        key: &InputKey,
        literal: String,
        trigger: TextTrigger,
    ) -> Result<Option<TextEvent>, FormError> {
        let field = self.text_field(key)?;
        let mode = self.dispatch(key).ok_or_else(invalid)?;
        if trigger == TextTrigger::Enter && mode == Dispatch::Quiet {
            return Ok(None);
        }
        let InputValue::Text(previous) = &field.value else {
            return Err(FormError::ValueTypeMismatch);
        };
        let suppress = trigger == TextTrigger::Change
            && mode != Dispatch::Quiet
            && trim_input(previous) == trim_input(&literal);
        let initial = field
            .raw
            .get("initial_value")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let emitted = if trigger == TextTrigger::Enter {
            literal.as_str()
        } else {
            trim_input(&literal)
        };
        let wire = json!({"type":"plain_text_input", "value":if emitted.is_empty() {Value::Null} else {json!(emitted)}});
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.value = InputValue::Text(literal);
        field.dirty = true;
        if !suppress {
            self.wire.insert(key.clone(), wire.clone());
        }
        let error = self.expected_error(key, &self.fields[key]);
        self.fields.get_mut(key).unwrap().error = error.clone();
        let dispatch = mode != Dispatch::Quiet
            && (trigger == TextTrigger::Enter
                || matches!(mode, Dispatch::Character | Dispatch::EnterOrCharacter));
        if suppress || !dispatch || error.is_some() {
            return Ok(None);
        }
        let mut action = wire;
        action["block_id"] = json!(key.block_id);
        action["action_id"] = json!(key.action_id);
        if let Some(initial) = initial {
            action["initial_value"] = json!(initial);
        }
        Ok(Some(TextEvent {
            view: self.view.clone(),
            key: key.clone(),
            action,
        }))
    }

    pub(super) fn expected_error(&self, key: &InputKey, field: &FormField) -> Option<String> {
        if matches!(field.kind, InputKind::Radio { .. }) && self.radio_state_valid(key) {
            return self.radio_error(key);
        }
        if matches!(field.kind, InputKind::Checkboxes { .. }) && self.checkbox_state_valid(key) {
            return self.checkbox_error(key);
        }
        if self
            .dispatch(key)
            .is_some_and(|mode| mode != Dispatch::Quiet)
            && matches!(field.kind, InputKind::Text { .. })
        {
            let mut emitted = field.clone();
            if let Some(wire) = self.wire.get(key) {
                emitted.value = InputValue::Text(wire["value"].as_str().unwrap_or("").into());
            }
            return values::error(&emitted);
        }
        home_error(field)
    }
    pub(super) fn refresh_text_errors(&mut self) {
        let errors: Vec<_> = self
            .fields
            .iter()
            .filter(|(key, field)| {
                self.dispatch(key)
                    .is_some_and(|mode| mode != Dispatch::Quiet)
                    && matches!(field.kind, InputKind::Text { .. })
            })
            .map(|(key, field)| (key.clone(), self.expected_error(key, field)))
            .collect();
        for (key, error) in errors {
            self.fields.get_mut(&key).unwrap().error = error;
        }
    }
}

#[cfg(test)]
mod tests;
