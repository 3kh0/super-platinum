//! Typed, local modal input state. Wire keys follow captured cWek/cxJV/AM6q.
//! See Slack's input/plain-text/select element reference for constraints.
//! Parsing and editing never dispatch an action or authorize submission.
//! Empty/null state and non-input state inclusion are documented at
//! https://docs.slack.dev/changelog/2020/09/01/full-state-on-view-submisson-and-block-actions

use serde_json::{Value, json};
use std::collections::BTreeMap;

mod message_conversations;
pub use message_conversations::{parse_message_conversations, validate_message_conversations};
mod message_channels;
pub use message_channels::{parse_message_channels, validate_message_channels};
mod message_channel;
pub use message_channel::{parse_message_channel, validate_message_channel};
mod message_conversation;
mod message_date;
mod message_multi;
mod message_overflow;
mod message_time;
mod message_user;
mod message_users;
mod modal_channel;
pub use message_conversation::{
    ConversationFilter, parse_message_conversation, valid_message_conversation,
    validate_message_conversation,
};
pub use message_overflow::parse_message_overflow;
pub use message_user::{parse_message_user, valid_message_user, validate_message_user};
pub use message_users::{parse_message_users, validate_message_users};
mod parse;
mod values;
pub use message_date::{parse_message_date, validate_message_date};
pub use message_multi::parse_message_multi;
pub(crate) use message_multi::validate_optional_selection as validate_message_multi_optional;
pub use message_time::{parse_message_time, validate_message_time};
pub use values::{datetime_local_value, parse_datetime_local};

/// Parse a message-owned single static select without synthesizing a modal.
pub fn parse_message_static_select(element: &Value) -> Result<FormField, FormError> {
    parse::message_static_select(element)
}

/// Parse a message-owned radio group, retaining raw option composition objects.
pub fn parse_message_radio(element: &Value) -> Result<FormField, FormError> {
    parse::message_radio(element)
}

pub(crate) fn radio_options(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    parse::radio_options(element)
}

