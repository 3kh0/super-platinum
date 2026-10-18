//! Synchronously reserve local-write custody before scheduling its async task.
//! This performs no I/O; the owned mutex guard stops reopen reads from overtaking
//! the task, and the exact captured token authorizes only its private write.
use super::*;

pub(super) struct Reservation {
    id: u64,
    token: Token,
    record: protocol::custody::Record,
    _guard: tokio::sync::OwnedMutexGuard<()>,
}
pub(super) fn owns(entry: &Entry, token: &Token) -> bool {
    entry
        .queued
        .as_ref()
        .is_some_and(|(_, owner)| owner == token)
}
pub(super) fn reserve(
    shell: &mut ShellState,
    token: &Token,
    record: protocol::custody::Record,
) -> Option<Reservation> {
    if !token.current(shell) {
        return None;
    }
    let entry = shell
        .profile_edit
        .recording
        .entries
        .entry(token.owner.clone())
        .or_default();
    if entry.storing || entry.inflight.is_some() || entry.queued.is_some() {
        return None;
    }
    let guard = entry.io_gate.clone().try_lock_owned().ok()?;
    let id = next_id();
    entry.queued = Some((id, token.clone()));
    entry.retained = Some(record.clone());
    Some(Reservation {
        id,
        token: token.clone(),
        record,
        _guard: guard,
    })
}
pub(super) async fn persist(mut state: Signal<ShellState>, reservation: Reservation) -> bool {
    let result = storage::persist_locked(state, &reservation.token, reservation.record).await;
    if let Some(entry) = state
        .write()
        .profile_edit
        .recording
        .entries
        .get_mut(&reservation.token.owner)
        .filter(|e| {
            e.queued
                .as_ref()
                .is_some_and(|(id, token)| *id == reservation.id && *token == reservation.token)
        })
    {
        entry.queued = None;
    }
    result
}
