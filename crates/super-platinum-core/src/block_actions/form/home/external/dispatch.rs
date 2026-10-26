//! IKXd/hvzX committed parent authority, separate from fresh display projection.
use super::*;
use std::collections::BTreeSet;
mod multiple;

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalParent {
    owner: Value,
    key: InputKey,
    multiple: bool,
}

impl HomeForm {
    pub(in crate::block_actions::form::home) fn initialize_external_wire(&mut self) {
        for (key, field) in &self.fields {
            if !matches!(
                field.kind,
                InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
            ) {
                continue;
            }
            let has_saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some();
            // Preserve existing raw saved hydration. Only pristine initialized
            // absence has the actual cxJV type-only parent representation.
            let omitted = match &field.value {
                InputValue::External(None) => Some("selected_option"),
                InputValue::ExternalOptions(v)
                    if v.is_empty()
                        && field.raw.get("initial_options").is_none_or(Value::is_null) =>
                {
                    Some("selected_options")
                }
                _ => None,
            };
            if !has_saved {
                if let (Some(name), Some(wire)) = (
                    omitted,
                    self.wire.get_mut(key).and_then(Value::as_object_mut),
                ) {
                    wire.remove(name);
                }
            }
        }
    }

    pub(in crate::block_actions::form::home) fn external_state_valid(
        &self,
        key: &InputKey,
    ) -> bool {
        if self
            .fields
            .get(key)
            .is_some_and(|f| matches!(f.kind, InputKind::MultiExternalSelect { .. }))
        {
            return self.multi_external_state_valid(key);
        }
        let Ok(field) = self.external_field(key) else {
            return false;
        };
        let InputValue::External(selected) = &field.value else {
            return false;
        };
        if !matches!(field.kind, InputKind::ExternalSelect { .. }) || !valid_state_value(field) {
            return false;
        }
        let Ok(parsed) = parse_message_external(&field.raw) else {
            return false;
        };
        let block = self.view["blocks"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["block_id"] == key.block_id));
        if parsed.kind != field.kind
            || block.is_none_or(|b| field.optional != b["optional"].as_bool().unwrap_or(false))
        {
            return false;
        }
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let mut expected = json!({"type":"external_select"});
        if selected.is_some() || wire.get("selected_option").is_some() {
            expected["selected_option"] = selected
                .as_ref()
                .map(|v| v.raw().clone())
                .unwrap_or(Value::Null);
        }
        wire == &expected
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    pub(in crate::block_actions::form::home) fn external_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| {
                    b["type"] == "input"
                        && matches!(
                            b["element"]["type"].as_str(),
                            Some("external_select" | "multi_external_select")
                        )
                })
                .all(|b| {
                    self.external_state_valid(&InputKey {
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
                    InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
                )
            })
            .all(|(key, _)| self.external_state_valid(key))
    }

    fn single_external_state_valid(&self, key: &InputKey) -> bool {
        self.fields
            .get(key)
            .is_some_and(|f| matches!(f.kind, InputKind::ExternalSelect { .. }))
            && self.external_state_valid(key)
    }

    /// Actual iGIl -> v36g/Q1qH -> IKXd fresh display seed, not parent state.
    /// A saved descriptor projects to text/value; a null or omitted parent falls
    /// back to the current full initial descriptor without committing it.
    pub fn external_seed(&self, key: &InputKey) -> Result<Option<ExternalOption>, FormError> {
        if !self.single_external_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        if let Some(raw) = self.wire[key]
            .get("selected_option")
            .filter(|v| !v.is_null())
        {
            return ExternalOption::parse(&json!({"text":raw["text"], "value":raw["value"]}))
                .map(Some);
        }
        self.fields[key]
            .raw
            .get("initial_option")
            .filter(|v| !v.is_null())
            .map(ExternalOption::parse)
            .transpose()
    }

    /// First render of an explicitly new parent, before the original initializer
    /// effect supplies saved props. The renderer must attest that lifecycle;
    /// catalog absence is not evidence of a new parent. Existing admitted raw
    /// saved descriptors still project through Q1qH; absent/null saved input
    /// preserves the full initial descriptor. This does not widen raw ingress.
    pub fn external_initial_seed(
        &self,
        key: &InputKey,
    ) -> Result<Option<ExternalOption>, FormError> {
        if !self.single_external_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        if let Some(raw) = self.view["state"]["values"]
            .get(&key.block_id)
            .and_then(|v| v.get(&key.action_id))
            .and_then(|v| v.get("selected_option"))
            .filter(|v| !v.is_null())
        {
            // The current raw saved parser already admitted this descriptor.
            ExternalOption::parse(raw)?;
            return ExternalOption::parse(&json!({"text":raw["text"], "value":raw["value"]}))
                .map(Some);
        }
        self.fields[key]
            .raw
            .get("initial_option")
            .filter(|v| !v.is_null())
            .map(ExternalOption::parse)
            .transpose()
    }

    pub fn external_parent(&self, key: &InputKey) -> Result<ExternalParent, FormError> {
        self.external_seed(key)?;
        Ok(ExternalParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: false,
        })
    }

    pub(in crate::block_actions::form::home) fn external_parent_keys(
        &self,
        next: &Self,
        parents: &[ExternalParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.external_state_valid(&parent.key)
                || !next.external_state_valid(&parent.key)
                || [self, next].iter().any(|form| {
                    matches!(
                        form.fields[&parent.key].kind,
                        InputKind::MultiExternalSelect { .. }
                    ) != parent.multiple
                })
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }

    /// Accept an exact caller-owned suggestion descriptor or explicit clear.
    /// Query/result/account/visit provenance remains the renderer's authority.
    /// Empty option.value is a selected object, never a clear operation.
    pub fn external_input(
        &mut self,
        key: &InputKey,
        selected: Option<ExternalOption>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.single_external_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let initial = self.fields[key]
            .raw
            .get("initial_option")
            .filter(|v| !v.is_null())
            .cloned();
        let placeholder = self.fields[key]
            .raw
            .get("placeholder")
            .filter(|v| !v.is_null())
            .cloned();
        self.set(key, InputValue::External(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::external(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
            placeholder,
        )))
    }
}

#[cfg(test)]
mod tests;
