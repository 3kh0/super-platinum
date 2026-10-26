//! Section snippets are an independent nonpersistent source in the existing
//! credential-owned file slot. Files.info and collaboration bodies cannot
//! publish this source. Consumer hydration/scheduling is deliberately separate.
use super::*;
use super_platinum_core::{
    canvas::section_titles,
    slack::api::{CanvasSectionsRequest, CanvasSectionsResponse},
};

pub(in crate::profile_edit::sections::rich) fn initialize(
    shell: &mut ShellState,
    team: &str,
) -> Result<(), Error> {
    let owner = Arc::new(Context::capture(shell, team)?);
    State::ensure_source(shell, &owner);
    Ok(())
}

#[derive(Clone)]
pub(in crate::profile_edit::sections::rich) struct Observation {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    events: u64,
    row: section_titles::Observation,
}
impl Observation {
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &ShellState,
        team: &str,
        file: &str,
        section: &str,
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
        let observed = Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            events: crate::file_events::snapshot_revision(shell),
            row: slot.section_titles.observe(file, section)?,
        };
        observed
            .current(shell)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(in crate::profile_edit::sections::rich) fn row(&self) -> &section_titles::Observation {
        &self.row
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        self.owner.current(shell)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && Arc::ptr_eq(&state.identity, &self.state)
            && !crate::file_events::changed_since(
                shell,
                &self.owner.session.team_id,
                self.row.file_id(),
                self.events,
            )
            && state
                .slots
                .get(&self.owner.session.team_id)
                .is_some_and(|slot| {
                    Arc::ptr_eq(&slot.identity, &self.slot)
                        && self.row.current(&slot.section_titles)
                })
    }
}

/// Exact pre-request source custody. No request is sent by this adapter and no
/// error from a retired request may clear a successor's snippets.
pub(in crate::profile_edit::sections::rich) struct Read {
    owner: Arc<Context>,
    state: Arc<()>,
    slot: Arc<()>,
    metadata: source::Collection,
    revision: u64,
    events: u64,
    request: CanvasSectionsRequest,
    read: section_titles::Read,
}
impl Read {
    pub(in crate::profile_edit::sections::rich) fn capture(
        shell: &mut ShellState,
        team: &str,
        request: CanvasSectionsRequest,
    ) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        State::ensure_source(shell, &owner);
        let state = &shell.profile_edit.sections.file_search;
        let slot = state.slots.get(team).ok_or_else(unavailable)?;
        let read = Self {
            owner,
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            metadata: slot.store.candidates(),
            revision: slot.revision,
            events: crate::file_events::snapshot_revision(shell),
            read: slot.section_titles.capture_read(&request)?,
            request,
        };
        read.current(shell).then_some(read).ok_or_else(unavailable)
    }
    pub(in crate::profile_edit::sections::rich) fn request(&self) -> &CanvasSectionsRequest {
        &self.request
    }
    pub(in crate::profile_edit::sections::rich) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        self.owner.current(shell)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && Arc::ptr_eq(&state.identity, &self.state)
            && !crate::file_events::changed_since(
                shell,
                &self.owner.session.team_id,
                self.request.file_id(),
                self.events,
            )
            && state
                .slots
                .get(&self.owner.session.team_id)
                .is_some_and(|slot| {
                    Arc::ptr_eq(&slot.identity, &self.slot)
                        && slot.revision == self.revision
                        && self.metadata.current(&slot.store)
                        && self.read.current(&slot.section_titles)
                })
    }
    pub(in crate::profile_edit::sections::rich) fn publish(
        self,
        shell: &mut ShellState,
        response: &CanvasSectionsResponse,
    ) -> Result<(), Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        let slot = shell
            .profile_edit
            .sections
            .file_search
            .slots
            .get_mut(&self.owner.session.team_id)
            .ok_or_else(unavailable)?;
        // The core response is tied to this exact request. Success omission is
        // a no-op; a caught current acquisition error clears only this file.
        if !slot.section_titles.apply(&self.read, response)? {
            return Err(unavailable());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
