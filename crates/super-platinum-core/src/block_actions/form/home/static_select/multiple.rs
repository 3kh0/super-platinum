//! etBz input callbacks commit their ordered array before hvzX validation.
use super::*;

impl HomeForm {
    /// Resolve only unique current options at the native boundary. The original
    /// picker prevents duplicate append; fabricated arrays are not UI authority.
    /// Unlike single static selection, an empty-string value is a real option.
    pub fn multi_static_input(
        &mut self,
        key: &InputKey,
        selected: Vec<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        let field = self.multi_static_field(key)?;
        let InputKind::StaticSelect { options, .. } = &field.kind else {
            return Err(FormError::ValueTypeMismatch);
        };
        let mut seen = std::collections::BTreeSet::new();
        let selected_options = selected
            .iter()
            .map(|value| {
                if !seen.insert(value) {
                    return Err(invalid());
                }
                options
                    .iter()
                    .find(|option| &option.value == value)
                    .map(|option| option.raw.clone())
                    .ok_or_else(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let raw = field.raw.clone();
        self.set(key, InputValue::Selected(selected))?;
        // Invalid callbacks do not mint a new occurrence or cancel an older one.
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        let mut action = json!({"type":"multi_static_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_options":selected_options});
        for prop in ["initial_options", "placeholder"] {
            if let Some(value) = raw.get(prop).filter(|value| !value.is_null()) {
                action[prop] = value.clone();
            }
        }
        Ok(Some(StaticEvent {
            view: self.view.clone(),
            key: key.clone(),
            action,
        }))
    }
}

#[cfg(test)]
mod tests;
