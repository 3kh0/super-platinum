//! Component display belongs to one mount; container wire state remains shared.
//! gMsa and z3Rn read saved options in their constructors, not on later prop updates.
use std::sync::{Arc, Mutex, Weak};

use serde_json::Value;
use super_platinum_core::block_actions::form::{FormField, InputKey, InputKind};

use super::{ContainerKey, next_id};

#[derive(Clone)]
pub(crate) struct Mounted(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    unmounted: bool,
    binding: Option<Binding>,
    raw: Value,
    selected: Option<String>,
    selected_options: Vec<Value>,
    selected_users: Vec<String>,
    options_present: bool,
    revision: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct Binding {
    owner: ContainerKey,
    key: InputKey,
    radio: bool,
    date: bool,
    time: bool,
    user: bool,
    users: bool,
    channels: bool,
    conversations: bool,
    conversation: bool,
    channel: bool,
    multi: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct Action {
    mounted: Weak<Mutex<State>>,
    revision: u64,
}

impl Mounted {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(State::default())))
    }

    pub(crate) fn unmount(self) {
        self.0.lock().unwrap().unmounted = true;
    }

    pub(super) fn sync(
        &self,
        owner: ContainerKey,
        key: InputKey,
        field: &FormField,
        saved: Option<&Value>,
    ) -> Option<String> {
        let mut state = self.0.lock().unwrap();
        if state.unmounted {
            return None;
        }
        let binding = Binding {
            owner,
            key,
            radio: matches!(field.kind, InputKind::Radio { .. }),
            date: matches!(field.kind, InputKind::Date),
            time: matches!(field.kind, InputKind::Time { .. }),
            conversation: matches!(field.kind, InputKind::Conversation),
            channel: matches!(field.kind, InputKind::Channel),
            user: matches!(field.kind, InputKind::User),
            users: false,
            channels: false,
            conversations: false,
            multi: None,
        };
        if binding.channel || binding.conversation || binding.user || binding.date || binding.time {
            let initial_key = if binding.channel {
                "initial_channel"
            } else if binding.conversation {
                "initial_conversation"
            } else if binding.user {
                "initial_user"
            } else if binding.time {
                "initial_time"
            } else {
                "initial_date"
            };
            if state.binding.as_ref() != Some(&binding) {
                state.selected = saved
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        field
                            .raw
                            .get(initial_key)
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                    })
                    .map(str::to_owned);
                state.binding = Some(binding);
                state.revision = next_id();
            } else if state.raw.get(initial_key) != field.raw.get(initial_key) {
                state.selected = field
                    .raw
                    .get(initial_key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                state.revision = next_id();
            }
            state.raw = field.raw.clone();
            return state.selected.clone();
        }
        if state.binding.as_ref() != Some(&binding) {
            // Radio uses saved.value || initial.value; a static select keeps a
            // saved empty value. Null shared state is already mapped to None.
            state.selected = saved
                .and_then(value)
                .filter(|selected| !binding.radio || !selected.is_empty())
                .or_else(|| field.raw.get("initial_option").and_then(value))
                .map(str::to_owned);
            state.binding = Some(binding);
            state.raw = field.raw.clone();
            state.revision = next_id();
        } else if state.raw != field.raw {
            let before = state.selected.clone();
            if !binding.radio {
                // z3Rn runs both updates, in this order. gMsa has no update
                // reset, so a mounted radio retains even a now-absent value.
                if state.raw.get("initial_option") != field.raw.get("initial_option") {
                    state.selected = field
                        .raw
                        .get("initial_option")
                        .and_then(value)
                        .map(str::to_owned);
                }
                if state.raw.get("options") != field.raw.get("options")
                    || state.raw.get("option_groups") != field.raw.get("option_groups")
                {
                    if let Some(selected) = &state.selected {
                        let available = match &field.kind {
                            InputKind::StaticSelect { options, .. } => {
                                options.iter().any(|option| &option.value == selected)
                            }
                            _ => false,
                        };
                        if !available {
                            state.selected = None;
                        }
                    }
                }
            }
            if state.selected != before {
                state.revision = next_id();
            }
            state.raw = field.raw.clone();
        }
        state.selected.clone()
    }

    pub(super) fn sync_multi(
        &self,
        owner: ContainerKey,
        key: InputKey,
        field: &FormField,
        saved: Option<&[Value]>,
    ) -> Vec<Value> {
        let mut state = self.0.lock().unwrap();
        if state.unmounted {
            return vec![];
        }
        let checkbox = matches!(field.kind, InputKind::Checkboxes { .. });
        let binding = Binding {
            owner,
            key,
            radio: false,
            date: false,
            time: false,
            user: false,
            users: false,
            channels: false,
            conversations: false,
            conversation: false,
            channel: false,
            multi: Some(checkbox),
        };
        if state.binding.as_ref() != Some(&binding) {
            state.options_present = checkbox
                || saved.is_some()
                || field
                    .raw
                    .get("initial_options")
                    .is_some_and(Value::is_array);
            state.selected_options = saved
                .map(<[Value]>::to_vec)
                .or_else(|| {
                    field
                        .raw
                        .get("initial_options")
                        .and_then(Value::as_array)
                        .cloned()
                })
                .unwrap_or_default();
            state.binding = Some(binding);
            state.revision = next_id();
        } else if !checkbox && state.raw.get("initial_options") != field.raw.get("initial_options")
        {
            // etBz resets only for a truthy array prop, including [] but not null.
            if let Some(initial) = field.raw.get("initial_options").and_then(Value::as_array) {
                state.selected_options = initial.clone();
                state.options_present = true;
                state.revision = next_id();
            }
        }
        state.raw = field.raw.clone();
        state.selected_options.clone()
    }

    pub(super) fn sync_users(
        &self,
        owner: ContainerKey,
        key: InputKey,
        field: &FormField,
        saved: Option<Vec<String>>,
    ) -> Vec<String> {
        let channels = matches!(field.kind, InputKind::MultiChannel { .. });
        let conversations = matches!(field.kind, InputKind::MultiConversation { .. });
        let initial_key = if conversations {
            "initial_conversations"
        } else if channels {
            "initial_channels"
        } else {
            "initial_users"
        };
        let mut state = self.0.lock().unwrap();
        if state.unmounted {
            return vec![];
        }
        let binding = Binding {
            owner,
            key,
            radio: false,
            date: false,
            time: false,
            user: false,
            users: !channels && !conversations,
            channels,
            conversations,
            conversation: false,
            channel: false,
            multi: None,
        };
        let initial = || {
            field
                .raw
                .get(initial_key)
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        if state.binding.as_ref() != Some(&binding) {
            state.selected_users = saved.unwrap_or_else(initial);
            state.binding = Some(binding);
            state.revision = next_id();
        } else if state.raw.get(initial_key) != field.raw.get(initial_key)
            && (!conversations || field.raw.get(initial_key).is_some_and(Value::is_array))
        {
            state.selected_users = initial();
            state.revision = next_id();
        }
        state.raw = field.raw.clone();
        state.selected_users.clone()
    }

    pub(super) fn retained_multi(
        &self,
        owner: &ContainerKey,
        key: &InputKey,
    ) -> Option<Vec<Value>> {
        let state = self.0.lock().unwrap();
        (!state.unmounted
            && state
                .binding
                .as_ref()
                .is_some_and(|b| &b.owner == owner && &b.key == key && b.multi.is_some()))
        .then(|| state.selected_options.clone())
    }

    pub(super) fn options_present(&self, owner: &ContainerKey, key: &InputKey) -> Option<bool> {
        let state = self.0.lock().unwrap();
        (!state.unmounted
            && state
                .binding
                .as_ref()
                .is_some_and(|b| &b.owner == owner && &b.key == key && b.multi.is_some()))
        .then_some(state.options_present)
    }

    pub(super) fn capture(&self, owner: &ContainerKey, key: &InputKey) -> Option<Action> {
        let state = self.0.lock().unwrap();
        if state.unmounted
            || state
                .binding
                .as_ref()
                .is_none_or(|binding| &binding.owner != owner || &binding.key != key)
        {
            return None;
        }
        Some(Action {
            mounted: Arc::downgrade(&self.0),
            revision: state.revision,
        })
    }
}

