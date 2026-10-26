//! JQ84 local maximum gate followed by hvzX committed parent validation.
use super::*;

impl HomeForm {
    pub(super) fn multi_external_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.external_field(key) else {
            return false;
        };
        let InputValue::ExternalOptions(selected) = &field.value else {
            return false;
        };
        if !matches!(field.kind, InputKind::MultiExternalSelect { .. }) || !valid_state_value(field)
        {
            return false;
        }
        let Ok(parsed) = parse_message_external_options(&field.raw) else {
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
        let mut expected = json!({"type":"multi_external_select"});
        if !selected.is_empty() || wire.get("selected_options").is_some() {
            expected["selected_options"] =
                json!(selected.iter().map(ExternalOption::raw).collect::<Vec<_>>());
        }
        wire == &expected
            && (self.native_errors.contains_key(key) || field.error == values::error(field))
    }

    /// Fresh retained-parent display. Projection never changes committed descriptors.
    pub fn multi_external_seed(&self, key: &InputKey) -> Result<Vec<ExternalOption>, FormError> {
        self.multi_external_seed_checked(key)?;
        if let Some(raw) = self.wire[key]
            .get("selected_options")
            .filter(|v| !v.is_null())
        {
            return projected(raw);
        }
        initial(&self.fields[key])
    }

    /// Only for an explicitly new parent lifecycle, before initializer saved props.
    /// The existing raw saved parser remains the admission authority.
    pub fn multi_external_initial_seed(
        &self,
        key: &InputKey,
    ) -> Result<Vec<ExternalOption>, FormError> {
        self.multi_external_seed_checked(key)?;
        if let Some(raw) = self.view["state"]["values"]
            .get(&key.block_id)
            .and_then(|v| v.get(&key.action_id))
            .and_then(|v| v.get("selected_options"))
            .filter(|v| !v.is_null())
        {
            return projected(raw);
        }
        initial(&self.fields[key])
    }

    fn multi_external_seed_checked(&self, key: &InputKey) -> Result<(), FormError> {
        if !self.multi_external_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(())
    }

    pub fn multi_external_parent(&self, key: &InputKey) -> Result<ExternalParent, FormError> {
        self.multi_external_seed_checked(key)?;
        Ok(ExternalParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
            multiple: true,
        })
    }

    /// Exact ordered suggestion descriptors. Renderer retains query/source custody
    /// and suppresses deep-equal local updates. Overmax stays local, unlike a
    /// required empty callback which stores [] before parent validation suppresses it.
    pub fn multi_external_input(
        &mut self,
        key: &InputKey,
        selected: Vec<ExternalOption>,
    ) -> Result<Option<StaticEvent>, FormError> {
        if !self.multi_external_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        let field = &self.fields[key];
        let InputKind::MultiExternalSelect {
            max_selected_items, ..
        } = field.kind
        else {
            return Err(FormError::ValueTypeMismatch);
        };
        let mut candidate = field.clone();
        candidate.value = InputValue::ExternalOptions(selected.clone());
        if !valid_state_value(&candidate) {
            return Err(FormError::ValueTypeMismatch);
        }
        if max_selected_items.is_some_and(|max| selected.len() > max) {
            return Ok(None);
        }
        let initial = field
            .raw
            .get("initial_options")
            .filter(|v| !v.is_null())
            .cloned();
        let placeholder = field
            .raw
            .get("placeholder")
            .filter(|v| !v.is_null())
            .cloned();
        self.set(key, InputValue::ExternalOptions(selected.clone()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        Ok(Some(StaticEvent::multi_external(
            self.view.clone(),
            key.clone(),
            selected,
            initial,
            placeholder,
        )))
    }
}

fn initial(field: &FormField) -> Result<Vec<ExternalOption>, FormError> {
    field
        .raw
        .get("initial_options")
        .filter(|v| !v.is_null())
        .map(super::super::super::super::message_external_options::parse_options)
        .transpose()
        .map(Option::unwrap_or_default)
}
fn projected(raw: &Value) -> Result<Vec<ExternalOption>, FormError> {
    let options = super::super::super::super::message_external_options::parse_options(raw)?;
    options
        .iter()
        .map(|o| ExternalOption::parse(&json!({"text":o.raw()["text"],"value":o.value()})))
        .collect()
}

#[cfg(test)]
mod tests;
