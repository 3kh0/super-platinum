//! s+nw retains owned selected objects across prop changes. New rows are always
//! resolved from current source; this is not an arbitrary retained-array input.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct CheckboxSelection {
    owner: Value,
    key: InputKey,
    selected: Vec<Value>,
}
impl CheckboxSelection {
    pub fn selected(&self) -> &[Value] {
        &self.selected
    }
}

pub(super) fn owner(view: &Value) -> Value {
    Value::Object(
        ["id", "app_id", "app_installed_team_id", "bot_id"]
            .into_iter()
            .filter_map(|key| view.get(key).map(|value| (key.into(), value.clone())))
            .collect(),
    )
}
pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "checkboxes"
        && matches!(kind, InputKind::Checkboxes { .. })
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
        && super::super::message_multi::choice_options(element).is_ok()
}

fn selected_values(selected: &[Value]) -> Result<Vec<String>, FormError> {
    if selected.is_empty() {
        return Ok(vec![]);
    }
    // Reuse existing bounded option grammar and uniqueness, without requiring
    // survivors to remain in the current options inventory.
    super::super::message_multi::choice_options(&json!({"type":"checkboxes","options":selected}))
        .map(|options| options.into_iter().map(|option| option.value).collect())
}

impl HomeForm {
    pub fn checkbox_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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

    /// Seed a new mounted selection from trusted current initial/committed wire.
    /// This does not model the original saved-descriptor constructor conversion.
    pub fn checkbox_selection(&self, key: &InputKey) -> Result<CheckboxSelection, FormError> {
        if !self.checkbox_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(CheckboxSelection {
            owner: owner(&self.view),
            key: key.clone(),
            selected: self.wire[key]["selected_options"]
                .as_array()
                .unwrap()
                .clone(),
        })
    }

    /// Apply one current row occurrence. A clone can be used with a cloned form
    /// to validate a candidate before the renderer commits its owner-bound edit.
    /// The renderer remains responsible for account/visit/revision/native custody.
    pub fn checkbox_input(
        &mut self,
        key: &InputKey,
        selection: &mut CheckboxSelection,
        option_index: usize,
        checked: bool,
    ) -> Result<Option<StaticEvent>, FormError> {
        let field = self.checkbox_field(key)?;
        if selection.owner != owner(&self.view)
            || &selection.key != key
            || !self.checkbox_state_valid(key)
        {
            return Err(FormError::ConflictingUpdate);
        }
        let InputKind::Checkboxes { options } = &field.kind else {
            return Err(FormError::ValueTypeMismatch);
        };
        let option = options.get(option_index).ok_or_else(invalid)?;
        let mut next = selection.selected.clone();
        let present = next.iter().any(|raw| raw["value"] == option.value);
        if checked == present {
            return Err(invalid());
        }
        if checked {
            next.push(option.raw.clone());
        } else {
            next.retain(|raw| raw["value"] != option.value);
        }
        let values = selected_values(&next)?;
        let wire = json!({"type":"checkboxes", "selected_options":next});
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.value = InputValue::Selected(values);
        field.dirty = true;
        self.wire.insert(key.clone(), wire);
        let error = self.checkbox_error(key);
        self.fields.get_mut(key).unwrap().error = error.clone();
        selection.selected = next;
        if self.dispatch(key) == Some(Dispatch::Quiet) || error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::checkbox(
            self.view.clone(),
            key.clone(),
            selection.selected.clone(),
        )))
    }

    pub(super) fn checkbox_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.checkbox_field(key) else {
            return false;
        };
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let Some(selected) = wire["selected_options"].as_array() else {
            return false;
        };
        wire.as_object().is_some_and(|object| object.len() == 2)
            && wire["type"] == "checkboxes"
            && selected_values(selected)
                .is_ok_and(|values| field.value == InputValue::Selected(values))
    }

    pub(super) fn checkbox_error(&self, key: &InputKey) -> Option<String> {
        let field = &self.fields[key];
        (matches!(&field.value, InputValue::Selected(values) if values.is_empty())
            && !field.optional)
            .then(|| "Complete this required field.".into())
    }

    pub(super) fn checkbox_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|block| {
                    block["type"] == "input" && block["element"]["type"] == "checkboxes"
                })
                .all(|block| {
                    let (Some(block_id), Some(action_id)) = (
                        block["block_id"].as_str(),
                        block["element"]["action_id"].as_str(),
                    ) else {
                        return false;
                    };
                    self.checkbox_state_valid(&InputKey {
                        block_id: block_id.into(),
                        action_id: action_id.into(),
                    })
                })
        })
    }

    pub(super) fn refresh_checkbox_errors(&mut self) {
        let errors: Vec<_> = self
            .fields
            .iter()
            .filter(|(key, field)| {
                matches!(field.kind, InputKind::Checkboxes { .. }) && self.checkbox_state_valid(key)
            })
            .map(|(key, _)| (key.clone(), self.checkbox_error(key)))
            .collect();
        for (key, error) in errors {
            self.fields.get_mut(&key).unwrap().error = error;
        }
    }
}

#[cfg(test)]
mod tests;
