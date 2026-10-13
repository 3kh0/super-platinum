//! Message-owned selections and their separately retained container wire state.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{Message, MessageStateError, element, kind, stateful};
use crate::block_actions::form::{
    FormField, InputKey, InputKind, InputValue, parse_message_radio, parse_message_static_select,
};

#[derive(Clone, Debug, PartialEq)]
pub struct MessageInputs {
    pub fields: BTreeMap<InputKey, FormField>,
    initial: BTreeMap<InputKey, Value>,
    committed: Option<BTreeMap<InputKey, Value>>,
}

impl MessageInputs {
    pub fn parse(message: &Message) -> Result<Self, MessageStateError> {
        if message.blocks.len() > 100 {
            return Err(MessageStateError::ExcessiveStructure);
        }
        let mut result = Self {
            fields: BTreeMap::new(),
            initial: BTreeMap::new(),
            committed: None,
        };
        let mut block_ids = BTreeSet::new();
        let mut remaining = 10_000;
        for block in &message.blocks {
            let block_id = block
                .get("block_id")
                .map(|value| identity(Some(value)))
                .transpose()?;
            if block_id
                .as_ref()
                .is_some_and(|id| !block_ids.insert(id.clone()))
            {
                return Err(MessageStateError::MalformedStructure);
            }
            let (elements, section): (Vec<&Value>, bool) = match kind(block)? {
                "section" => (block.get("accessory").into_iter().collect(), true),
                "actions" => (
                    block
                        .get("elements")
                        .and_then(Value::as_array)
                        .ok_or(MessageStateError::MalformedStructure)?
                        .iter()
                        .collect(),
                    false,
                ),
                "input" => return Err(MessageStateError::StatefulControl),
                "header" | "context" | "divider" | "image" | "rich_text" | "video" | "file"
                | "markdown" | "table" => (vec![], false),
                _ => return Err(MessageStateError::UnsupportedStructure),
            };
            scan_owned(block, 0, &mut remaining, &elements)?;
            let mut action_ids = BTreeSet::new();
            for control in &elements {
                let action_id = control
                    .get("action_id")
                    .map(|value| identity(Some(value)))
                    .transpose()?;
                if action_id
                    .as_ref()
                    .is_some_and(|id| !action_ids.insert(id.clone()))
                {
                    return Err(MessageStateError::MalformedStructure);
                }
                let control_kind = kind(control)?;
                if !matches!(control_kind, "static_select" | "radio_buttons") {
                    element(control)?;
                    continue;
                }
                let key = InputKey {
                    block_id: block_id
                        .clone()
                        .ok_or(MessageStateError::MalformedStructure)?,
                    action_id: action_id.ok_or(MessageStateError::MalformedStructure)?,
                };
                let field = match control_kind {
                    "radio_buttons" => parse_message_radio(control),
                    _ => parse_message_static_select(control),
                }
                .map_err(|_| MessageStateError::MalformedStructure)?;
                let mut initial = json!({"type":control_kind});
                if let Some(option) = control
                    .get("initial_option")
                    .filter(|value| !value.is_null())
                {
                    initial["selected_option"] = option.clone();
                    result.initial.insert(key.clone(), initial);
                } else if section {
                    // Captured M0Sk keeps a section's type-only entry, whereas
                    // actions omits elements whose cxJV initial value is absent.
                    result.initial.insert(key.clone(), initial);
                }
                result.fields.insert(key, field);
            }
        }
        Ok(result)
    }

    /// The first interaction, including an ordinary button, seeds the entire
    /// container. Rendering alone must leave state absent from blocks.actions.
    pub fn initialize(&mut self) {
        if self.committed.is_none() {
            self.committed = Some(self.initial.clone());
        }
    }

