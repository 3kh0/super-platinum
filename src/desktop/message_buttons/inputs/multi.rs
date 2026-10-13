//! Drafts belong to one visible mount. Only Go/Apply publishes display and wire state.
use super::*;
use serde_json::Value;
use super_platinum_core::block_actions::{BlockConfirmation, BlockConfirmationState};

pub(crate) use super::presentation;

pub(crate) struct Picker {
    source: Source,
    metadata: BlockElementMetadata,
    display: display::Action,
    retained: Vec<Value>,
    selected: Vec<Value>,
    selected_present: bool,
    input_revision: u64,
    revision: u64,
}

pub(crate) struct PickerPresentation {
    pub revision: u64,
    pub options: Vec<OptionSpec>,
    pub selected: Vec<String>,
    pub selected_options: Vec<Value>,
    pub enabled: bool,
    pub reason: Option<String>,
    pub placeholder: String,
    pub max_selected: Option<usize>,
    pub confirmation: Option<BlockConfirmation>,
}

pub(crate) fn display_selected(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> Vec<Value> {
    let Some(entry) = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .filter(|entry| entry.source.same_action_revision(source) && source.current(shell))
    else {
        return vec![];
    };
    let Some(key) = field_key(metadata) else {
        return vec![];
    };
    let Some(field) = entry.model.fields.get(&key) else {
        return vec![];
    };
    mounted.sync_multi(
        ContainerKey::new(source),
        key.clone(),
        field,
        entry.model.saved_options(&key),
    )
}

fn capture(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    selected: &[Value],
    retained: &[Value],
    display: display::Action,
) -> Option<Captured> {
    capture_optional(shell, source, metadata, Some(selected), retained, display)
}
fn capture_optional(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    selected: Option<&[Value]>,
    retained: &[Value],
    display: display::Action,
) -> Option<Captured> {
    if !display.current() || !presentation(shell, source, metadata).enabled {
        return None;
    }
    let key = field_key(metadata)?;
    let mut captured = source
        .capture_multi_optional(shell, &key.block_id, &key.action_id, selected, retained)
        .ok()?;
    if captured.metadata() != metadata {
        return None;
    }
    captured.set_display(display);
    Some(captured)
}

pub(crate) fn change(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    selected: Vec<Value>,
    mounted: display::Mounted,
) {
    ensure(state, source.clone());
    let captured = {
        let mut shell = state.write();
        if shell.overlay.is_some() {
            return;
        }
        let Some(key) = field_key(&metadata) else {
            return;
        };
        let owner = ContainerKey::new(&source);
        let Some(retained) = mounted.retained_multi(&owner, &key) else {
            return;
        };
        let Some(display) = mounted.capture(&owner, &key) else {
            return;
        };
        let Some(captured) = capture(&shell, &source, &metadata, &selected, &retained, display)
        else {
            invalidate(&mut shell, &source);
            return;
        };
        if captured.confirmation().is_some() {
            let input_revision = revision(&shell, &source);
            shell.message_buttons.confirmation = Some(Pending {
                captured,
                revision: next_id(),
                input_revision,
            });
            shell.overlay = Some(Overlay::ButtonConfirmation);
            return;
        }
        captured
    };
    super::super::dispatch(state, captured);
}

pub(crate) fn open(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) {
    ensure(state, source.clone());
    let mut shell = state.write();
    if shell.overlay.is_some() || !presentation(&shell, &source, &metadata).enabled {
        return;
    }
    let Some(key) = field_key(&metadata) else {
        return;
    };
    let owner = ContainerKey::new(&source);
    let Some(selected) = mounted.retained_multi(&owner, &key) else {
        return;
    };
    let Some(selected_present) = mounted.options_present(&owner, &key) else {
        return;
    };
    let Some(display) = mounted.capture(&owner, &key) else {
        return;
    };
    let Some(input_revision) = revision(&shell, &source) else {
        return;
    };
    shell.message_buttons.multi_picker = Some(Picker {
        source,
        metadata,
        display,
        retained: selected.clone(),
        selected,
        selected_present,
        input_revision,
        revision: next_id(),
    });
    shell.overlay = Some(Overlay::MessageMultiSelect);
}

fn current(shell: &ShellState, picker: &Picker) -> bool {
    shell.overlay == Some(Overlay::MessageMultiSelect)
        && picker.display.current()
        && picker.source.current(shell)
        && revision(shell, &picker.source) == Some(picker.input_revision)
        && presentation(shell, &picker.source, &picker.metadata).enabled
}

pub(crate) fn picker(shell: &ShellState) -> Option<PickerPresentation> {
    let picker = shell.message_buttons.multi_picker.as_ref()?;
    let view = presentation(shell, &picker.source, &picker.metadata);
    let over = view
        .max_selected
        .is_some_and(|max| picker.selected.len() > max);
    let live = current(shell, picker);
    Some(PickerPresentation {
        revision: picker.revision,
        options: view.options,
        selected: picker
            .selected
            .iter()
            .filter_map(|v| v.get("value").and_then(Value::as_str).map(str::to_owned))
            .collect(),
        selected_options: picker.selected.clone(),
        enabled: live && !over,
        reason: if !live {
            Some(UNAVAILABLE.into())
        } else if over {
            Some(format!(
                "Choose at most {} options",
                view.max_selected.unwrap()
            ))
        } else {
            None
        },
        placeholder: view.placeholder,
        max_selected: view.max_selected,
        confirmation: match &picker.metadata.confirm {
            BlockConfirmationState::Parsed(confirm) => Some(confirm.clone()),
            _ => None,
        },
    })
}

pub(crate) fn toggle(mut state: Signal<ShellState>, expected: u64, value: String) {
    toggle_local(&mut state.write(), expected, value);
}
fn toggle_local(shell: &mut ShellState, expected: u64, value: String) {
    let Some(picker) = shell.message_buttons.multi_picker.as_ref() else {
        return;
    };
    if picker.revision != expected || !current(&shell, picker) {
        return;
    }
    let option = presentation(&shell, &picker.source, &picker.metadata)
        .options
        .into_iter()
        .find(|o| o.value == value);
    let picker = shell.message_buttons.multi_picker.as_mut().unwrap();
    if let Some(index) = picker
        .selected
        .iter()
        .position(|o| o.get("value").and_then(Value::as_str) == Some(value.as_str()))
    {
        picker.selected.remove(index);
        picker.selected_present = true;
    } else if let Some(option) = option {
        picker.selected.push(option.raw);
        picker.selected_present = true;
    }
}

pub(crate) fn apply(mut state: Signal<ShellState>, expected: u64) {
    let captured = {
        let mut shell = state.write();
        let Some(view) = picker(&shell) else {
            return;
        };
        if view.revision != expected || !view.enabled {
            return;
        }
        let Some(picker) = shell.message_buttons.multi_picker.take() else {
            return;
        };
        let captured = capture_optional(
            &shell,
            &picker.source,
            &picker.metadata,
            picker
                .selected_present
                .then_some(picker.selected.as_slice()),
            &picker.retained,
            picker.display,
        );
        shell.overlay = None;
        captured
    };
    // A multiselect's confirmation is integrated into this picker, unlike a
    // checkbox's separate onGo confirmation. Never show a second confirmation.
    if let Some(captured) = captured {
        super::super::dispatch(state, captured);
    }
}

pub(crate) fn cancel(mut state: Signal<ShellState>, expected: u64) {
    let mut shell = state.write();
    if shell
        .message_buttons
        .multi_picker
        .as_ref()
        .is_some_and(|p| p.revision == expected)
    {
        cancel_local(&mut shell);
    }
}
pub(crate) fn cancel_local(shell: &mut ShellState) {
    shell.message_buttons.multi_picker = None;
    if shell.overlay == Some(Overlay::MessageMultiSelect) {
        shell.overlay = None;
    }
}
pub(in crate::message_buttons) fn poll(shell: &mut ShellState) -> bool {
    if shell
        .message_buttons
        .multi_picker
        .as_ref()
        .is_some_and(|p| !current(shell, p))
    {
        cancel_local(shell);
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
