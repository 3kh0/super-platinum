//! Multi-user drafts are private to the picker until Submit; Cancel publishes nothing.
use super::*;

pub(super) struct Selection {
    pub selected: Vec<String>,
    pub proofs: super::super::channels::Proofs,
    channels: bool,
    conversations: bool,
    max: Option<usize>,
    required: bool,
}

pub(crate) fn display_selected_users(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> Vec<String> {
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
    mounted.sync_users(
        ContainerKey::new(source),
        key.clone(),
        field,
        if matches!(field.kind, InputKind::MultiChannel { .. }) {
            entry.model.saved_channels(&key)
        } else if matches!(field.kind, InputKind::MultiConversation { .. }) {
            entry.model.saved_conversations(&key)
        } else {
            entry.model.saved_users(&key)
        },
    )
}

pub(super) fn selection(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> Option<Selection> {
    if !is_multiple(metadata) {
        return None;
    }
    let entry = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))?;
    let field = entry.model.fields.get(&field_key(metadata)?)?;
    let (max_selected_items, channels, conversations) = match &field.kind {
        InputKind::MultiUser { max_selected_items } => (max_selected_items, false, false),
        InputKind::MultiChannel { max_selected_items } => (max_selected_items, true, false),
        InputKind::MultiConversation { max_selected_items } => (max_selected_items, false, true),
        _ => return None,
    };
    Some(Selection {
        channels,
        conversations,
        proofs: entry.channel_choices.clone(),
        selected: display_selected_users(shell, source, metadata, mounted),
        max: *max_selected_items,
        required: !field.optional,
    })
}

pub(super) fn error(selection: &Selection) -> Option<String> {
    if selection.required && selection.selected.is_empty() {
        Some(format!(
            "Choose at least one {}",
            if selection.conversations {
                "conversation"
            } else if selection.channels {
                "channel"
            } else {
                "user"
            }
        ))
    } else if selection
        .max
        .is_some_and(|max| selection.selected.len() > max)
    {
        Some(format!(
            "Choose at most {} {}",
            selection.max.unwrap(),
            if selection.conversations {
                "conversations"
            } else if selection.channels {
                "channels"
            } else {
                "users"
            }
        ))
    } else {
        None
    }
}

pub(super) fn toggle(selection: &mut Selection, id: String) {
    if let Some(index) = selection.selected.iter().position(|old| old == &id) {
        selection.selected.remove(index);
    } else {
        selection.selected.push(id);
    }
}

pub(crate) fn remove_user(mut state: Signal<ShellState>, expected: u64, id: String) {
    let mut shell = state.write();
    let Some(picker) = shell.message_buttons.user_picker.as_ref() else {
        return;
    };
    if picker.revision != expected || !current(&shell, picker) {
        return;
    }
    if let Some(selection) = shell
        .message_buttons
        .user_picker
        .as_mut()
        .and_then(|p| p.multiple.as_mut())
    {
        selection.selected.retain(|user| user != &id);
    }
}

pub(crate) fn submit_users(mut state: Signal<ShellState>, expected: u64) {
    let captured = {
        let mut shell = state.write();
        let Some(picker) = shell.message_buttons.user_picker.as_ref() else {
            return;
        };
        if picker.revision != expected
            || !current(&shell, picker)
            || picker.multiple.as_ref().is_none_or(|m| error(m).is_some())
            || channel_error(&shell, picker).is_some()
        {
            return;
        }
        let key = field_key(&picker.metadata).unwrap();
        let selection = picker.multiple.as_ref().unwrap();
        let action = if selection.conversations {
            picker.source.capture_conversations(
                &shell,
                &key.block_id,
                &key.action_id,
                &selection.selected,
                &selection.proofs,
            )
        } else if selection.channels {
            picker.source.capture_channels(
                &shell,
                &key.block_id,
                &key.action_id,
                &selection.selected,
                &selection.proofs,
            )
        } else {
            picker
                .source
                .capture_users(&shell, &key.block_id, &key.action_id, &selection.selected)
        };
        let Ok(mut captured) = action else {
            return;
        };
        if captured.metadata() != &picker.metadata {
            return;
        }
        captured.set_display(picker.display.clone());
        let input_revision = Some(picker.input_revision);
        cancel_local(&mut shell);
        if captured.confirmation().is_some() {
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
    super::super::super::dispatch(state, captured);
}

pub(super) fn channel_error(shell: &ShellState, picker: &Picker) -> Option<String> {
    let selection = picker.multiple.as_ref()?;
    if !selection.channels && !selection.conversations {
        return None;
    }
    let workspace = shell.core.workspaces.get(&picker.source.owner().team_id)?;
    let validate = if selection.conversations {
        super::super::conversations::validate
    } else {
        super::super::channels::validate
    };
    validate(
        workspace,
        picker.filter.as_ref()?,
        &selection.selected,
        &selection.proofs,
    )
    .err()
    .map(str::to_owned)
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod channel_tests;

#[cfg(test)]
mod conversation_tests;
