//! z3Rn/etBz callbacks with hvzX input or Te7m wrapper validation semantics.
use super::*;
mod multiple;

#[derive(Clone, Debug, PartialEq)]
pub struct StaticEvent {
    view: Value,
    key: InputKey,
    action: Value,
}
impl StaticEvent {
    pub(super) fn multi_external(
        view: Value,
        key: InputKey,
        selected: Vec<ExternalOption>,
        initial: Option<Value>,
        placeholder: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"multi_external_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_options":selected.iter().map(ExternalOption::raw).collect::<Vec<_>>()});
        if let Some(initial) = initial {
            action["initial_options"] = initial;
        }
        if let Some(placeholder) = placeholder {
            action["placeholder"] = placeholder;
        }
        Self { view, key, action }
    }

    pub(super) fn external(
        view: Value,
        key: InputKey,
        selected: Option<ExternalOption>,
        initial: Option<Value>,
        placeholder: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"external_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_option":selected.as_ref().map(ExternalOption::raw)});
        if let Some(initial) = initial {
            action["initial_option"] = initial;
        }
        if let Some(placeholder) = placeholder {
            action["placeholder"] = placeholder;
        }
        Self { view, key, action }
    }

    pub(super) fn multi_conversation(
        view: Value,
        key: InputKey,
        selected: Vec<String>,
        initial: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"multi_conversations_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_conversations":selected});
        if let Some(initial) = initial {
            action["initial_conversations"] = initial;
        }
        Self { view, key, action }
    }

    pub(super) fn multi_channel(
        view: Value,
        key: InputKey,
        selected: Vec<String>,
        initial: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"multi_channels_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_channels":selected});
        if let Some(initial) = initial {
            action["initial_channels"] = initial;
        }
        Self { view, key, action }
    }

    pub(super) fn conversation(
        view: Value,
        key: InputKey,
        selected: Option<String>,
        initial: Option<String>,
        response_url_enabled: bool,
    ) -> Self {
        let mut action = json!({"type":"conversations_select","block_id":key.block_id,
            "action_id":key.action_id,"selected_conversation":selected});
        if let Some(initial) = initial {
            action["initial_conversation"] = json!(initial);
        }
        if response_url_enabled {
            action["response_url_enabled"] = json!(true);
        }
        Self { view, key, action }
    }

    pub(super) fn channel(
        view: Value,
        key: InputKey,
        selected: Option<String>,
        initial: Option<String>,
        response_url: bool,
    ) -> Self {
        let mut action = json!({"type":"channels_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_channel":selected});
        if let Some(initial) = initial {
            action["initial_channel"] = json!(initial);
        }
        if response_url {
            action["response_url_enabled"] = json!(true);
        }
        Self { view, key, action }
    }

    pub(super) fn multi_user(
        view: Value,
        key: InputKey,
        selected: Vec<String>,
        initial: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"multi_users_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_users":selected});
        if let Some(initial) = initial {
            action["initial_users"] = initial;
        }
        Self { view, key, action }
    }

    pub(super) fn user(
        view: Value,
        key: InputKey,
        selected: Option<String>,
        initial: Option<String>,
    ) -> Self {
        let mut action = json!({"type":"users_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_user":selected});
        if let Some(initial) = initial {
            action["initial_user"] = json!(initial);
        }
        Self { view, key, action }
    }

    pub(super) fn datetime(
        view: Value,
        key: InputKey,
        selected: Option<i64>,
        initial: Option<i64>,
    ) -> Self {
        let mut action = json!({"type":"datetimepicker", "block_id":key.block_id,
            "action_id":key.action_id, "selected_date_time":selected});
        if let Some(initial) = initial {
            action["initial_date_time"] = json!(initial);
        }
        Self { view, key, action }
    }

    pub(super) fn time(
        view: Value,
        key: InputKey,
        selected: Option<String>,
        initial: Option<String>,
        timezone: Option<String>,
    ) -> Self {
        let mut action = json!({"type":"timepicker", "block_id":key.block_id,
            "action_id":key.action_id, "selected_time":selected});
        if let Some(initial) = initial {
            action["initial_time"] = json!(initial);
        }
        if let Some(timezone) = timezone {
            action["timezone"] = json!(timezone);
        }
        Self { view, key, action }
    }
    pub(super) fn date(
        view: Value,
        key: InputKey,
        selected: Option<String>,
        initial: Option<String>,
    ) -> Self {
        let mut action = json!({"type":"datepicker", "block_id":key.block_id,
            "action_id":key.action_id, "selected_date":selected});
        if let Some(initial) = initial {
            action["initial_date"] = json!(initial);
        }
        Self { view, key, action }
    }
    pub(super) fn radio(
        view: Value,
        key: InputKey,
        selected: Value,
        initial: Option<Value>,
    ) -> Self {
        let mut action = json!({"type":"radio_buttons", "block_id":key.block_id,
            "action_id":key.action_id, "selected_option":selected});
        if let Some(initial) = initial {
            action["initial_option"] = initial;
        }
        Self { view, key, action }
    }
    pub(super) fn checkbox(view: Value, key: InputKey, selected: Vec<Value>) -> Self {
        Self {
            view,
            action: json!({"type":"checkboxes", "block_id":key.block_id,
                "action_id":key.action_id, "selected_options":selected}),
            key,
        }
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
    let shape = match kind {
        InputKind::StaticSelect {
            multiple: false, ..
        } => {
            matches!(
                block["type"].as_str(),
                Some("input" | "actions" | "section")
            ) && element["type"] == "static_select"
        }
        InputKind::StaticSelect { multiple: true, .. } => {
            block["type"] == "input" && element["type"] == "multi_static_select"
        }
        _ => false,
    };
    shape
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|k| element.get(*k).is_none())
}

