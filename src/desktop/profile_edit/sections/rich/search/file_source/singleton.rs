//! One actual files.info receipt, independent of autocomplete query bookkeeping.
use super::*;

#[cfg(test)]
mod tests;

/// Source authority only. The consumer must separately retain its original
/// occurrence/visit and cancel the request when that authority retires.
/// Concurrent collection publication retires this fence; recapture before a
/// subsequent request instead of attaching an old response to a new source.
pub(crate) struct FileInfoRead {
    owner: Arc<Context>,
    id: String,
    state: Arc<()>,
    slot: Arc<()>,
    collection: source::Collection,
    revision: u64,
    events: u64,
}

impl FileInfoRead {
    pub(crate) fn capture(shell: &mut ShellState, team: &str, id: &str) -> Result<Self, Error> {
        // Same bounded file identity as the core source store. This admission
        // does not derive identity from a token URL, title or typed File.
        if !(2..=255).contains(&id.len())
            || !id.starts_with('F')
            || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(unavailable());
        }
        let owner = Arc::new(Context::capture(shell, team)?);
        State::ensure_source(shell, &owner);
        let state = &shell.profile_edit.sections.file_search;
        let slot = state.slots.get(team).ok_or_else(unavailable)?;
        let read = Self {
            owner,
            id: id.into(),
            state: state.identity.clone(),
            slot: slot.identity.clone(),
            collection: slot.store.candidates(),
            revision: slot.revision,
            events: crate::file_events::snapshot_revision(shell),
        };
        read.current(shell).then_some(read).ok_or_else(unavailable)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let state = &shell.profile_edit.sections.file_search;
        let team = &self.owner.session.team_id;
        self.owner.current(shell)
            && Arc::ptr_eq(&state.identity, &self.state)
            && state
                .owner
                .as_ref()
                .is_some_and(|o| o.account_current(shell))
            && state.slots.get(team).is_some_and(|slot| {
                Arc::ptr_eq(&self.slot, &slot.identity)
                    && self.revision == slot.revision
                    && self.collection.current(&slot.store)
            })
            && !crate::file_events::changed_since(shell, team, &self.id, self.events)
            && shell
                .core
                .workspaces
                .get(team)
                .is_some_and(|w| !w.deleted_files.contains(&self.id))
    }

    /// Consumes this read, retaining the exact original singleton envelope.
    /// Malformed/error/wrong-ID/stale responses do not mutate. A valid but
    /// unsupported source update *does* install the core's poisoned history,
    /// then returns unavailable; an older title must not remain authoritative.
    /// No search prefixes, exact queries or completion receipts are created.
    pub(crate) fn publish(
        self,
        shell: &mut ShellState,
        received: &source::OrderedFileInfo,
    ) -> Result<(), Error> {
        let envelope = received.raw();
        if !self.current(shell)
            || envelope["ok"] != true
            || envelope.get("error").is_some_and(|v| !v.is_null())
            || !envelope["file"].is_object()
            || envelope["file"]["id"].as_str() != Some(self.id.as_str())
        {
            return Err(unavailable());
        }
        let team = &self.owner.session.team_id;
        let slot = shell
            .profile_edit
            .sections
            .file_search
            .slots
            .get_mut(team)
            .ok_or_else(unavailable)?;
        let mut next = slot.store.clone();
        let outcome = next.ingest_info(team, received);
        if self.collection.current(&next) {
            // Core rejected before any original row was installed.
            return outcome.and_then(|_| Err(unavailable()));
        }
        slot.store = next;
        slot.invalidated.remove(&self.id);
        slot.stamps.insert(
            self.id.clone(),
            Stamp {
                owner: self.owner.clone(),
                events: self.events,
            },
        );
        outcome?;
        let published = Snapshot::under(shell, self.owner, None)?;
        if published.rows().iter().any(|row| row.id() == self.id) {
            published.title(&self.id)?;
            Ok(())
        } else {
            Err(unavailable())
        }
    }
}
