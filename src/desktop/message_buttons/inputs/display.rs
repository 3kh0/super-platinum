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
    revision: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct Binding {
    owner: ContainerKey,
    key: InputKey,
    radio: bool,
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
        };
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