    /// Apply a new choice using the current canonical raw option. None is an
    /// explicit clear; Some("") remains a real empty-valued option.
    pub fn commit(
        &mut self,
        key: &InputKey,
        selected: Option<&str>,
    ) -> Result<Value, MessageStateError> {
        let field = self
            .fields
            .get(key)
            .ok_or(MessageStateError::UnknownField)?;
        let options = options(field)?;
        let selected_option = match selected {
            Some(value) => options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.raw.clone())
                .ok_or(MessageStateError::InvalidSelection)?,
            None => Value::Null,
        };
        let wire_type = match &field.kind {
            InputKind::Radio { .. } => "radio_buttons",
            _ => "static_select",
        };
        let wire = json!({"type":wire_type, "selected_option":selected_option});
        self.initialize();
        let field = self.fields.get_mut(key).expect("checked field");
        field.value = InputValue::Selected(selected.into_iter().map(str::to_owned).collect());
        field.dirty = true;
        field.error = None;
        self.committed
            .as_mut()
            .expect("initialized state")
            .insert(key.clone(), wire.clone());
        Ok(wire)
    }

    /// Raw values map; the API layer adds the single outer `values` envelope.
    /// Captured 9tv2 retains old committed option objects until another explicit
    /// interaction, even when the component displays a reset after an update.
    pub fn values(&self) -> Result<Option<Value>, MessageStateError> {
        let Some(committed) = &self.committed else {
            return Ok(None);
        };
        let mut values = serde_json::Map::new();
        for (key, value) in committed {
            values
                .entry(key.block_id.clone())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("created block object")
                .insert(key.action_id.clone(), value.clone());
        }
        Ok(Some(Value::Object(values)))
    }

    /// Captured iGIl supplies a saved option only when the retained value is an
    /// object. A mounted component consumes this on construction, not on every
    /// shared-state update. Null and missing selections both become undefined.
    pub fn saved_option(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_option")
            .filter(|value| value.is_object())
    }

    /// Parse transactionally, prune only removed identities from stored state,
    /// and reconcile component display independently from committed wire state.
    pub fn reconcile(&mut self, message: &Message) -> Result<(), MessageStateError> {
        let mut next = Self::parse(message)?;
        for (key, field) in &mut next.fields {
            let Some(old) = self.fields.get(key) else {
                continue;
            };
            field.dirty = old.dirty;
            field.revision = old.revision;
            let was_radio = matches!(&old.kind, InputKind::Radio { .. });
            let is_radio = matches!(&field.kind, InputKind::Radio { .. });
            if was_radio != is_radio {
                // A different component mounts. Captured iGIl supplies the
                // retained selected_option by identity, even if its wire type
                // differs. Null/missing state becomes undefined. gMsa then uses
                // saved.value || initial.value; z3Rn preserves an empty value.
                let saved = self
                    .committed
                    .as_ref()
                    .and_then(|values| values.get(key))
                    .and_then(|state| state.get("selected_option"))
                    .and_then(|option| option.get("value"))
                    .and_then(Value::as_str)
                    .filter(|value| !is_radio || !value.is_empty());
                if let Some(value) = saved {
                    field.value = InputValue::Selected(vec![value.to_owned()]);
                }
                field.revision = old.revision.wrapping_add(1);
                continue;
            }
            // Captured gMsa has no update reset: a mounted radio keeps its
            // saved value even when its initial option or available options change.
            if matches!(
                (&old.kind, &field.kind),
                (InputKind::Radio { .. }, InputKind::Radio { .. })
            ) {
                field.value = old.value.clone();
                continue;
            }
            let initial_changed = old.raw.get("initial_option") != field.raw.get("initial_option");
            if initial_changed {
                field.revision = field.revision.wrapping_add(1);
            } else {
                field.value = old.value.clone();
                let choices_changed = old.raw.get("options") != field.raw.get("options")
                    || old.raw.get("option_groups") != field.raw.get("option_groups");
                let InputValue::Selected(selected) = &field.value else {
                    return Err(MessageStateError::MalformedStructure);
                };
                if choices_changed
                    && selected.iter().any(|value| {
                        !options(field).is_ok_and(|options| {
                            options.iter().any(|option| &option.value == value)
                        })
                    })
                {
                    field.value = InputValue::Selected(vec![]);
                    field.revision = field.revision.wrapping_add(1);
                }
            }
        }
        next.committed = self.committed.as_ref().map(|committed| {
            committed
                .iter()
                .filter(|(key, _)| next.fields.contains_key(*key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        });
        *self = next;
        Ok(())
    }
}

fn options(
    field: &FormField,
) -> Result<&[crate::block_actions::form::OptionSpec], MessageStateError> {
    match &field.kind {
        InputKind::StaticSelect {
            options,
            multiple: false,
            ..
        }
        | InputKind::Radio { options } => Ok(options),
        _ => Err(MessageStateError::MalformedStructure),
    }
}

fn identity(value: Option<&Value>) -> Result<String, MessageStateError> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty() && id.len() <= 255)
        .map(str::to_owned)
        .ok_or(MessageStateError::MalformedStructure)
}

fn scan_owned(
    value: &Value,
    depth: usize,
    remaining: &mut usize,
    elements: &[&Value],
) -> Result<(), MessageStateError> {
    if depth > 32 || *remaining == 0 {
        return Err(MessageStateError::ExcessiveStructure);
    }
    *remaining -= 1;
    match value {
        Value::Object(object) => {
            let direct = elements.iter().any(|element| std::ptr::eq(*element, value));
            if let Some(kind) = object.get("type").and_then(Value::as_str)
                && (stateful(kind) || kind.ends_with("_select") || kind.ends_with("_input"))
                && !(direct && matches!(kind, "static_select" | "radio_buttons"))
            {
                return Err(MessageStateError::StatefulControl);
            }
            if object.contains_key("action_id") && !direct {
                return Err(MessageStateError::UnsupportedStructure);
            }
            for child in object.values() {
                scan_owned(child, depth + 1, remaining, elements)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                scan_owned(child, depth + 1, remaining, elements)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
