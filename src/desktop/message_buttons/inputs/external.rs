//! App option selections commit only after confirmation and final source checks.
pub(crate) mod multiple;
pub(crate) mod native_check;
use super::*;
use crate::message_buttons::external::{self as shared, Owner, Query, Row};
use std::sync::Arc;
use super_platinum_core::{block_actions::external::ExternalOption, slack::api};

#[derive(Clone)]
pub(crate) struct Ticket {
    source: Source,
    key: InputKey,
    revision: u64,
    input_revision: u64,
    display: display::Action,
    pub(crate) context: Option<Arc<crate::scheduled::Context>>,
    pub(crate) route: api::SuggestionsContext,
}
pub(crate) struct Picker {
    ticket: Ticket,
    query: Query,
    multiple: Option<multiple::Selection>,
}
#[derive(Clone)]
pub(super) struct Catalog {
    context: Option<Arc<crate::scheduled::Context>>,
    route: api::SuggestionsContext,
    raw: serde_json::Value,
    proofs: shared::Proofs,
}
impl Catalog {
    fn current(&self, shell: &ShellState, source: &Source, key: &InputKey) -> bool {
        shared::context_current(shell, &self.context)
            && route(shell, source, key).as_ref() == Some(&self.route)
            && shell
                .message_buttons
                .inputs
                .get(&ContainerKey::new(source))
                .and_then(|e| e.model.fields.get(key))
                .is_some_and(|f| f.raw == self.raw)
    }
}
#[derive(Clone)]
pub(crate) struct Proof {
    context: Option<Arc<crate::scheduled::Context>>,
    route: api::SuggestionsContext,
}
impl Proof {
    pub(crate) fn current_options(
        &self,
        shell: &ShellState,
        source: &Source,
        metadata: &BlockElementMetadata,
        selected: &[serde_json::Value],
    ) -> bool {
        let Some(key) = field_key(metadata) else {
            return false;
        };
        shared::context_current(shell, &self.context)
            && route(shell, source, &key).as_ref() == Some(&self.route)
            && selected
                .iter()
                .all(|o| ExternalOption::parse(o).is_ok_and(|o| known(shell, source, &key, &o)))
    }
    pub(crate) fn current(
        &self,
        shell: &ShellState,
        source: &Source,
        metadata: &BlockElementMetadata,
        selected: Option<&serde_json::Value>,
    ) -> bool {
        let Some(key) = field_key(metadata) else {
            return false;
        };
        shared::context_current(shell, &self.context)
            && route(shell, source, &key).as_ref() == Some(&self.route)
            && selected
                .and_then(|o| ExternalOption::parse(o).ok())
                .is_some_and(|option| known(shell, source, &key, &option))
    }
}
fn route(shell: &ShellState, source: &Source, key: &InputKey) -> Option<api::SuggestionsContext> {
    let message = source.message(shell).ok()?;
    let service =
        super::super::service::resolve(&shell.core.workspaces[&source.owner().team_id], message)
            .ok()?;
    api::SuggestionsContext::message(
        source.owner(),
        message,
        key,
        &service.app_id,
        &service.team_id,
    )
    .ok()
}
fn known(shell: &ShellState, source: &Source, key: &InputKey, option: &ExternalOption) -> bool {
    shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .is_some_and(|e| {
            e.external_proofs.get(key).is_some_and(|catalog| {
                catalog.current(shell, source, key) && catalog.proofs.contains(option)
            }) || (e.external_proofs.get(key).is_none()
                && e.model
                    .fields
                    .get(key)
                    .is_some_and(|f| f.raw.get("initial_option") == Some(option.raw())))
        })
}
pub(super) fn validate_entry(shell: &ShellState, entry: &Entry) -> Result<(), &'static str> {
    for (key, field) in &entry.model.fields {
        if let InputValue::ExternalOptions(selected) = &field.value {
            let selected = entry
                .model
                .saved_external_options(key)
                .unwrap_or_else(|| selected.clone());
            for option in &selected {
                if let Some(proofs) = entry.external_proofs.get(key) {
                    if proofs.raw != field.raw
                        || !proofs.current(shell, &entry.source, key)
                        || !proofs.proofs.contains(option)
                    {
                        return Err(UNAVAILABLE);
                    }
                } else if !matches!(&field.value, InputValue::ExternalOptions(values) if values.contains(option))
                {
                    return Err(UNAVAILABLE);
                }
            }
        }
        if let InputValue::External(selected) = &field.value {
            let selected = entry
                .model
                .saved_external(key)
                .unwrap_or_else(|| selected.clone());
            if let Some(option) = selected {
                if let Some(proofs) = entry.external_proofs.get(key) {
                    if proofs.raw != field.raw
                        || !proofs.current(shell, &entry.source, key)
                        || !proofs.proofs.contains(&option)
                    {
                        return Err(UNAVAILABLE);
                    }
                } else if field.raw.get("initial_option") != Some(option.raw())
                    && !matches!(&field.value,InputValue::External(Some(value)) if value==&option)
                {
                    return Err(UNAVAILABLE);
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn presentation(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
    mounted: &display::Mounted,
) -> (bool, String, Option<String>) {
    let unavailable = (false, "Select an option".into(), Some(UNAVAILABLE.into()));
    let Some(key) = field_key(metadata) else {
        return unavailable;
    };
    let Some(entry) = shell
        .message_buttons
        .inputs
        .get(&ContainerKey::new(source))
        .filter(|e| e.source.same_action_revision(source) && source.current(shell))
    else {
        return unavailable;
    };
    let Some(field) = entry.model.fields.get(&key) else {
        return unavailable;
    };
    if !matches!(
        field.kind,
        InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
    ) || entry.controls.get(&key) != Some(metadata)
        || route(shell, source, &key).is_none()
    {
        return unavailable;
    }
    if matches!(field.kind, InputKind::MultiExternalSelect { .. }) {
        let selected = mounted.sync_external_options(ContainerKey::new(source), key.clone(), field);
        let label = if selected.is_empty() {
            field
                .placeholder
                .clone()
                .unwrap_or_else(|| "Select options".into())
        } else {
            format!("{} selected", selected.len())
        };
        let status = super::super::operation_status(shell, source, metadata);
        return (status.enabled, label, status.reason);
    }
    let selected = mounted.sync_external(
        ContainerKey::new(source),
        key.clone(),
        field,
        entry.model.saved_external(&key),
    );
    let label = selected.map(|o| o.text().to_owned()).unwrap_or_else(|| {
        field
            .placeholder
            .clone()
            .unwrap_or_else(|| "Select an option".into())
    });
    let status = super::super::operation_status(shell, source, metadata);
    (status.enabled, label, status.reason)
}
pub(crate) fn open(
    mut state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) {
    ensure(state, source.clone());
    let owner = open_local(&mut state.write(), source, metadata, mounted);
    if let Some(owner) = owner {
        let immediate = owner.selected(&state.read()).is_none();
        shared::search(state, owner, String::new(), immediate, false);
    }
}
fn open_local(
    shell: &mut ShellState,
    source: Source,
    metadata: BlockElementMetadata,
    mounted: display::Mounted,
) -> Option<Owner> {
    if shell.overlay.is_some() || !presentation(&shell, &source, &metadata, &mounted).0 {
        return None;
    }
    let Some(key) = field_key(&metadata) else {
        return None;
    };
    let Some(route) = route(&shell, &source, &key) else {
        return None;
    };
    let Some(display) = mounted.capture(&ContainerKey::new(&source), &key) else {
        return None;
    };
    let entry = &shell.message_buttons.inputs[&ContainerKey::new(&source)];
    let min_query_length = match entry.model.fields[&key].kind {
        InputKind::ExternalSelect { min_query_length }
        | InputKind::MultiExternalSelect {
            min_query_length, ..
        } => min_query_length,
        _ => return None,
    };
    let multiple = matches!(
        entry.model.fields[&key].kind,
        InputKind::MultiExternalSelect { .. }
    )
    .then(|| {
        let selected = entry.model.saved_external_options(&key).unwrap_or_else(|| {
            mounted.sync_external_options(
                ContainerKey::new(&source),
                key.clone(),
                &entry.model.fields[&key],
            )
        });
        multiple::Selection::new(selected)
    });
    let ticket = Ticket {
        source: source.clone(),
        key: key.clone(),
        revision: next_id(),
        input_revision: entry.revision,
        display,
        context: crate::scheduled::Context::capture(&shell).map(Arc::new),
        route,
    };
    let valid_catalog = shell.message_buttons.inputs[&ContainerKey::new(&source)]
        .external_proofs
        .get(&key)
        .is_some_and(|c| c.current(&shell, &source, &key));
    let entry = shell
        .message_buttons
        .inputs
        .get_mut(&ContainerKey::new(&source))
        .unwrap();
    let field = &entry.model.fields[&key];
    if !valid_catalog {
        entry.external_proofs.insert(
            key.clone(),
            Catalog {
                context: ticket.context.clone(),
                route: ticket.route.clone(),
                raw: field.raw.clone(),
                proofs: shared::Proofs::default(),
            },
        );
    }
    if let Some(o) = field
        .raw
        .get("initial_option")
        .and_then(|v| ExternalOption::parse(v).ok())
    {
        entry
            .external_proofs
            .get_mut(&key)
            .unwrap()
            .proofs
            .remember(&o);
    }
    if let Some(initial) = field
        .raw
        .get("initial_options")
        .and_then(serde_json::Value::as_array)
    {
        for o in initial.iter().filter_map(|v| ExternalOption::parse(v).ok()) {
            entry
                .external_proofs
                .get_mut(&key)
                .unwrap()
                .proofs
                .remember(&o);
        }
    }
    shell.message_buttons.external_picker = Some(Picker {
        ticket: ticket.clone(),
        query: Query::new(min_query_length),
        multiple,
    });
    shell.overlay = Some(Overlay::MessageExternalSelect);
    Some(Owner::Message(ticket))
}
impl Ticket {
    pub(crate) fn selected(&self, shell: &ShellState) -> Option<Vec<ExternalOption>> {
        if !self.current(shell) {
            return None;
        }
        shell
            .message_buttons
            .external_picker
            .as_ref()?
            .multiple
            .as_ref()
            .map(|m| m.selected.clone())
    }
    pub(crate) fn id(&self) -> u64 {
        self.revision
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        shell.overlay == Some(Overlay::MessageExternalSelect)
            && self.source.current(shell)
            && self.display.current()
            && shared::context_current(shell, &self.context)
            && revision(shell, &self.source) == Some(self.input_revision)
            && route(shell, &self.source, &self.key).as_ref() == Some(&self.route)
            && shell
                .message_buttons
                .external_picker
                .as_ref()
                .is_some_and(|p| p.ticket.revision == self.revision)
    }
    pub(crate) fn query<'a>(&self, shell: &'a ShellState) -> Option<&'a Query> {
        shell
            .message_buttons
            .external_picker
            .as_ref()
            .map(|p| &p.query)
    }
    pub(crate) fn query_mut<'a>(&self, shell: &'a mut ShellState) -> Option<&'a mut Query> {
        shell
            .message_buttons
            .external_picker
            .as_mut()
            .map(|p| &mut p.query)
    }
    pub(crate) fn remember(&self, shell: &mut ShellState, rows: &[Row]) {
        let staged = shell
            .message_buttons
            .external_picker
            .as_ref()
            .and_then(|p| p.multiple.as_ref())
            .map(|m| {
                m.selected
                    .iter()
                    .chain(&m.opening)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let entry = shell
            .message_buttons
            .inputs
            .get_mut(&ContainerKey::new(&self.source))
            .unwrap();
        let field = &entry.model.fields[&self.key];
        let mut retained = entry
            .model
            .saved_external(&self.key)
            .flatten()
            .into_iter()
            .chain(match &field.value {
                InputValue::External(o) => o.clone(),
                _ => None,
            })
            .collect::<Vec<_>>();
        retained.extend(staged);
        retained.extend(
            entry
                .model
                .saved_external_options(&self.key)
                .unwrap_or_default(),
        );
        if let InputValue::ExternalOptions(options) = &field.value {
            retained.extend(options.clone());
        }
        let proofs = &mut entry.external_proofs.get_mut(&self.key).unwrap().proofs;
        // Keep selected proofs and current results; a query is not an unbounded cache.
        proofs.retain(
            retained
                .into_iter()
                .chain(rows.iter().map(|row| row.option.clone())),
        );
        for row in rows {
            proofs.remember(&row.option);
        }
    }
}
pub(crate) fn owner(shell: &ShellState) -> Option<Owner> {
    let t = &shell.message_buttons.external_picker.as_ref()?.ticket;
    t.current(shell).then(|| Owner::Message(t.clone()))
}
pub(crate) fn cancel(mut state: Signal<ShellState>, expected: u64) {
    let mut shell = state.write();
    if shell
        .message_buttons
        .external_picker
        .as_ref()
        .is_some_and(|p| p.ticket.revision == expected)
    {
        cancel_local(&mut shell);
    }
}
pub(crate) fn cancel_local(shell: &mut ShellState) {
    if let Some(p) = shell.message_buttons.external_picker.take() {
        if let Some(m) = p.multiple {
            // Bounded native behavior: every dismissal rolls back the opening preview.
            p.ticket.display.preview_external_options(
                &m.opening
                    .iter()
                    .map(|o| o.raw().clone())
                    .collect::<Vec<_>>(),
            );
        }
    }
    if shell.overlay == Some(Overlay::MessageExternalSelect) {
        shell.overlay = None;
    }
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    owner: Owner,
    query_revision: u64,
    index: usize,
) {
    let Owner::Message(ticket) = owner else {
        return;
    };
    let captured = {
        let mut shell = state.write();
        if !ticket.current(&shell) {
            return;
        }
        let query = ticket.query(&shell).unwrap();
        if query.revision != query_revision || query.loading {
            return;
        }
        let Some(option) = query.rows.get(index).map(|r| r.option.clone()) else {
            return;
        };
        if !known(&shell, &ticket.source, &ticket.key, &option) {
            ticket.query_mut(&mut shell).unwrap().error =
                Some("The app returned conflicting options. Choose a different value or ask the app owner to update this control.".into());
            return;
        }
        if shell
            .message_buttons
            .external_picker
            .as_ref()
            .is_some_and(|p| p.multiple.is_some())
        {
            multiple::stage(&mut shell, &ticket, option);
            return;
        }
        let proof = Proof {
            context: ticket.context.clone(),
            route: ticket.route.clone(),
        };
        let Ok(mut captured) = ticket.source.capture_external(
            &shell,
            &ticket.key.block_id,
            &ticket.key.action_id,
            Some(&option),
            proof,
        ) else {
            return;
        };
        captured.set_display(ticket.display.clone());
        shell.message_buttons.external_picker = None;
        shell.overlay = None;
        if captured.confirmation().is_some() {
            shell.message_buttons.confirmation = Some(Pending {
                captured,
                revision: next_id(),
                input_revision: Some(ticket.input_revision),
            });
            shell.overlay = Some(Overlay::ButtonConfirmation);
            return;
        }
        captured
    };
    super::super::dispatch(state, captured);
}
pub(in crate::message_buttons) fn poll(shell: &mut ShellState) -> bool {
    if shell
        .message_buttons
        .external_picker
        .as_ref()
        .is_some_and(|p| !p.ticket.current(shell))
    {
        cancel_local(shell);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod multiple_tests;
#[cfg(test)]
mod tests;
pub(crate) use multiple::remove;
