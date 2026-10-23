//! Reproject a mounted page when original shared emoji assignments arrive.
use super::*;

/// Called at the shared initialization publication boundary. Only cached visible
/// rows change: no history request, editor remount, or draft/journal mutation.
pub(crate) fn changed(shell: &mut ShellState, team: &str) {
    if !shell.slack_lists.comments.open {
        return;
    }
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return;
    };
    if entry.owner.scope.team != team || !entry.owner.active(shell) {
        return;
    }
    let (Some(page), Some(projection)) = (&entry.page, &entry.projection) else {
        return;
    };
    let Some(rows) = accepted::rows_with_live(
        shell,
        &entry.owner,
        page,
        projection,
        &entry.accepted,
        &entry.live,
    ) else {
        return;
    };
    // The original owner/page check and publication share this serial borrow.
    shell.slack_lists.comments.entry.as_mut().unwrap().rows = rows;
}
