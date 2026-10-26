//! One current metadata authority for existing tokens. Live credential-owned
//! callers use the shared collection; disconnected pure fixtures retain their
//! existing per-visit source without manufacturing shared authority.
use super::super::search::file_source::reference::{Reference, ReferenceRead};
use super::*;

#[derive(Clone)]
pub(super) struct Metadata {
    pub(super) row: source::ReferenceObservation,
    pub(super) socket: u64,
    pub(super) revision: u64,
    shared: Option<Reference>,
    stamp: Option<Arc<()>>,
}
impl Metadata {
    pub(super) fn capture(shell: &ShellState, token: &Token, id: &str) -> Result<Self, Error> {
        let generation = socket(shell, token).ok_or_else(unavailable)?;
        if crate::directory_member_fetches::Context::capture(shell, &token.owner.team).is_ok() {
            let shared = Reference::capture(shell, &token.owner.team, id)?;
            return Ok(Self {
                row: shared.row().clone(),
                socket: generation,
                revision: crate::file_events::snapshot_revision(shell),
                shared: Some(shared),
                stamp: None,
            });
        }
        // No live/shared source authority is inferred from this fixture-only path.
        if !crate::fixture::is_fixture() && !cfg!(test) {
            return Err(unavailable());
        }
        let cache = shell
            .profile_edit
            .sections
            .file_titles
            .as_ref()
            .ok_or_else(unavailable)?;
        let (socket, revision) = *cache.reads.get(id).ok_or_else(unavailable)?;
        let observed = Self {
            row: cache.store.reference(id)?,
            socket,
            revision,
            shared: None,
            stamp: cache.changes.get(id).cloned(),
        };
        observed
            .current(shell, token)
            .then_some(observed)
            .ok_or_else(unavailable)
    }
    pub(super) fn current(&self, shell: &ShellState, token: &Token) -> bool {
        if socket(shell, token) != Some(self.socket) {
            return false;
        }
        if let Some(shared) = &self.shared {
            return shared.current(shell);
        }
        let id = self.row.id();
        !crate::file_events::changed_since(shell, &token.owner.team, id, self.revision)
            && shell
                .core
                .workspaces
                .get(&token.owner.team)
                .is_some_and(|w| !w.deleted_files.contains(id))
            && shell
                .profile_edit
                .sections
                .file_titles
                .as_ref()
                .is_some_and(|cache| {
                    cache.token == *token
                        && cache.reads.get(id) == Some(&(self.socket, self.revision))
                        && self.row.current(&cache.store)
                        && self.stamp.as_ref().is_some_and(|stamp| {
                            cache.changes.get(id).is_some_and(|s| Arc::ptr_eq(s, stamp))
                        })
                })
    }
}

pub(super) fn read(
    shell: &mut ShellState,
    token: &Token,
    id: &str,
) -> Result<Option<ReferenceRead>, Error> {
    if !token.current(shell) {
        return Err(unavailable());
    }
    if crate::directory_member_fetches::Context::capture(shell, &token.owner.team).is_ok() {
        ReferenceRead::capture(shell, &token.owner.team, id).map(Some)
    } else if crate::fixture::is_fixture() || cfg!(test) {
        Ok(None)
    } else {
        Err(unavailable())
    }
}
