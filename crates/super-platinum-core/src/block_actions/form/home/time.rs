//! 9rwn callbacks retain an immutable time while delivery reads current state.
//! Mounted display state and native occurrence/lease authority stay in the renderer.
use super::*;
use std::collections::BTreeSet;

/// Identity attestation for an initialized time parent, not a captured value.
/// The renderer supplies this only while the same owned field remains mounted.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeParent {
    owner: Value,
    key: InputKey,
}

pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "timepicker"
        && matches!(kind, InputKind::Time { .. })
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
}

impl HomeForm {
    pub fn time_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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
        let mut initial = field.clone();
        initial.optional = true;
        initial.value = parse::input(&field.raw, true, None, None)?.1;
        if values::error(&initial).is_some() {
            return Err(invalid());
        }
        Ok(field)
    }

    fn time_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.time_field(key) else {
            return false;
        };
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        // Initialized Home parent metadata survives timezone-only props. The
        // private wire is authored only by parse/set or trusted reconciliation;
        // the next callback replaces it with current source timezone metadata.
        let mut expected = values::wire(field);
        expected.as_object_mut().unwrap().remove("timezone");
        if let Some(timezone) = wire.get("timezone") {
            if !timezone.is_string() {
                return false;
            }
            expected["timezone"] = timezone.clone();
        }
        valid_state_value(field)
            && wire == &expected
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(super) fn time_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|block| {
                    block["type"] == "input" && block["element"]["type"] == "timepicker"
                })
                .all(|block| {
                    self.time_state_valid(&InputKey {
                        block_id: block["block_id"].as_str().unwrap_or_default().into(),
                        action_id: block["element"]["action_id"]
                            .as_str()
                            .unwrap_or_default()
                            .into(),
                    })
                })
        }) && self
            .fields
            .iter()
            .filter(|(_, f)| matches!(f.kind, InputKind::Time { .. }))
            .all(|(key, _)| self.time_state_valid(key))
    }

    /// Fresh display only: mapped savedTime || initialTime.
    /// This neither changes the parent nor widens raw saved-state ingestion.
    pub fn time_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.time_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let field = &self.fields[key];
        Ok(self.wire[key]["selected_time"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| field.raw["initial_time"].as_str().filter(|v| !v.is_empty()))
            .map(str::to_owned))
    }

    pub fn time_parent(&self, key: &InputKey) -> Result<TimeParent, FormError> {
        self.time_seed(key)?;
        Ok(TimeParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
        })
    }

    pub(super) fn time_parent_keys(
        &self,
        next: &Self,
        parents: &[TimeParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.time_state_valid(&parent.key)
                || !next.time_state_valid(&parent.key)
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// An acknowledged canonical parsed Time buffer can clear a safety warning
    /// without committing display-only props or an unchanged local time.
    /// The renderer must attest its current mount/sequence and native validity.
    pub fn time_acknowledge_native_buffer(
        &mut self,
        key: &InputKey,
        buffer: Option<&str>,
    ) -> Result<(), FormError> {
        if !self.time_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let mut candidate = self.fields[key].clone();
        candidate.optional = true;
        candidate.value = InputValue::Time(buffer.filter(|v| !v.is_empty()).map(str::to_owned));
        if values::error(&candidate).is_some() {
            return Err(invalid());
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.error = values::error(field);
        Ok(())
    }

    /// Direct callback semantics. None/empty stores null before requiredness
    /// suppresses a new event; this does not authorize a required clear UI.
    /// Direct equal callbacks still emit. Native event reachability and draft
    /// parsing belong to the renderer; this accepts only canonical HH:mm.
    pub fn time_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.time_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let field = self.time_field(key)?;
        let selected = selected.filter(|v| !v.is_empty());
        let mut candidate = field.clone();
        candidate.optional = true;
        candidate.value = InputValue::Time(selected.clone());
        if values::error(&candidate).is_some() {
            return Err(invalid());
        }
        let initial = field.raw["initial_time"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let timezone = field.raw["timezone"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        self.set(key, InputValue::Time(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::time(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
            timezone,
        )))
    }
}

#[cfg(test)]
mod tests;
