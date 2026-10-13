//! Record-specific native Undo. No mutation is replayed and no fields are sent.
use super::*;

#[derive(Clone)]
pub(super) struct Capability {
    serial: u64,
    pub(super) deleted: bool,
    intent: Arc<DeletionIntent>,
}
pub(super) fn retain(shell: &mut ShellState, attempt: &Attempt) {
    // Completed Undo capabilities may be retired; unresolved operation receipts
    // are separately bounded and are never evicted.
    if shell.slack_lists.delete.undo.len() >= MAX_TOTAL
        && let Some(oldest) = shell
            .slack_lists
            .delete
            .undo
            .iter()
            .min_by_key(|(_, cap)| cap.serial)
            .map(|(key, _)| key.clone())
    {
        shell.slack_lists.delete.undo.remove(&oldest);
    }
    shell.slack_lists.delete.undo.insert(
        attempt.key(),
        Capability {
            serial: next_id(),
            intent: attempt.intent.clone(),
            deleted: attempt.restoring && !attempt.redo,
        },
    );
}
pub(super) fn available(shell: &ShellState) -> Option<u64> {
    let key = key(shell)?;
    if blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&key.1)
    {
        return None;
    }
    if shell.slack_lists.delete.receipts.len() >= MAX_TOTAL
        || shell
            .slack_lists
            .delete
            .receipts
            .keys()
            .filter(|(owner, _)| owner == &key.0)
            .count()
            >= MAX_PER_OWNER
    {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    if shell.overlay != Some(Overlay::SlackList)
        || view.loading
        || view.stale
        || view.unavailable
        || view.retry_at > now()
        || view.editor.is_some()
    {
        return None;
    }
    shell
        .slack_lists
        .delete
        .undo
        .get(&key)
        .map(|cap| cap.serial)
}
pub(crate) fn undo_delete(mut state: Signal<ShellState>, serial: u64, visit: u64) {
    if !callback_current(&state.peek(), serial, visit) {
        return;
    }
    let attempt = begin(&mut state.write(), serial);
    if let Some(attempt) = attempt {
        network::dispatch(state, attempt);
    }
}
pub(crate) fn dismiss_undo(mut state: Signal<ShellState>, serial: u64, visit: u64) {
    let mut shell = state.write();
    if !callback_current(&shell, serial, visit) {
        return;
    }
    let Some(key) = key(&shell) else {
        return;
    };
    if shell
        .slack_lists
        .delete
        .undo
        .get(&key)
        .is_some_and(|cap| cap.serial == serial)
    {
        shell.slack_lists.delete.undo.remove(&key);
        shell.slack_lists.delete.notice = None;
    }
}
fn callback_current(shell: &ShellState, serial: u64, visit: u64) -> bool {
    checkbox_visit_current(shell, visit)
        && key(shell)
            .and_then(|key| shell.slack_lists.delete.undo.get(&key))
            .is_some_and(|cap| cap.serial == serial)
}
pub(super) fn begin(shell: &mut ShellState, serial: u64) -> Option<Attempt> {
    if available(shell) != Some(serial) {
        return None;
    }
    let key = key(shell)?;
    let cap = shell.slack_lists.delete.undo.get(&key)?;
    let view = shell.slack_lists.view.as_ref()?;
    let attempt = Attempt {
        scope: shell.slack_lists.scope.clone()?,
        serial: next_id(),
        visit: view.generation,
        revision: view.revision,
        detail_generation: 0,
        detail_open: record_detail::presentation(view)
            .filter(|detail| detail.record_id == cap.intent.record_id())
            .map_or(0, |detail| detail.open_generation),
        intent: cap.intent.clone(),
        restoring: true,
        redo: cap.deleted,
    };
    shell.slack_lists.delete.receipts.insert(
        key,
        Receipt {
            attempt: attempt.clone(),
            stage: Stage::Preparing,
            retry_at: 0,
            read: None,
            message: None,
            response_seen: false,
            restore_intent: None,
        },
    );
    Some(attempt)
}
pub(super) fn validate_restore(
    shell: &mut ShellState,
    attempt: &Attempt,
    loaded: &Loaded,
    readback: &ListRecordReadback,
) -> Result<Arc<DeletionStateIntent>, &'static str> {
    if !preflight_current(shell, attempt)
        || create::blocks_mutation(shell)
        || shell
            .slack_lists
            .operations
            .contains_key(attempt.intent.list_id())
    {
        return Err("This Undo is out of date. Review the list before trying again.");
    }
    let intent = attempt
        .intent
        .deletion_state_intent(
            &loaded.document,
            &loaded.records,
            readback,
            attempt.redo,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .map_err(|error| error.0)?;
    let intent = Arc::new(intent);
    let receipt = shell
        .slack_lists
        .delete
        .receipts
        .get_mut(&attempt.key())
        .unwrap();
    receipt.restore_intent = Some(intent.clone());
    receipt.stage = Stage::Sending;
    Ok(intent)
}
pub(super) fn preflight_current(shell: &ShellState, attempt: &Attempt) -> bool {
    attempt.restoring
        && attempt.owned(shell)
        && attempt.visible(shell)
        && shell
            .slack_lists
            .delete
            .receipts
            .get(&attempt.key())
            .is_some_and(|receipt| receipt.stage == Stage::Preparing)
}

#[cfg(test)]
mod tests;