pub(crate) fn static_select_options(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    parse::static_select_options(element)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputKey {
    pub block_id: String,
    pub action_id: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FormBlock {
    Display(Value),
    Input(InputKey),
}
#[derive(Clone, Debug, PartialEq)]
pub struct OptionSpec {
    pub text: String,
    pub value: String,
    pub raw: Value,
    pub group: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum InputKind {
    Text {
        multiline: bool,
        min_length: Option<usize>,
        max_length: Option<usize>,
    },
    StaticSelect {
        options: Vec<OptionSpec>,
        multiple: bool,
        max_selected_items: Option<usize>,
    },
    Checkboxes {
        options: Vec<OptionSpec>,
    },
    Radio {
        options: Vec<OptionSpec>,
    },
    Date,
    User,
    MultiConversation {
        max_selected_items: Option<usize>,
    },
    MultiChannel {
        max_selected_items: Option<usize>,
    },
    MultiUser {
        max_selected_items: Option<usize>,
    },
    Conversation,
    Channel,
    Time {
        timezone: Option<String>,
    },
    DateTime,
    Unsupported {
        kind: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Selected(Vec<String>),
    Date(Option<String>),
    User(Option<String>),
    Users(Vec<String>),
    Channels(Vec<String>),
    Conversations(Vec<String>),
    Conversation(Option<String>),
    Channel(Option<String>),
    Time(Option<String>),
    DateTime(Option<i64>),
    Unsupported,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FormField {
    pub label: String,
    pub hint: Option<String>,
    pub placeholder: Option<String>,
    pub optional: bool,
    pub kind: InputKind,
    pub initial_value: InputValue,
    pub value: InputValue,
    pub dirty: bool,
    pub error: Option<String>,
    /// Increment only when a pristine value changes remotely. User typing does
    /// not remount an uncontrolled native field.
    pub revision: u64,
    pub raw: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ModalForm {
    pub view_id: String,
    pub title: String,
    pub submit_label: Option<String>,
    pub close_label: Option<String>,
    pub blocks: Vec<FormBlock>,
    pub fields: BTreeMap<InputKey, FormField>,
    /// Stateful controls outside supported input blocks cannot silently vanish.
    pub unsupported: Vec<String>,
    pub raw_view: Value,
    /// Frozen action origin; retained across server updates to this form.
    origin_conversation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FormError {
    #[error("This form is malformed: {0}")]
    InvalidView(String),
    #[error("This field no longer exists")]
    UnknownField,
    #[error("The input value does not match this field")]
    ValueTypeMismatch,
    #[error("Complete the highlighted fields")]
    Validation(Vec<(InputKey, String)>),
    #[error("This form includes unsupported controls")]
    Unsupported(Vec<String>),
    #[error("The form changed while you were editing; your writing has been retained")]
    ConflictingUpdate,
}

impl ModalForm {
    pub fn parse(view: &Value) -> Result<Self, FormError> {
        Self::parse_with_origin(view, None)
    }

    pub fn parse_with_origin(
        view: &Value,
        origin_conversation: Option<&str>,
    ) -> Result<Self, FormError> {
        parse::form(view, origin_conversation)
    }

    pub fn set_value(&mut self, key: &InputKey, value: InputValue) -> Result<(), FormError> {
        let field = self.fields.get_mut(key).ok_or(FormError::UnknownField)?;
        if !values::matches(&field.kind, &value) {
            return Err(FormError::ValueTypeMismatch);
        }
        field.value = value;
        field.dirty = true;
        field.error = values::error(field);
        Ok(())
    }

    pub fn validate(&self) -> Vec<(InputKey, String)> {
        self.fields
            .iter()
            .filter_map(|(key, field)| {
                values::error(field)
                    .or_else(|| field.error.clone())
                    .map(|error| (key.clone(), error))
            })
            .collect()
    }

    /// State sent as the `state` form argument to views.submit. Option objects
    /// retain the server's exact text/value metadata, not their display labels.
    pub fn submission(&self) -> Result<Value, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let errors = self.validate();
        if !errors.is_empty() {
            return Err(FormError::Validation(errors));
        }
        let mut blocks = serde_json::Map::new();
        for (key, field) in &self.fields {
            let block = blocks
                .entry(key.block_id.clone())
                .or_insert_with(|| json!({}));
            block
                .as_object_mut()
                .expect("new object")
                .insert(key.action_id.clone(), values::wire(field));
        }
        Ok(json!({"values": blocks}))
    }

    /// Transactional: identity/type removals cannot silently discard dirty
    /// text. The controller must block submission after a conflicting update.
    pub fn reconcile(&mut self, view: &Value) -> Result<(), FormError> {
        let mut next = Self::parse_with_origin(view, self.origin_conversation.as_deref())?;
        if next.view_id != self.view_id {
            return Err(FormError::ConflictingUpdate);
        }
        for (key, old) in &self.fields {
            let Some(new) = next.fields.get_mut(key) else {
                if old.dirty {
                    return Err(FormError::ConflictingUpdate);
                }
                continue;
            };
            if old.dirty {
                if values::wire_type(&old.kind) != values::wire_type(&new.kind) {
                    return Err(FormError::ConflictingUpdate);
                }
                new.value = old.value.clone();
                new.dirty = true;
                new.revision = old.revision;
                // Native parsing and server errors may refer to text that has
                // not become a typed value. Retain those until an explicit
                // edit; an unrelated remote update must not unblock an old
                // valid value while invalid text is still displayed.
                let external_error = (old.error != values::error(old))
                    .then(|| old.error.clone())
                    .flatten();
                new.error = external_error.or_else(|| values::error(new));
            } else {
                new.revision = old.revision.wrapping_add(u64::from(old.value != new.value));
            }
        }
        *self = next;
        Ok(())
    }

    pub fn apply_server_errors(&mut self, errors: &BTreeMap<String, String>) {
        for (key, field) in &mut self.fields {
            field.error = errors
                .get(&key.block_id)
                .cloned()
                .or_else(|| values::error(field));
        }
    }

    /// An acknowledged submission may replace the accepted step's fields.
    /// Captured AgTQ/4Z6N upsert the same view while 9tv2 prunes only missing
    /// block/action identities; retained controls keep their current state.
    /// Unlike an unsolicited update, removed or replaced dirty fields do not
    /// create a conflict after the app has accepted their submission.
    pub fn reconcile_submitted(&mut self, view: &Value) -> Result<(), FormError> {
        let mut next = Self::parse_with_origin(view, self.origin_conversation.as_deref())?;
        if next.view_id != self.view_id {
            return Err(FormError::ConflictingUpdate);
        }
        for (key, new) in &mut next.fields {
            let Some(old) = self.fields.get(key) else {
                continue;
            };
            if values::wire_type(&old.kind) == values::wire_type(&new.kind) {
                new.value = old.value.clone();
                new.dirty = old.dirty;
                new.revision = old.revision;
                new.error = values::error(new);
            } else {
                new.revision = old.revision.wrapping_add(1);
            }
        }
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod modal_channel_tests;
#[cfg(test)]
mod modal_channels_tests;
#[cfg(test)]
mod modal_conversation_tests;
#[cfg(test)]
mod modal_conversations_tests;
#[cfg(test)]
mod modal_user_tests;
#[cfg(test)]
mod modal_users_tests;
#[cfg(test)]
mod tests;
