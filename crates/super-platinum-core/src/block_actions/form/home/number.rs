//! Home dd8t numeric callbacks. JS normalization and local edit acceptance belong
//! to the renderer; this module validates the finite callback domain and custody.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberTrigger {
    Change,
    Enter,
    Blur,
}

/// Initialized field identity, never a captured parent or native buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberParent {
    owner: Value,
    key: InputKey,
}

// Decimal/exponent strings from a number control, not JavaScript source syntax.
// This does not recreate Number.toString or authorize method-only NaN/Infinity.
fn finite(value: &str) -> Option<f64> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return None;
    }
    value.parse::<f64>().ok().filter(|v| v.is_finite())
}
pub(super) fn valid_value(value: Option<&str>) -> bool {
    value.is_none_or(|v| v.is_empty() || finite(v).is_some())
}

/// Home-only parser. Generic Modal and message number admission stays unsupported.
pub(super) fn parse(element: &Value) -> Result<(InputKind, InputValue), FormError> {
    if element["type"] != "number_input" {
        return Err(invalid());
    }
    let decimal = element["is_decimal_allowed"]
        .as_bool()
        .ok_or_else(invalid)?;
    let string = |key: &str| -> Result<Option<String>, FormError> {
        element
            .get(key)
            .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
            .transpose()
    };
    let initial = string("initial_value")?;
    let min_value = string("min_value")?;
    let max_value = string("max_value")?;
    if !valid_value(initial.as_deref())
        || min_value.as_deref().is_some_and(|v| finite(v).is_none())
        || max_value.as_deref().is_some_and(|v| finite(v).is_none())
    {
        return Err(invalid());
    }
    Ok((
        InputKind::Number {
            is_decimal_allowed: decimal,
            min_value,
            max_value,
        },
        InputValue::Number(initial.filter(|v| !v.is_empty())),
    ))
}

pub(super) fn supported(block: &Value, element: &Value, kind: &InputKind) -> bool {
    block["type"] == "input"
        && element["type"] == "number_input"
        && matches!(kind, InputKind::Number { .. })
        && ["confirm", "visible_to_user_ids"]
            .iter()
            .all(|k| element.get(*k).is_none())
}
pub(super) fn error(field: &FormField) -> Option<String> {
    let (
        InputKind::Number {
            min_value,
            max_value,
            ..
        },
        InputValue::Number(value),
    ) = (&field.kind, &field.value)
    else {
        return Some("This value no longer matches the field.".into());
    };
    let Some(value) = value.as_deref().filter(|v| !v.is_empty()) else {
        return (!field.optional).then(|| "Complete this required field.".into());
    };
    let Some(number) = finite(value) else {
        return Some("Enter a valid number.".into());
    };
    // hvzX tests the converted bound's truthiness: either signed zero is skipped.
    if min_value
        .as_deref()
        .and_then(finite)
        .is_some_and(|min| min != 0.0 && number < min)
    {
        return Some(format!(
            "Enter a number at least {}.",
            min_value.as_deref().unwrap()
        ));
    }
    if max_value
        .as_deref()
        .and_then(finite)
        .is_some_and(|max| max != 0.0 && number > max)
    {
        return Some(format!(
            "Enter a number no greater than {}.",
            max_value.as_deref().unwrap()
        ));
    }
    None
}

