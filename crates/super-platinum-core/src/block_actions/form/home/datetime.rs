//! Home 7XR/ epoch callbacks. Local composite parsing and occurrence custody
//! belong to the renderer; no Modal minute/DST normalization is used here.
use super::*;
use std::collections::BTreeSet;

/// Identity of an initialized composite parent, never a captured value.
#[derive(Clone, Debug, PartialEq)]
pub struct DatetimeParent {
    owner: Value,
    key: InputKey,
}

// ECMAScript TimeClip: milliseconds are bounded by 8.64e15. Epoch seconds
// remain exact integers throughout this interval. This is not a UI range claim.
fn valid_epoch(epoch: i64) -> bool {
    (-8_640_000_000_000..=8_640_000_000_000).contains(&epoch)
}
pub(super) fn error(field: &FormField) -> Option<String> {
    match (&field.kind, &field.value) {
        (InputKind::DateTime, InputValue::DateTime(None)) => {
            (!field.optional).then(|| "Complete this required field.".into())
        }
        (InputKind::DateTime, InputValue::DateTime(Some(epoch))) if valid_epoch(*epoch) => None,
        _ => Some("Enter a valid date and time.".into()),
    }
}
pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "datetimepicker"
        && matches!(kind, InputKind::DateTime)
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
}
impl HomeForm {
    pub fn datetime_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.view["blocks"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["block_id"] == key.block_id))
            .ok_or_else(invalid)?;
        if !supported(block, &field.raw, &field.kind)
            || block.get("element") != Some(&field.raw)
            || field.raw["action_id"] != key.action_id
            || field.optional != block["optional"].as_bool().unwrap_or(false)
        {
            return Err(invalid());
        }
        let (kind, initial) = parse::input(&field.raw, true, None, None)?;
        if kind != field.kind {
            return Err(invalid());
        }
        let mut candidate = field.clone();
        candidate.optional = true;
        candidate.value = initial;
        if error(&candidate).is_some() {
            return Err(invalid());
        }
        Ok(field)
    }
    /// Source constructor initialization differs from a callback's explicit null.
    /// Raw saved-state ingestion retains the existing parser projection.
    pub(super) fn initialize_datetime_wire(&mut self) {
        for (key, field) in &mut self.fields {
            if !matches!(field.kind, InputKind::DateTime) {
                continue;
            }
            let has_saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some();
            if !has_saved
                && field.raw["initial_date_time"]
                    .as_i64()
                    .is_none_or(|v| v == 0)
            {
                field.value = InputValue::DateTime(None);
                field.initial_value = InputValue::DateTime(None);
                self.wire
                    .insert(key.clone(), json!({"type":"datetimepicker"}));
            }
            field.error = error(field);
        }
    }
    pub(super) fn datetime_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.datetime_field(key) else {
            return false;
        };
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let expected = values::wire(field);
        let omitted = matches!(field.value, InputValue::DateTime(None))
            && wire == &json!({"type":"datetimepicker"});
        valid_state_value(field)
            && (wire == &expected || omitted)
            && (self.native_errors.contains_key(key) || field.error == error(field))
    }
    pub(super) fn datetime_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "input" && b["element"]["type"] == "datetimepicker")
                .all(|b| {
                    self.datetime_state_valid(&InputKey {
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
            .filter(|(_, f)| matches!(f.kind, InputKind::DateTime))
            .all(|(key, _)| self.datetime_state_valid(key))
    }
    /// Mapped savedDateTime || initialDateTime || 0, independent of parent wire.
    /// The renderer displays a composite selection only when this is positive.
    pub fn datetime_seed(&self, key: &InputKey) -> Result<i64, FormError> {
        if !self.datetime_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.wire[key]["selected_date_time"]
            .as_i64()
            .filter(|v| *v != 0)
            .or_else(|| {
                self.fields[key].raw["initial_date_time"]
                    .as_i64()
                    .filter(|v| *v != 0)
            })
            .unwrap_or(0))
    }
    pub fn datetime_parent(&self, key: &InputKey) -> Result<DatetimeParent, FormError> {
        self.datetime_seed(key)?;
        Ok(DatetimeParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
        })
    }
    pub(super) fn datetime_parent_keys(
        &self,
        next: &Self,
        parents: &[DatetimeParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.datetime_state_valid(&parent.key)
                || !next.datetime_state_valid(&parent.key)
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }
    /// Only an actually acknowledged canonical composite buffer may clear a
    /// native safety marker. No parent value, dirty flag or revision is changed.
    pub fn datetime_acknowledge_native_buffer(
        &mut self,
        key: &InputKey,
        canonical: Option<i64>,
    ) -> Result<(), FormError> {
        if !self.datetime_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        if canonical.is_some_and(|v| !valid_epoch(v)) {
            return Err(invalid());
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.error = error(field);
        Ok(())
    }
    /// Direct callback boundary: zero maps to null. Parent is stored before
    /// required validation suppresses a new event. Leaf reachability is separate.
    pub fn datetime_input(
        &mut self,
        key: &InputKey,
        selected: Option<i64>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.datetime_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        if selected.is_some_and(|v| !valid_epoch(v)) {
            return Err(invalid());
        }
        let initial = self.fields[key].raw["initial_date_time"]
            .as_i64()
            .filter(|v| *v != 0);
        let selected = selected.filter(|v| *v != 0);
        self.set(key, InputValue::DateTime(selected))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::datetime(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}
#[cfg(test)]
mod tests;