impl HomeForm {
    fn static_block(&self, key: &InputKey) -> Result<&Value, FormError> {
        self.view["blocks"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["block_id"] == key.block_id))
            .ok_or_else(invalid)
    }

    pub fn static_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        self.static_typed_field(key, false)
    }

    pub fn multi_static_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        self.static_typed_field(key, true)
    }

    fn static_typed_field(&self, key: &InputKey, multiple: bool) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.static_block(key)?;
        let source = match block["type"].as_str() {
            Some("input") => block.get("element"),
            Some("section") => block.get("accessory"),
            Some("actions") => block["elements"]
                .as_array()
                .and_then(|elements| elements.iter().find(|e| e["action_id"] == key.action_id)),
            _ => None,
        }
        .ok_or_else(invalid)?;
        let optional = block["type"] != "input"
            || block
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if !supported(block, &field.raw, &field.kind)
            || !matches!(&field.kind, InputKind::StaticSelect { multiple: actual, .. } if *actual == multiple)
            || &field.raw != source
            || field.optional != optional
            || field.raw["action_id"] != key.action_id
            || parse::input(&field.raw, true, None, None)?.0 != field.kind
        {
            return Err(invalid());
        }
        Ok(field)
    }

    /// Only optional input wrappers expose a clear affordance. Te7m's actions
    /// and section fields have no required-input validation, but cannot clear
    /// through that UI affordance.
    pub fn static_allows_clear(&self, key: &InputKey) -> Result<bool, FormError> {
        let field = self.static_field(key)?;
        Ok(self.static_block(key)?["type"] == "input" && field.optional)
    }

    /// A source-resolved selection (or explicit clear), not an arbitrary option
    /// object. Store the callback state before field validation. An invalid clear
    /// cannot erase an earlier queued valid occurrence in the renderer.
    pub fn static_input(
        &mut self,
        key: &InputKey,
        selected: Option<String>,
    ) -> Result<Option<StaticEvent>, FormError> {
        let field = self.static_field(key)?;
        // z3Rn.resolveOption only resolves truthy values. An option with an
        // empty value therefore emits the same null callback as a clear.
        let selected = selected.filter(|value| !value.is_empty());
        let InputKind::StaticSelect { options, .. } = &field.kind else {
            return Err(FormError::ValueTypeMismatch);
        };
        let option = selected
            .as_ref()
            .map(|value| {
                options
                    .iter()
                    .find(|o| &o.value == value)
                    .map(|o| o.raw.clone())
                    .ok_or_else(invalid)
            })
            .transpose()?;
        let raw = field.raw.clone();
        self.set(key, InputValue::Selected(selected.into_iter().collect()))?;
        if self.dispatch(key) == Some(Dispatch::Quiet) || self.fields[key].error.is_some() {
            return Ok(None);
        }
        let mut action = json!({"type":"static_select", "block_id":key.block_id,
            "action_id":key.action_id, "selected_option":option});
        // z3Rn retains these source props in the callback, independently of the
        // selected option. AM6q state deliberately contains only selected_option.
        for prop in ["initial_option", "placeholder"] {
            if let Some(value) = raw.get(prop).filter(|v| !v.is_null()) {
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
