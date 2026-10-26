//! I8j+ callbacks retain an immutable date while delivery reads current state.
//! Mounted display state and native occurrence/lease authority stay in the renderer.
use super::*;
use std::collections::BTreeSet;

/// Identity attestation for an initialized date parent, not a captured value.
/// The renderer supplies this only while the same owned field remains mounted.
#[derive(Clone, Debug, PartialEq)]
pub struct DateParent {
    owner: Value,
    key: InputKey,
}

pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "datepicker"
        && matches!(kind, InputKind::Date)
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
}

impl HomeForm {
    pub fn date_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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

    fn date_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.date_field(key) else {
            return false;
        };
        valid_state_value(field)
            && self.wire.get(key) == Some(&values::wire(field))
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(super) fn date_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|block| {
                    block["type"] == "input" && block["element"]["type"] == "datepicker"
                })
                .all(|block| {
                    self.date_state_valid(&InputKey {
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
            .filter(|(_, f)| matches!(f.kind, InputKind::Date))
            .all(|(key, _)| self.date_state_valid(key))
    }

    /// Fresh display only: mapped savedDate || initialDate || undefined.
    /// This neither changes the parent nor widens raw saved-state ingestion.
    pub fn date_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.date_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let field = &self.fields[key];
        Ok(self.wire[key]["selected_date"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| field.raw["initial_date"].as_str().filter(|v| !v.is_empty()))
            .map(str::to_owned))
    }

    pub fn date_parent(&self, key: &InputKey) -> Result<DateParent, FormError> {
        self.date_seed(key)?;
        Ok(DateParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
        })
    }

    pub(super) fn date_parent_keys(
        &self,
        next: &Self,
        parents: &[DateParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.date_state_valid(&parent.key)
                || !next.date_state_valid(&parent.key)
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// A structurally valid current native buffer can clear a safety warning
    /// without committing display-only props or an unchanged local date.
    /// The renderer must attest its current mount/sequence and native validity.
    pub fn date_acknowledge_native_buffer(
        &mut self,
        key: &InputKey,
        buffer: Option<&str>,
    ) -> Result<(), FormError> {
        if !self.date_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let mut candidate = self.fields[key].clone();
        candidate.optional = true;
        candidate.value = InputValue::Date(buffer.filter(|v| !v.is_empty()).map(str::to_owned));
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
    /// Same-display suppression belongs to the original leaf/native adapter.
    pub fn date_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.date_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let field = self.date_field(key)?;
        let selected = selected.filter(|v| !v.is_empty());
        let mut candidate = field.clone();
        candidate.optional = true;
        candidate.value = InputValue::Date(selected.clone());
        if values::error(&candidate).is_some() {
            return Err(invalid());
        }
        let initial = field.raw["initial_date"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        self.set(key, InputValue::Date(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::date(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
        )))
    }
}

#[cfg(test)]
mod tests;
