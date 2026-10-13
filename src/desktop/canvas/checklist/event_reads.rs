//! Coalesced visible-view read refreshes driven by file revisions, never writes.
use super::*;

pub(in crate::canvas) fn needed(shell: &ShellState) -> Option<u64> {
    let view = shell.canvas_checklist.view.as_ref()?;
    (view_current(shell, view)
        // A reopened view may have deferred its first load while a retained
        // write was still Saving. Admit that initial pair once it settles even
        // if no file event arrives. Any claimed read consumes this admission.
        && (view.read == 0 || view.revision != view.event_read_revision)
        && !view.loading
        && !view.event_reads_blocked
        && operation_index(shell, &view.file).is_none_or(|index| {
            !shell.canvas_checklist.operations[index]
                .stage
                .blocks_refresh()
        }))
    .then_some(view.generation)
}

pub(in crate::canvas) fn claim(shell: &mut ShellState, visit: u64) -> Option<Read> {
    (needed(shell) == Some(visit)).then_some(())?;
    begin_read(shell)
}

/// Even an event-invalidated response can report a 429/offline failure. Do not
/// turn subsequent events into retries; an explicit refresh/reopen may recover.
pub(in crate::canvas) fn block_after_error(shell: &mut ShellState, read: &Read) {
    if !read.scope.current(shell) {
        return;
    }
    if let Some(view) = shell.canvas_checklist.view.as_mut().filter(|view| {
        view.generation == read.visit && view.file == read.file && view.read == read.serial
    }) {
        view.event_reads_blocked = true;
    }
}
