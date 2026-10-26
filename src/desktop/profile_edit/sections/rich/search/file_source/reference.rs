//! Existing-token references share the credential-owned collection. Event facts
//! survive sockets; a captured opening never borrows a successor socket.
use super::*;
use super_platinum_core::file_actions::FileDeletionEvent;

#[derive(Clone)]
pub(in crate::profile_edit::sections::rich) struct Reference {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    row: source::ReferenceObservation,
    events: u64,
}
impl Reference {
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &ShellState,
        team: &str,
        id: &str,
    ) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        let state = &shell.profile_edit.sections.file_search;
        if !state
            .owner
            .as_ref()
            .is_some_and(|o| o.account_current(shell))
        {
            return Err(unavailable());
        }
        let slot = state.slots.get(team).ok_or_else(unavailable)?;
        let row = slot.store.reference(id)?;
        let observed = Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            row,
            events: crate::file_events::snapshot_revision(shell),
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(in crate::profile_edit::sections::rich) fn row(&self) -> &source::ReferenceObservation {
        &self.row
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        let team = &self.owner.session.team_id;
        self.owner.current(shell)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && Arc::ptr_eq(&state.identity, &self.state)
            && !crate::file_events::changed_since(shell, team, self.row.id(), self.events)
            && state.slots.get(team).is_some_and(|slot| {
                Arc::ptr_eq(&slot.identity, &self.slot)
                    && !slot.invalidated.contains(self.row.id())
                    && self.row.current(&slot.store)
                    && if self.row.is_event_deletion() {
                        shell
                            .core
                            .workspaces
                            .get(team)
                            .is_some_and(|w| w.deleted_files.contains(self.row.id()))
                    } else {
                        slot.stamps
                            .get(self.row.id())
                            .is_some_and(|stamp| stamp.current(shell, self.row.id()))
                    }
            })
    }
}

/// Capture before files.info. Publication checks this exact source predecessor,
/// not a freshly recaptured collection after the response arrives.
pub(in crate::profile_edit::sections::rich) struct ReferenceRead {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    collection: source::Collection,
    revision: u64,
    events: u64,
    id: String,
}
impl ReferenceRead {
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &mut ShellState,
        team: &str,
        id: &str,
    ) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        State::ensure_source(shell, &owner);
        let state = &shell.profile_edit.sections.file_search;
        let slot = state.slots.get(team).ok_or_else(unavailable)?;
        let read = Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            collection: slot.store.candidates(),
            revision: slot.revision,
            events: crate::file_events::snapshot_revision(shell),
            id: id.into(),
        };
        read.current(shell).then_some(read).ok_or_else(unavailable)
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        let team = &self.owner.session.team_id;
        self.owner.current(shell)
            && Arc::ptr_eq(&state.identity, &self.state)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && !crate::file_events::changed_since(shell, team, &self.id, self.events)
            && shell
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| !w.deleted_files.contains(&self.id))
            && state.slots.get(team).is_some_and(|s| {
                Arc::ptr_eq(&s.identity, &self.slot)
                    && s.revision == self.revision
                    && self.collection.current(&s.store)
            })
    }
    pub(in crate::profile_edit::sections::rich) fn publish(
        self,
        shell: &mut ShellState,
        response: &source::FileReferenceResponse,
    ) -> Result<source::Store, Error> {
        if response.id() != self.id || !self.current(shell) {
            return Err(unavailable());
        }
        let slot = shell
            .profile_edit
            .sections
            .file_search
            .slots
            .get_mut(&self.owner.session.team_id)
            .ok_or_else(unavailable)?;
        // Unsupported original updates still retire the old shared row. Never
        // recover an old observation after a failed ingestion.
        let result = slot
            .store
            .ingest_reference(&self.owner.session.team_id, response);
        slot.invalidated.remove(&self.id);
        slot.stamps.insert(
            self.id,
            Stamp {
                owner: self.owner,
                events: self.events,
            },
        );
        result?;
        Ok(slot.store.clone())
    }
}

/// Called only at the validated realtime event boundary. The core plan performs
/// original known-ID filtering; absent IDs never acquire a source row.
pub(crate) fn deletion(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &FileDeletionEvent,
) {
    let Ok(owner) = Context::capture(shell, team) else {
        return;
    };
    if owner.realtime_generation() != generation || !owner.current(shell) {
        return;
    }
    let state = &shell.profile_edit.sections.file_search;
    if !state
        .owner
        .as_ref()
        .is_some_and(|o| o.account_current(shell))
    {
        return;
    }
    let Some(slot) = state.slots.get(team) else {
        return;
    };
    let plan = slot.store.prepare_deletion(event);
    let slot = shell
        .profile_edit
        .sections
        .file_search
        .slots
        .get_mut(team)
        .unwrap();
    if let Ok(rows) = slot.store.apply_deletion(plan) {
        if !rows.is_empty() {
            slot.revision = slot.revision.wrapping_add(1);
        }
        for row in rows {
            slot.invalidated.remove(row.id());
            slot.stamps.remove(row.id());
        }
    }
}

pub(crate) fn fixture_proof(shell: &ShellState, team: &str, ids: &[&str]) -> Value {
    if !crate::profile_edit::sections::fixture::deletion_event::named()
        && !(crate::profile_edit::sections::fixture::section_reference::named()
            && crate::profile_edit::sections::fixture::section_reference::case() == "status")
        && !cfg!(test)
    {
        return Value::Null;
    }
    let state = &shell.profile_edit.sections.file_search;
    if !state
        .owner
        .as_ref()
        .is_some_and(|o| o.account_current(shell))
    {
        return Value::Null;
    }
    let Some(slot) = state.slots.get(team) else {
        return Value::Null;
    };
    let rows: Vec<_> = ids.iter().map(|id| {
        let reference = slot.store.reference(id).ok();
        serde_json::json!({"id":id,"source":slot.store.source(id),
            "effective":reference.as_ref().map(|r|r.effective()),
            "receipt":reference.as_ref().map(|r|r.receipt()),
            "event":reference.as_ref().and_then(|r|r.deletion_event()).map(|e|e.event().raw()),
            "preimageSource":reference.as_ref().and_then(|r|r.deletion_event()).map(|e|e.preimage_source()),
            "preimageReceipt":reference.as_ref().and_then(|r|r.deletion_event()).map(|e|e.preimage_receipt()),
            "current":Reference::capture(shell,team,id).is_ok(),
            "workspaceDeleted":shell.core.workspaces.get(team).is_some_and(|w|w.deleted_files.contains(*id))})
    }).collect();
    serde_json::json!({"rows":rows,"revision":slot.revision,"queryCount":state.queries.len()})
}

#[cfg(test)]
mod tests;
