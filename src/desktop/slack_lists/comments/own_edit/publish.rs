use super::*;
use super_platinum_core::slack_lists::comments::OwnEditAccepted;

/// The original HTTP receipt is immutable. Only confirmed body fields reach
/// typed rows; current reactions are retained by the core accepted projector.
pub(super) fn accepted(shell: &mut ShellState, binding: &Binding, value: &OwnEditAccepted) -> bool {
    if !binding.current(shell) {
        return false;
    }
    let Some(e) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    let (Some(page), Some(projection)) = (&e.page, &e.projection) else {
        return false;
    };
    let ts = value.record().ts();
    if e.deleted.contains(ts) || super::super::accepted::deleted(page, ts) {
        return false;
    }
    let mut page = page.clone();
    let mut tail = e.accepted.clone();
    let mut live = e.live.clone();
    if !page.apply_own_edit(value) {
        return false;
    }
    for row in tail
        .iter_mut()
        .chain(live.iter_mut())
        .filter(|m| m.ts.as_deref() == Some(ts))
    {
        let Some(next) = value.apply(row) else {
            return false;
        };
        *row = next;
    }
    let Some(rows) =
        super::super::accepted::rows_with_live(shell, &e.owner, &page, projection, &tail, &live)
    else {
        return false;
    };
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    e.page = Some(page);
    e.accepted = tail;
    e.live = live;
    e.rows = rows;
    if e.loading && e.read_replaces_page {
        e.read = next_id();
        e.loading = false;
    }
    true
}

/// Preflight never publishes its Page and cannot retire a successor read.
pub(super) fn finish_read(
    shell: &mut ShellState,
    owner: &Owner,
    serial: u64,
    error: Option<&Error>,
) {
    if !owner.reading(shell, serial) {
        return;
    }
    let deadline = error.map_or(0, retry_deadline).max(cooldown(shell));
    if deadline > now() {
        shell.slack_lists.comments.cooldown = Some((owner.scope.clone(), deadline));
    }
    let e = shell.slack_lists.comments.entry.as_mut().unwrap();
    e.loading = false;
    e.read = next_id();
    e.retry_at = e.retry_at.max(deadline);
}
