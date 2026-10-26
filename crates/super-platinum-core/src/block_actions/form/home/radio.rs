//! DNgy/gMsa stores one current-row callback, while committed raw state can
//! outlive its row. Mounted display values are renderer-owned and never supplied
//! here as arbitrary option objects.
use super::*;

pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "radio_buttons"
        && matches!(kind, InputKind::Radio { .. })
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
        && parse::radio_options(element).is_ok()
}

fn selected_values(selected: &Value) -> Result<Vec<String>, FormError> {
    if selected.is_null() {
        return Ok(vec![]);
    }
    // Structural grammar only: this private committed object need not remain
    // among current rows. New callbacks always resolve a current source index.
    parse::radio_options(&json!({"type":"radio_buttons", "options":[selected]}))
        .map(|options| options.into_iter().map(|option| option.value).collect())
}

impl HomeForm {
    pub fn radio_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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
            || block.get("element") != Some(&field.raw)
            || field.raw["action_id"] != key.action_id
            || field.optional != block["optional"].as_bool().unwrap_or(false)
            || parse::input(&field.raw, true, None, None)?.0 != field.kind
        {
            return Err(invalid());
        }
        Ok(field)
    }

    /// Seed a newly mounted display using mapped saved.value || initial.value.
    /// Existing widgets do not synchronize props through this accessor. This
    /// uses trusted current parent state, not a new raw saved-descriptor parser.
    pub fn radio_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.radio_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.wire[key]["selected_option"]["value"]
            .as_str()
            .filter(|value| !value.is_empty())
            .or_else(|| self.fields[key].raw["initial_option"]["value"].as_str())
            .map(str::to_owned))
    }

    /// Apply a source-resolved radio callback. Native rows supply Some(index).
    /// None models the original direct unresolved/null callback, not a rendered
    /// clear affordance. Foreign indices are errors, never converted into null.
    pub fn radio_input(
        &mut self,
        key: &InputKey,
        option_index: Option<usize>,
    ) -> Result<Option<StaticEvent>, FormError> {
        let field = self.radio_field(key)?;
        if !self.radio_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let InputKind::Radio { options } = &field.kind else {
            return Err(FormError::ValueTypeMismatch);
        };
        let selected = option_index
            .map(|index| {
                options
                    .get(index)
                    .map(|o| o.raw.clone())
                    .ok_or_else(invalid)
            })
            .transpose()?
            .unwrap_or(Value::Null);
        let values = selected_values(&selected)?;
        let initial = field
            .raw
            .get("initial_option")
            .filter(|v| !v.is_null())
            .cloned();
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.value = InputValue::Selected(values);
        field.dirty = true;
        self.wire.insert(
            key.clone(),
            json!({"type":"radio_buttons", "selected_option":selected}),
        );
        let error = self.radio_error(key);
        self.fields.get_mut(key).unwrap().error = error.clone();
        if self.dispatch(key) == Some(Dispatch::Quiet) || error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::radio(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }

    pub(super) fn radio_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.radio_field(key) else {
            return false;
        };
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let Some(selected) = wire.get("selected_option") else {
            return false;
        };
        wire.as_object().is_some_and(|object| object.len() == 2)
            && wire["type"] == "radio_buttons"
            && selected_values(selected)
                .is_ok_and(|values| field.value == InputValue::Selected(values))
    }

    pub(super) fn radio_error(&self, key: &InputKey) -> Option<String> {
        let field = &self.fields[key];
        (matches!(&field.value, InputValue::Selected(values) if values.is_empty())
            && !field.optional)
            .then(|| "Complete this required field.".into())
    }

    pub(super) fn radio_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|block| {
                    block["type"] == "input" && block["element"]["type"] == "radio_buttons"
                })
                .all(|block| {
                    let (Some(block_id), Some(action_id)) = (
                        block["block_id"].as_str(),
                        block["element"]["action_id"].as_str(),
                    ) else {
                        return false;
                    };
                    self.radio_state_valid(&InputKey {
                        block_id: block_id.into(),
                        action_id: action_id.into(),
                    })
                })
        })
    }

    pub(super) fn refresh_radio_errors(&mut self) {
        let errors: Vec<_> = self
            .fields
            .iter()
            .filter(|(key, field)| {
                matches!(field.kind, InputKind::Radio { .. }) && self.radio_state_valid(key)
            })
            .map(|(key, _)| (key.clone(), self.radio_error(key)))
            .collect();
        for (key, error) in errors {
            self.fields.get_mut(&key).unwrap().error = error;
        }
    }
}

#[cfg(test)]
mod tests;
