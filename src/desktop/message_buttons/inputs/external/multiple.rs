//! Ordinary multi selection has a mount-owned preview and a separate committed wire value.
use super::*;
use super_platinum_core::block_actions::{BlockConfirmation, BlockConfirmationState};

pub(super) struct Selection {
    pub opening: Vec<ExternalOption>,
    pub selected: Vec<ExternalOption>,
    confirming: bool,
}
impl Selection {
    pub(super) fn new(selected: Vec<ExternalOption>) -> Self {
        Self {
            opening: selected.clone(),
            selected,
            confirming: false,
        }
    }
}
pub(crate) struct Presentation {
    pub confirmation: Option<BlockConfirmation>,
    pub confirming: bool,
    pub enabled: bool,
    pub reason: Option<String>,
}
pub(crate) fn picker(shell: &ShellState) -> Option<Presentation> {
    let p = shell.message_buttons.external_picker.as_ref()?;
    let selected = p.multiple.as_ref()?;
    let entry = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(&p.ticket.source))?;
    let field = entry.model.fields.get(&p.ticket.key)?;
    let InputKind::MultiExternalSelect {
        max_selected_items, ..
    } = field.kind
    else {
        return None;
    };
    let metadata = entry.controls.get(&p.ticket.key)?;
    let live = p.ticket.current(shell)
        && selected
            .selected
            .iter()
            .all(|o| known(shell, &p.ticket.source, &p.ticket.key, o));
    let over = max_selected_items.is_some_and(|max| selected.selected.len() > max);
    let missing = !field.optional && selected.selected.is_empty();
    Some(Presentation {
        confirmation: match &metadata.confirm {
            BlockConfirmationState::Parsed(c) => Some(c.clone()),
            _ => None,
        },
        confirming: selected.confirming,
        enabled: live && !over && !missing,
        reason: if !live {
            Some(UNAVAILABLE.into())
        } else if over {
            Some(format!(
                "Choose at most {} options",
                max_selected_items.unwrap()
            ))
        } else if missing {
            Some("Choose at least one option".into())
        } else {
            None
        },
    })
}
pub(super) fn stage(shell: &mut ShellState, ticket: &Ticket, option: ExternalOption) -> bool {
    if !ticket.current(shell) || !known(shell, &ticket.source, &ticket.key, &option) {
        return false;
    }
    let p = shell.message_buttons.external_picker.as_mut().unwrap();
    let Some(multi) = p.multiple.as_mut().filter(|m| !m.confirming) else {
        return false;
    };
    if let Some(index) = multi
        .selected
        .iter()
        .position(|o| o.value() == option.value())
    {
        multi.selected.remove(index);
    } else {
        if multi.selected.len() >= 1000 {
            p.query.error = Some("Choose at most 1000 options".into());
            return false;
        }
        multi.selected.push(option);
    }
    p.ticket.display.preview_external_options(
        &multi
            .selected
            .iter()
            .map(|o| o.raw().clone())
            .collect::<Vec<_>>(),
    )
}
pub(crate) fn remove(mut state: Signal<ShellState>, owner: Owner, value: String) {
    remove_local(&mut state.write(), owner, &value);
}
pub(super) fn remove_local(shell: &mut ShellState, owner: Owner, value: &str) -> bool {
    let Owner::Message(ticket) = owner else {
        return false;
    };
    if !ticket.current(shell) {
        return false;
    }
    let Some(p) = shell.message_buttons.external_picker.as_mut() else {
        return false;
    };
    let Some(m) = p.multiple.as_mut().filter(|m| !m.confirming) else {
        return false;
    };
    let Some(index) = m.selected.iter().position(|o| o.value() == value) else {
        return false;
    };
    // Removing a now-conflicting option must remain possible so the user can correct it.
    m.selected.remove(index);
    p.ticket.display.preview_external_options(
        &m.selected
            .iter()
            .map(|o| o.raw().clone())
            .collect::<Vec<_>>(),
    )
}
pub(crate) fn confirmation_cancel(mut state: Signal<ShellState>, expected: u64) {
    confirmation_cancel_local(&mut state.write(), expected);
}
pub(super) fn confirmation_cancel_local(shell: &mut ShellState, expected: u64) {
    if let Some(p) = shell
        .message_buttons
        .external_picker
        .as_mut()
        .filter(|p| p.ticket.revision == expected)
    {
        if let Some(m) = &mut p.multiple {
            m.confirming = false;
        }
    }
}
pub(crate) fn apply(mut state: Signal<ShellState>, expected: u64) {
    let captured = apply_local(&mut state.write(), expected);
    if let Some(captured) = captured {
        super::super::super::dispatch(state, captured);
    }
}
pub(super) fn apply_local(shell: &mut ShellState, expected: u64) -> Option<Captured> {
    let view = picker(shell)?;
    let p = shell.message_buttons.external_picker.as_ref()?;
    if p.ticket.revision != expected || !view.enabled {
        return None;
    }
    if view.confirmation.is_some() && !view.confirming {
        shell
            .message_buttons
            .external_picker
            .as_mut()?
            .multiple
            .as_mut()?
            .confirming = true;
        return None;
    }
    let proof = Proof {
        context: p.ticket.context.clone(),
        route: p.ticket.route.clone(),
    };
    let mut captured = p
        .ticket
        .source
        .capture_external_options(
            shell,
            &p.ticket.key.block_id,
            &p.ticket.key.action_id,
            &p.multiple.as_ref()?.selected,
            proof,
        )
        .ok()?;
    captured.set_display(p.ticket.display.clone());
    shell.message_buttons.external_picker = None;
    shell.overlay = None;
    Some(captured)
}
