//! Inline modal selection is local form state, never a block action.
//! Exact raw options are retained, including saved metadata; the modern Slack
//! wrapper's text/value-only saved projection is deliberately not reproduced.
use super::*;
use super_platinum_core::block_actions::external::ExternalOption;

pub(super) fn options(value: &InputValue) -> Vec<ExternalOption> {
    match value {
        InputValue::External(option) => option.iter().cloned().collect(),
        InputValue::ExternalOptions(options) => options.clone(),
        _ => vec![],
    }
}

impl Ticket {
    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        self.visit == other.visit
            && self.generation == other.generation
            && self.revision == other.revision
            && self.field_revision == other.field_revision
            && self.key == other.key
            && self.view == other.view
            && self.value == other.value
            && self.raw == other.raw
            && self.route == other.route
            && self.self_user == other.self_user
    }

    pub(crate) fn selected(&self, shell: &ShellState) -> Option<Vec<ExternalOption>> {
        if !self.current(shell) {
            return None;
        }
        match &self.value {
            InputValue::ExternalOptions(options) => Some(options.clone()),
            _ => None,
        }
    }
}

/// Caller has checked the exact accepted row and its catalog proof.
pub(super) fn toggle(shell: &mut ShellState, ticket: Ticket, option: ExternalOption) -> bool {
    let Some(mut options) = ticket.selected(shell) else {
        return false;
    };
    if let Some(index) = options.iter().position(|old| old.value() == option.value()) {
        options.remove(index);
    } else {
        options.push(option);
    }
    commit(shell, ticket, options)
}

pub(crate) fn remove(mut state: Signal<ShellState>, owner: Owner, value: String) {
    remove_local(&mut state.write(), owner, &value);
}

pub(super) fn remove_local(shell: &mut ShellState, owner: Owner, value: &str) -> bool {
    let Owner::Modal(ticket) = owner else {
        return false;
    };
    let Some(mut options) = ticket.selected(shell) else {
        return false;
    };
    let Some(index) = options.iter().position(|option| option.value() == value) else {
        return false;
    };
    options.remove(index);
    // Requiredness and the current maximum are submission validation. Retain an
    // empty/over-limit array so the user can correct it without losing choices.
    commit(shell, ticket, options)
}

fn commit(shell: &mut ShellState, ticket: Ticket, options: Vec<ExternalOption>) -> bool {
    if !ticket.current(shell) {
        return false;
    }
    let catalog_key = (ticket.view.clone(), ticket.key.clone());
    let mut catalog_current = shell
        .app_modal
        .as_ref()
        .unwrap()
        .external_proofs
        .get(&catalog_key)
        .is_some_and(|catalog| catalog.current(shell, &ticket));
    let session = shell.app_modal.as_mut().unwrap();
    // A collapsed pristine chip may be removed without ever opening search.
    // Its remaining initial/saved options have the same source authority as
    // opening this exact field. Never promote dirty or conflicting evidence.
    if !catalog_current && !session.current_form().unwrap().fields[&ticket.key].dirty {
        let mut proofs = Proofs::default();
        for option in self::options(&ticket.value) {
            proofs.remember(&option);
        }
        session.external_proofs.insert(
            catalog_key.clone(),
            Catalog {
                owner: ticket.clone(),
                proofs,
            },
        );
        catalog_current = true;
    }
    if session
        .current_form_mut()
        .unwrap()
        .set_value(&ticket.key, InputValue::ExternalOptions(options))
        .is_err()
    {
        return false;
    }
    let field = session
        .current_form_mut()
        .unwrap()
        .fields
        .get_mut(&ticket.key)
        .unwrap();
    field.revision = field.revision.wrapping_add(1);
    let mut updated = Ticket::capture(shell, &ticket.key).unwrap();
    let session = shell.app_modal.as_mut().unwrap();
    if let Some(picker) = session
        .external_picker
        .as_mut()
        .filter(|picker| picker.ticket.key == ticket.key && picker.ticket.view == ticket.view)
    {
        updated.visit = picker.ticket.visit;
        // Old callbacks carry the old exact selected value. Retire their query
        // too, preserving its route cooldown and accepted rows for correction.
        picker.query.retire_request();
        picker.ticket = updated.clone();
    }
    if catalog_current {
        session.external_proofs.get_mut(&catalog_key).unwrap().owner = updated;
    }
    true
}