impl Action {
    pub(crate) fn current(&self) -> bool {
        self.mounted.upgrade().is_some_and(|mounted| {
            let state = mounted.lock().unwrap();
            !state.unmounted && state.revision == self.revision
        })
    }

    #[cfg(test)]
    pub(crate) fn commit_multi(&self, selected: &[Value]) -> bool {
        self.commit_multi_optional(Some(selected))
    }
    pub(crate) fn commit_multi_optional(&self, selected: Option<&[Value]>) -> bool {
        let Some(mounted) = self.mounted.upgrade() else {
            return false;
        };
        let mut state = mounted.lock().unwrap();
        if state.unmounted || state.revision != self.revision {
            return false;
        }
        state.selected_options = selected.unwrap_or_default().to_vec();
        state.options_present = selected.is_some();
        state.revision = next_id();
        true
    }

    pub(crate) fn commit_users(&self, selected: &[String]) -> bool {
        let Some(mounted) = self.mounted.upgrade() else {
            return false;
        };
        let mut state = mounted.lock().unwrap();
        if state.unmounted || state.revision != self.revision {
            return false;
        }
        state.selected_users = selected.to_vec();
        state.revision = next_id();
        true
    }

    pub(crate) fn commit(&self, selected: Option<&str>) -> bool {
        let Some(mounted) = self.mounted.upgrade() else {
            return false;
        };
        let mut state = mounted.lock().unwrap();
        if state.unmounted || state.revision != self.revision {
            return false;
        }
        state.selected = selected.map(str::to_owned);
        state.revision = next_id();
        true
    }
}

fn value(option: &Value) -> Option<&str> {
    option.get("value").and_then(Value::as_str)
}

#[cfg(test)]
mod tests;