impl HomeForm {
    pub fn number_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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
            || parse(&field.raw)?.0 != field.kind
            || self.dispatch(key) != Some(dispatch(block, &field.raw, &field.kind)?)
        {
            return Err(invalid());
        }
        Ok(field)
    }
    pub(super) fn initialize_number_wire(&mut self) {
        for (key, field) in &self.fields {
            if !matches!(field.kind, InputKind::Number { .. }) {
                continue;
            }
            let saved = self.view["state"]["values"]
                .get(&key.block_id)
                .and_then(|b| b.get(&key.action_id));
            // Preserve omission only when the saved row actually omitted it or
            // the new initializer had no truthy initial string.
            if saved.is_some_and(|v| v.get("value").is_none())
                || (saved.is_none()
                    && field.raw["initial_value"]
                        .as_str()
                        .is_none_or(str::is_empty))
            {
                self.wire
                    .insert(key.clone(), json!({"type":"number_input"}));
            }
        }
    }
    pub(super) fn number_state_valid(&self, key: &InputKey) -> bool {
        let Ok(field) = self.number_field(key) else {
            return false;
        };
        let Some(wire) = self.wire.get(key) else {
            return false;
        };
        let omitted = matches!(field.value, InputValue::Number(None))
            && wire == &json!({"type":"number_input"});
        valid_state_value(field)
            && (wire == &values::wire(field) || omitted)
            && (self.native_errors.contains_key(key) || field.error == error(field))
    }
    pub(super) fn number_state_inventory_valid(&self) -> bool {
        self.view["blocks"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .filter(|b| b["element"]["type"] == "number_input")
                .all(|b| {
                    self.number_state_valid(&InputKey {
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
            .filter(|(_, f)| matches!(f.kind, InputKind::Number { .. }))
            .all(|(key, _)| self.number_state_valid(key))
    }
    /// Fresh mapped savedValue || initialValue. Some("") and None stay distinct.
    /// Mounted widgets do not re-seed when source or parent props change.
    pub fn number_seed(&self, key: &InputKey) -> Result<Option<String>, FormError> {
        if !self.number_state_valid(key) || self.native_errors.contains_key(key) {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(self.wire[key]["value"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| self.fields[key].raw["initial_value"].as_str())
            .map(str::to_owned))
    }
    pub fn number_parent(&self, key: &InputKey) -> Result<NumberParent, FormError> {
        self.number_seed(key)?;
        Ok(NumberParent {
            owner: checkboxes::owner(&self.view),
            key: key.clone(),
        })
    }
    pub(super) fn number_parent_keys(
        &self,
        next: &Self,
        parents: &[NumberParent],
    ) -> Result<BTreeSet<InputKey>, FormError> {
        let mut keys = BTreeSet::new();
        for parent in parents {
            if parent.owner != checkboxes::owner(&self.view)
                || parent.owner != checkboxes::owner(&next.view)
                || !self.number_state_valid(&parent.key)
                || !next.number_state_valid(&parent.key)
                || !keys.insert(parent.key.clone())
            {
                return Err(FormError::ConflictingUpdate);
            }
        }
        Ok(keys)
    }
    pub(super) fn number_store(
        &mut self,
        key: &InputKey,
        value: Option<String>,
    ) -> Result<(), FormError> {
        if !self.number_state_valid(key) {
            return Err(FormError::ConflictingUpdate);
        }
        if !valid_value(value.as_deref()) {
            return Err(invalid());
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.value = InputValue::Number(value);
        field.dirty = true;
        field.error = error(field);
        self.wire.insert(key.clone(), values::wire(field));
        Ok(())
    }
    /// The owned renderer supplies the actual finite callback scalar: normalized
    /// JS Number(trim).toString for Change, local literal for Enter, trimmed-zero
    /// spelling for Blur. It separately proves event reachability/local equality.
    /// No callback is invented for missing-local blur or a rejected native buffer.
    pub fn number_input(
        &mut self,
        key: &InputKey,
        value: Option<String>,
        trigger: NumberTrigger,
    ) -> Result<Option<TextEvent>, FormError> {
        let field = self.number_field(key)?;
        let initial = field.raw["initial_value"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let mode = self.dispatch(key).ok_or_else(invalid)?;
        let value = value.filter(|v| !v.is_empty());
        self.number_store(key, value.clone())?;
        let emit = match trigger {
            NumberTrigger::Change => {
                value.is_some() && matches!(mode, Dispatch::Character | Dispatch::EnterOrCharacter)
            }
            NumberTrigger::Enter => matches!(mode, Dispatch::Enter | Dispatch::EnterOrCharacter),
            NumberTrigger::Blur => mode != Dispatch::Quiet,
        };
        if !emit || self.fields[key].error.is_some() {
            return Ok(None);
        }
        let mut action = self.wire[key].clone();
        action["block_id"] = json!(key.block_id);
        action["action_id"] = json!(key.action_id);
        if trigger != NumberTrigger::Enter {
            if let Some(initial) = initial {
                action["initial_value"] = json!(initial);
            }
        }
        Ok(Some(TextEvent::number(
            self.view.clone(),
            key.clone(),
            action,
        )))
    }
    /// Clear only a native safety marker after an owned, acknowledged finite
    /// buffer observation; do not commit that buffer or source display props.
    pub fn number_acknowledge_native_buffer(
        &mut self,
        key: &InputKey,
        value: Option<&str>,
    ) -> Result<(), FormError> {
        if !self.number_state_valid(key) || !valid_value(value) {
            return Err(FormError::ConflictingUpdate);
        }
        self.native_errors.remove(key);
        let field = self.fields.get_mut(key).unwrap();
        field.error = error(field);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
