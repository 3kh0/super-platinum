//! Message-owned selections and their separately retained container wire state.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{Message, MessageStateError, element, kind, stateful};
use crate::block_actions::form::{
    FormField, InputKey, InputKind, InputValue, parse_message_channel, parse_message_channels,
    parse_message_conversation, parse_message_conversations, parse_message_date,
    parse_message_multi, parse_message_radio, parse_message_static_select, parse_message_time,
    parse_message_user, parse_message_users, validate_message_channel,
    validate_message_conversation, validate_message_date, validate_message_time,
    validate_message_user,
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
                if !matches!(
                    control_kind,
                    "static_select"
                        | "radio_buttons"
                        | "checkboxes"
                        | "multi_static_select"
                        | "datepicker"
                        | "timepicker"
                        | "users_select"
                        | "multi_users_select"
                        | "multi_channels_select"
                        | "multi_conversations_select"
                        | "conversations_select"
                        | "channels_select"
                ) {
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
                    "datepicker" => parse_message_date(control),
                    "conversations_select" => parse_message_conversation(control),
                    "channels_select" => parse_message_channel(control),
                    "users_select" => parse_message_user(control),
                    "multi_users_select" => parse_message_users(control),
                    "multi_channels_select" => parse_message_channels(control),
                    "multi_conversations_select" => parse_message_conversations(control),
                    "timepicker" => parse_message_time(control),
                    "radio_buttons" => parse_message_radio(control),
                    "checkboxes" | "multi_static_select" => parse_message_multi(control),
                    _ => parse_message_static_select(control),
                }
                .map_err(|_| MessageStateError::MalformedStructure)?;
                let mut initial = json!({"type":control_kind});
                if control_kind == "timepicker" {
                    for (input, wire) in
                        [("initial_time", "selected_time"), ("timezone", "timezone")]
                    {
                        if let Some(value) = control
                            .get(input)
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                        {
                            initial[wire] = json!(value);
                        }
                    }
                    if section || initial.as_object().expect("type object").len() > 1 {
                        result.initial.insert(key.clone(), initial);
                    }
                    result.fields.insert(key, field);
                    continue;
                }
                let multiple = matches!(control_kind, "checkboxes" | "multi_static_select");
                if let Some(option) = control
                    .get(if control_kind == "channels_select" {
                        "initial_channel"
                    } else if control_kind == "conversations_select" {
                        "initial_conversation"
                    } else if control_kind == "multi_conversations_select" {
                        "initial_conversations"
                    } else if control_kind == "multi_channels_select" {
                        "initial_channels"
                    } else if control_kind == "multi_users_select" {
                        "initial_users"
                    } else if control_kind == "users_select" {
                        "initial_user"
                    } else if control_kind == "datepicker" {
                        "initial_date"
                    } else if multiple {
                        "initial_options"
                    } else {
                        "initial_option"
                    })
                    .filter(|value| {
                        !value.is_null()
                            && (!matches!(
                                control_kind,
                                "datepicker"
                                    | "users_select"
                                    | "conversations_select"
                                    | "channels_select"
                            ) || value.as_str().is_some_and(|value| !value.is_empty()))
                    })
                {
                    initial[if control_kind == "channels_select" {
                        "selected_channel"
                    } else if control_kind == "conversations_select" {
                        "selected_conversation"
                    } else if control_kind == "multi_conversations_select" {
                        "selected_conversations"
                    } else if control_kind == "multi_channels_select" {
                        "selected_channels"
                    } else if control_kind == "multi_users_select" {
                        "selected_users"
                    } else if control_kind == "users_select" {
                        "selected_user"
                    } else if control_kind == "datepicker" {
                        "selected_date"
                    } else if multiple {
                        "selected_options"
                    } else {
                        "selected_option"
                    }] = option.clone();
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
        if matches!(field.kind, InputKind::Channel) {
            let wire = validate_message_channel(field, selected)
                .map_err(|_| MessageStateError::InvalidSelection)?;
            self.initialize();
            let field = self.fields.get_mut(key).expect("checked field");
            field.value = InputValue::Channel(selected.map(str::to_owned));
            field.dirty = true;
            field.error = None;
            self.committed
                .as_mut()
                .expect("initialized")
                .insert(key.clone(), wire.clone());
            return Ok(wire);
        }
        if matches!(field.kind, InputKind::Conversation) {
            let wire = validate_message_conversation(field, selected)
                .map_err(|_| MessageStateError::InvalidSelection)?;
            self.initialize();
            let field = self.fields.get_mut(key).expect("checked field");
            field.value = InputValue::Conversation(selected.map(str::to_owned));
            field.dirty = true;
            field.error = None;
            self.committed
                .as_mut()
                .expect("initialized")
                .insert(key.clone(), wire.clone());
            return Ok(wire);
        }
        if matches!(field.kind, InputKind::User) {
            let wire = validate_message_user(field, selected)
                .map_err(|_| MessageStateError::InvalidSelection)?;
            self.initialize();
            let field = self.fields.get_mut(key).expect("checked field");
            field.value = InputValue::User(selected.map(str::to_owned));
            field.dirty = true;
            field.error = None;
            self.committed
                .as_mut()
                .expect("initialized")
                .insert(key.clone(), wire.clone());
            return Ok(wire);
        }
        if matches!(field.kind, InputKind::Date) {
            let wire = validate_message_date(field, selected)
                .map_err(|_| MessageStateError::InvalidSelection)?;
            self.initialize();
            let field = self.fields.get_mut(key).expect("checked field");
            field.value = InputValue::Date(selected.map(str::to_owned));
            field.dirty = true;
            field.error = None;
            self.committed
                .as_mut()
                .expect("initialized")
                .insert(key.clone(), wire.clone());
            return Ok(wire);
        }
        if matches!(field.kind, InputKind::Time { .. }) {
            let wire = validate_message_time(field, selected)
                .map_err(|_| MessageStateError::InvalidSelection)?;
            self.initialize();
            let field = self.fields.get_mut(key).expect("checked field");
            field.value = InputValue::Time(selected.map(str::to_owned));
            field.dirty = true;
            field.error = None;
            self.committed
                .as_mut()
                .expect("initialized")
                .insert(key.clone(), wire.clone());
            return Ok(wire);
        }
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

    /// Commit the complete raw selection array after confirmation. Retained
    /// options must come from trusted mounted display state, not DOM metadata.
    pub fn commit_multi(
        &mut self,
        key: &InputKey,
        selected: &[Value],
        retained: &[Value],
    ) -> Result<Value, MessageStateError> {
        self.commit_multi_optional(key, Some(selected), retained)
    }

    pub fn commit_multi_optional(
        &mut self,
        key: &InputKey,
        selected: Option<&[Value]>,
        retained: &[Value],
    ) -> Result<Value, MessageStateError> {
        let field = self
            .fields
            .get(key)
            .ok_or(MessageStateError::UnknownField)?;
        crate::block_actions::form::validate_message_multi_optional(field, selected, retained)
            .map_err(|_| MessageStateError::InvalidSelection)?;
        let wire_type = match field.kind {
            InputKind::Checkboxes { .. } => "checkboxes",
            _ => "multi_static_select",
        };
        let mut wire = json!({"type":wire_type});
        if let Some(selected) = selected {
            wire["selected_options"] = json!(selected);
        }
        self.initialize();
        let field = self.fields.get_mut(key).expect("checked field");
        field.value = InputValue::Selected(
            selected
                .unwrap_or_default()
                .iter()
                .map(|option| option["value"].as_str().expect("validated").to_owned())
                .collect(),
        );
        field.dirty = true;
        field.error = None;
        self.committed
            .as_mut()
            .expect("initialized")
            .insert(key.clone(), wire.clone());
        Ok(wire)
    }

    pub fn saved_options(&self, key: &InputKey) -> Option<&[Value]> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_options")?
            .as_array()
            .map(Vec::as_slice)
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

    pub fn saved_channel(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_channel")
            .filter(|value| value.is_string())
    }

    pub fn saved_conversation(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_conversation")
            .filter(|value| value.is_string())
    }

    pub fn saved_user(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_user")
            .filter(|value| value.is_string())
    }

    pub fn saved_date(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_date")
            .filter(|value| value.is_string())
    }

    pub fn saved_time(&self, key: &InputKey) -> Option<&Value> {
        self.committed
            .as_ref()?
            .get(key)?
            .get("selected_time")
            .filter(|value| value.is_string())
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
            if matches!(
                field.kind,
                InputKind::Channel
                    | InputKind::Conversation
                    | InputKind::User
                    | InputKind::MultiUser { .. }
                    | InputKind::MultiChannel { .. }
                    | InputKind::MultiConversation { .. }
                    | InputKind::Date
                    | InputKind::Time { .. }
            ) || matches!(
                old.kind,
                InputKind::Channel
                    | InputKind::Conversation
                    | InputKind::User
                    | InputKind::MultiUser { .. }
                    | InputKind::MultiChannel { .. }
                    | InputKind::MultiConversation { .. }
                    | InputKind::Date
                    | InputKind::Time { .. }
            ) {
                let initial_key = if matches!(field.kind, InputKind::Channel) {
                    "initial_channel"
                } else if matches!(field.kind, InputKind::Conversation) {
                    "initial_conversation"
                } else if matches!(field.kind, InputKind::MultiConversation { .. }) {
                    "initial_conversations"
                } else if matches!(field.kind, InputKind::MultiChannel { .. }) {
                    "initial_channels"
                } else if matches!(field.kind, InputKind::MultiUser { .. }) {
                    "initial_users"
                } else if matches!(field.kind, InputKind::User) {
                    "initial_user"
                } else if matches!(field.kind, InputKind::Time { .. }) {
                    "initial_time"
                } else {
                    "initial_date"
                };
                if matches!(
                    (&old.kind, &field.kind),
                    (InputKind::Channel, InputKind::Channel)
                        | (InputKind::Conversation, InputKind::Conversation)
                        | (InputKind::User, InputKind::User)
                        | (InputKind::MultiUser { .. }, InputKind::MultiUser { .. })
                        | (
                            InputKind::MultiChannel { .. },
                            InputKind::MultiChannel { .. }
                        )
                        | (
                            InputKind::MultiConversation { .. },
                            InputKind::MultiConversation { .. }
                        )
                        | (InputKind::Date, InputKind::Date)
                        | (InputKind::Time { .. }, InputKind::Time { .. })
                ) {
                    // QjSI only resets mounted display for a changed, present
                    // array (including []). Removing/nulling initial is not reset.
                    if old.raw.get(initial_key) == field.raw.get(initial_key)
                        || (matches!(field.kind, InputKind::MultiConversation { .. })
                            && field.raw.get(initial_key).is_none_or(Value::is_null))
                    {
                        field.value = old.value.clone();
                    } else {
                        field.revision = old.revision.wrapping_add(1);
                    }
                } else {
                    // A different component is mounted; it consumes only its
                    // own saved wire property, never another control's value.
                    if matches!(field.kind, InputKind::Channel) {
                        if let Some(saved) = self.saved_channel(key).and_then(Value::as_str) {
                            field.value = InputValue::Channel(Some(saved.into()));
                        }
                    } else if matches!(field.kind, InputKind::Conversation) {
                        if let Some(saved) = self.saved_conversation(key).and_then(Value::as_str) {
                            field.value = InputValue::Conversation(Some(saved.into()));
                        }
                    } else if matches!(field.kind, InputKind::MultiConversation { .. }) {
                        if let Some(saved) = self.saved_conversations(key) {
                            field.value = InputValue::Conversations(saved);
                        }
                    } else if matches!(field.kind, InputKind::MultiChannel { .. }) {
                        if let Some(saved) = self.saved_channels(key) {
                            field.value = InputValue::Channels(saved);
                        }
                    } else if matches!(field.kind, InputKind::MultiUser { .. }) {
                        if let Some(saved) = self.saved_users(key) {
                            field.value = InputValue::Users(saved);
                        }
                    } else if matches!(field.kind, InputKind::User) {
                        if let Some(saved) = self.saved_user(key).and_then(Value::as_str) {
                            field.value = InputValue::User(Some(saved.into()));
                        }
                    } else if matches!(field.kind, InputKind::Date) {
                        if let Some(saved) = self
                            .saved_date(key)
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            field.value = InputValue::Date(Some(saved.into()));
                        }
                    } else if matches!(field.kind, InputKind::Time { .. }) {
                        if let Some(saved) = self
                            .saved_time(key)
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            field.value = InputValue::Time(Some(saved.into()));
                        }
                    } else if matches!(
                        field.kind,
                        InputKind::StaticSelect { multiple: true, .. }
                            | InputKind::Checkboxes { .. }
                    ) {
                        if let Some(saved) = self.saved_options(key) {
                            field.value = InputValue::Selected(
                                saved
                                    .iter()
                                    .filter_map(|o| {
                                        o.get("value").and_then(Value::as_str).map(str::to_owned)
                                    })
                                    .collect(),
                            );
                        }
                    } else if let Some(saved) = self
                        .saved_option(key)
                        .and_then(|o| o.get("value"))
                        .and_then(Value::as_str)
                        .filter(|v| !matches!(field.kind, InputKind::Radio { .. }) || !v.is_empty())
                    {
                        field.value = InputValue::Selected(vec![saved.into()]);
                    }
                    field.revision = old.revision.wrapping_add(1);
                }
                continue;
            }
            let was_multi = matches!(
                &old.kind,
                InputKind::Checkboxes { .. } | InputKind::StaticSelect { multiple: true, .. }
            );
            let is_multi = matches!(
                &field.kind,
                InputKind::Checkboxes { .. } | InputKind::StaticSelect { multiple: true, .. }
            );
            if was_multi || is_multi {
                let same_kind = old.raw.get("type") == field.raw.get("type");
                if !same_kind {
                    let saved = if is_multi {
                        self.saved_options(key).map(|options| {
                            options
                                .iter()
                                .filter_map(|option| {
                                    option
                                        .get("value")
                                        .and_then(Value::as_str)
                                        .map(str::to_owned)
                                })
                                .collect()
                        })
                    } else {
                        self.saved_option(key)
                            .and_then(|option| option.get("value"))
                            .and_then(Value::as_str)
                            .filter(|value| {
                                !matches!(&field.kind, InputKind::Radio { .. }) || !value.is_empty()
                            })
                            .map(|value| vec![value.to_owned()])
                    };
                    if let Some(saved) = saved {
                        field.value = InputValue::Selected(saved);
                    }
                    field.revision = old.revision.wrapping_add(1);
                } else if matches!(&field.kind, InputKind::Checkboxes { .. }) {
                    // s+nw has no prop-update reset at all.
                    field.value = old.value.clone();
                } else if field
                    .raw
                    .get("initial_options")
                    .is_some_and(|value| !value.is_null())
                    && old.raw.get("initial_options") != field.raw.get("initial_options")
                {
                    // etBz resets only for a changed truthy initialOptions;
                    // JavaScript empty arrays are truthy. Wire state stays put.
                    field.revision = old.revision.wrapping_add(1);
                } else {
                    field.value = old.value.clone();
                }
                continue;
            }
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
                && !(direct
                    && matches!(
                        kind,
                        "static_select"
                            | "radio_buttons"
                            | "checkboxes"
                            | "multi_static_select"
                            | "datepicker"
                            | "timepicker"
                            | "users_select"
                            | "multi_users_select"
                            | "multi_channels_select"
                            | "multi_conversations_select"
                            | "conversations_select"
                            | "channels_select"
                    ))
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

#[cfg(test)]
mod multi_tests;

#[cfg(test)]
mod date_tests;

#[cfg(test)]
mod time_tests;

#[cfg(test)]
mod user_tests;

#[cfg(test)]
mod conversation_tests;

mod users;

mod channels;

mod conversations;
