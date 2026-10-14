//! External options in modal inputs are local edits until views.submit.
use super::*;
use crate::message_buttons::external::{self as shared, Owner, Proofs, Query, Row};
use std::sync::Arc;
use super_platinum_core::block_actions::form::InputKind;
mod multi;
pub(crate) use multi::remove;

#[derive(Clone)]
pub(crate) struct Ticket {
    pub(crate) context: Option<Arc<Context>>,
    pub(crate) route: api::SuggestionsContext,
    generation: u64,
    revision: u64,
    field_revision: u64,
    key: InputKey,
    view: String,
    raw: Value,
    value: InputValue,
    self_user: String,
    visit: u64,
}
pub(crate) struct Picker {
    ticket: Ticket,
    query: Query,
}
#[derive(Clone)]
pub(crate) struct Catalog {
    owner: Ticket,
    proofs: Proofs,
}
impl Catalog {
    fn current(&self, shell: &ShellState, ticket: &Ticket) -> bool {
        shared::context_current(shell, &self.owner.context)
            && self.owner.generation == ticket.generation
            && self.owner.revision == ticket.revision
            && self.owner.raw == ticket.raw
            && self.owner.field_revision == ticket.field_revision
            && self.owner.self_user == ticket.self_user
            && self.owner.route == ticket.route
    }
}
impl Ticket {
    pub(crate) fn id(&self) -> u64 {
        self.visit
    }
    pub(crate) fn capture(shell: &ShellState, key: &InputKey) -> Option<Self> {
        let s = shell.app_modal.as_ref()?;
        if !current_scope(shell, s) || s.locked() || shell.overlay != Some(Overlay::AppModal) {
            return None;
        }
        let form = s.current_form()?;
        if !form.unsupported.is_empty() {
            return None;
        }
        let field = form.fields.get(key)?;
        if !matches!(
            field.kind,
            InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
        ) {
            return None;
        }
        let self_user = shell
            .core
            .workspaces
            .get(&s.modal.scope().team)?
            .self_user_id
            .clone();
        let route = api::SuggestionsContext::modal(
            s.modal.scope(),
            &self_user,
            s.modal.stack().last()?,
            key,
        )
        .ok()?;
        Some(Self {
            context: Context::capture(shell).map(Arc::new),
            route,
            generation: s.generation,
            revision: s.modal.revision(),
            field_revision: field.revision,
            key: key.clone(),
            view: form.view_id.clone(),
            raw: field.raw.clone(),
            value: field.value.clone(),
            self_user,
            visit: 0,
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !shared::context_current(shell, &self.context) {
            return false;
        }
        Self::capture(shell, &self.key).is_some_and(|t| {
            t.generation == self.generation
                && t.revision == self.revision
                && t.field_revision == self.field_revision
                && t.view == self.view
                && t.raw == self.raw
                && t.value == self.value
                && t.route == self.route
                && t.self_user == self.self_user
        })
    }
    pub(crate) fn query<'a>(&self, shell: &'a ShellState) -> Option<&'a Query> {
        shell
            .app_modal
            .as_ref()?
            .external_picker
            .as_ref()
            .filter(|p| {
                p.ticket.visit == self.visit
                    && p.ticket.key == self.key
                    && p.ticket.revision == self.revision
                    && p.ticket.field_revision == self.field_revision
            })
            .map(|p| &p.query)
    }
    pub(crate) fn query_mut<'a>(&self, shell: &'a mut ShellState) -> Option<&'a mut Query> {
        shell
            .app_modal
            .as_mut()?
            .external_picker
            .as_mut()
            .filter(|p| {
                p.ticket.visit == self.visit
                    && p.ticket.key == self.key
                    && p.ticket.revision == self.revision
                    && p.ticket.field_revision == self.field_revision
            })
            .map(|p| &mut p.query)
    }
    pub(crate) fn remember(&self, shell: &mut ShellState, rows: &[Row]) {
        let s = shell.app_modal.as_mut().unwrap();
        let selected = multi::options(&s.current_form().unwrap().fields[&self.key].value);
        let catalog = s
            .external_proofs
            .entry((self.view.clone(), self.key.clone()))
            .or_insert_with(|| Catalog {
                owner: self.clone(),
                proofs: Proofs::default(),
            });
        catalog.proofs.retain(
            selected
                .into_iter()
                .chain(rows.iter().map(|r| r.option.clone())),
        );
        for row in rows {
            catalog.proofs.remember(&row.option);
        }
    }
}
pub(crate) fn owner(shell: &ShellState, key: &InputKey) -> Option<Owner> {
    let ticket = shell
        .app_modal
        .as_ref()?
        .external_picker
        .as_ref()?
        .ticket
        .clone();
    if ticket.key != *key || !ticket.current(shell) {
        return None;
    }
    ticket.query(shell)?;
    Some(Owner::Modal(ticket))
}
pub(crate) fn open(mut state: Signal<ShellState>, expected: Ticket) {
    let immediate = !matches!(expected.value, InputValue::ExternalOptions(_));
    let owner = open_local(&mut state.write(), expected);
    if let Some(owner) = owner {
        shared::search(state, owner, String::new(), immediate, false);
    }
}
fn open_local(shell: &mut ShellState, expected: Ticket) -> Option<Owner> {
    if !expected.current(shell) {
        return None;
    }
    let key = expected.key.clone();
    let Some(mut ticket) = Ticket::capture(shell, &key) else {
        return None;
    };
    if owner(shell, &key).is_some() {
        return None;
    }
    ticket.visit = next_id();
    let previous = shell
        .app_modal
        .as_ref()
        .unwrap()
        .external_proofs
        .get(&(ticket.view.clone(), key.clone()));
    if previous.is_some_and(|c| !shared::context_current(shell, &c.owner.context)) {
        shell
            .app_modal
            .as_mut()
            .unwrap()
            .external_proofs
            .remove(&(ticket.view.clone(), key.clone()));
    }
    let s = shell.app_modal.as_mut().unwrap();
    let field = &s.current_form().unwrap().fields[&key];
    let min_query_length = match field.kind {
        InputKind::ExternalSelect { min_query_length }
        | InputKind::MultiExternalSelect {
            min_query_length, ..
        } => min_query_length,
        _ => return None,
    };
    let selected = multi::options(&field.value);
    let catalog_key = (ticket.view.clone(), key.clone());
    let dirty = field.dirty;
    let stale = s.external_proofs.get(&catalog_key).is_some_and(|c| {
        c.owner.raw != ticket.raw
            || c.owner.revision != ticket.revision
            || c.owner.generation != ticket.generation
            || c.owner.field_revision != ticket.field_revision
            || c.owner.route != ticket.route
    });
    if stale {
        s.external_proofs.remove(&catalog_key);
    }
    let catalog = s
        .external_proofs
        .entry(catalog_key)
        .or_insert_with(|| Catalog {
            owner: ticket.clone(),
            proofs: Proofs::default(),
        });
    if !dirty {
        for o in selected {
            catalog.proofs.remember(&o);
        }
    }
    s.external_picker = Some(Picker {
        ticket: ticket.clone(),
        query: Query::new(min_query_length),
    });
    Some(Owner::Modal(ticket))
}
pub(crate) fn close(mut state: Signal<ShellState>, expected: u64) {
    if let Some(s) = state.write().app_modal.as_mut() {
        if s.external_picker
            .as_ref()
            .is_some_and(|p| p.ticket.visit == expected)
        {
            s.external_picker = None;
        }
    }
}
pub(crate) fn choose(mut state: Signal<ShellState>, owner: Owner, revision: u64, index: usize) {
    choose_local(&mut state.write(), owner, revision, index);
}
fn choose_local(shell: &mut ShellState, owner: Owner, revision: u64, index: usize) -> bool {
    let Owner::Modal(ticket) = owner else {
        return false;
    };
    if !ticket.current(shell) {
        return false;
    }
    let Some(query) = ticket.query(shell) else {
        return false;
    };
    if query.revision != revision || query.loading {
        return false;
    }
    let Some(option) = query.rows.get(index).map(|r| r.option.clone()) else {
        return false;
    };
    if !shell
        .app_modal
        .as_ref()
        .unwrap()
        .external_proofs
        .get(&(ticket.view.clone(), ticket.key.clone()))
        .is_some_and(|c| c.current(shell, &ticket) && c.proofs.contains(&option))
    {
        ticket.query_mut(shell).unwrap().error =
            Some("The app returned conflicting options. Choose a different value or ask the app owner to update this form.".into());
        return false;
    }
    if matches!(ticket.value, InputValue::ExternalOptions(_)) {
        return multi::toggle(shell, ticket, option);
    }
    let s = shell.app_modal.as_mut().unwrap();
    if s.current_form_mut()
        .unwrap()
        .set_value(&ticket.key, InputValue::External(Some(option)))
        .is_ok()
    {
        s.external_picker = None;
        let updated = Ticket::capture(shell, &ticket.key).unwrap();
        shell
            .app_modal
            .as_mut()
            .unwrap()
            .external_proofs
            .get_mut(&(ticket.view.clone(), ticket.key.clone()))
            .unwrap()
            .owner = updated;
        return true;
    }
    false
}
pub(crate) fn clear(mut state: Signal<ShellState>, ticket: Ticket) {
    clear_local(&mut state.write(), ticket);
}
fn clear_local(shell: &mut ShellState, ticket: Ticket) -> bool {
    if !ticket.current(shell) {
        return false;
    }
    let key = ticket.key.clone();
    let s = shell.app_modal.as_mut().unwrap();
    if !s.current_form().unwrap().fields[&key].optional {
        return false;
    }
    if s.current_form_mut()
        .unwrap()
        .set_value(&ticket.key, InputValue::External(None))
        .is_ok()
    {
        s.external_picker = None;
        s.external_proofs.remove(&(ticket.view.clone(), key));
        return true;
    }
    false
}
pub(super) fn validate(shell: &ShellState) -> Result<(), FormError> {
    let Some(s) = shell.app_modal.as_ref() else {
        return Ok(());
    };
    let Some(form) = s.current_form() else {
        return Ok(());
    };
    for (key, field) in &form.fields {
        if !matches!(
            field.value,
            InputValue::External(_) | InputValue::ExternalOptions(_)
        ) {
            continue;
        }
        let Some(ticket) = Ticket::capture(shell, key) else {
            return Err(FormError::Unsupported(vec![
                "This app option field is unavailable or has changed".into(),
            ]));
        };
        for option in multi::options(&field.value) {
            if let Some(c) = s.external_proofs.get(&(form.view_id.clone(), key.clone())) {
                if (c.current(shell, &ticket) && !c.proofs.contains(&option))
                    || (!c.current(shell, &ticket) && field.dirty)
                {
                    return Err(FormError::Unsupported(vec![
                        "The selected app option could not be verified".into(),
                    ]));
                }
            } else if field.dirty {
                return Err(FormError::Unsupported(vec![
                    "The selected app option could not be verified".into(),
                ]));
            }
        }
    }
    Ok(())
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    if shell
        .app_modal
        .as_ref()
        .and_then(|s| s.external_picker.as_ref())
        .is_some_and(|p| !p.ticket.current(shell))
    {
        shell.app_modal.as_mut().unwrap().external_picker = None;
        true
    } else {
        false
    }
}

pub(crate) fn install_fixture(shell: &mut ShellState) {
    shell.app_modal = Some(Session::fixture(
        crate::message_buttons::external_fixture::view(),
    ));
    shell.overlay = Some(Overlay::AppModal);
}
pub(crate) fn complete_fixture(mut state: Signal<ShellState>, uncertain: bool) {
    if !crate::message_buttons::external_fixture::enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() {
        return;
    }
    let Some((view, revision, token)) = shell
        .message_buttons
        .external_fixture
        .as_mut()
        .and_then(|f| f.modal_pending.take())
    else {
        return;
    };
    requests::complete(
        &mut shell,
        &view,
        revision,
        &token,
        if uncertain {
            Err(super_platinum_core::slack::Error::Api(
                "fixture_unknown_outcome".into(),
            ))
        } else {
            Ok(api::BlockViewResponse::Rejected(api::BlockViewRejection {
                error: "validation_errors".into(),
                message: None,
                errors: [("choice".into(), "Choose a different release.".into())].into(),
                view_error: None,
            }))
        },
    );
}

#[cfg(test)]
mod multi_tests;
#[cfg(test)]
mod tests;
