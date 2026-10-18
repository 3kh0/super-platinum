//! Personal primary To-dos expose the saved-record reminder action separately
//! from calendar editing. No route here creates or reinterprets a cell draft.
use super::*;

pub(crate) fn reminder_for(shell: &ShellState, row: &str, column: &str) -> bool {
    let Some(loaded) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
    else {
        return false;
    };
    let Some(record) = loaded.records.record(row) else {
        return false;
    };
    let Some(column) = loaded.document.column(column) else {
        return false;
    };
    mode(loaded, record, column) == Ok(Mode::PersonalReminder)
}
pub(crate) fn reminder_reason(shell: &ShellState) -> Option<&'static str> {
    if assignment::blocks_mutation(shell) {
        return Some("Check the retained assignment before opening a reminder.");
    }
    crate::slack_lists::reminder::reason(shell)
}
fn cell_target(
    shell: &mut ShellState,
    instance: &str,
    column: &str,
    visit: u64,
    layout_generation: u64,
) -> Option<u64> {
    let row = layout::instance_record(shell, instance, column, visit, layout_generation)?;
    if !reminder_for(shell, &row, column)
        || reminder_reason(shell).is_some()
        || shell.slack_lists.view.as_ref()?.detail.is_some()
        || shell.slack_lists.view.as_ref()?.editor.is_some()
    {
        return None;
    }
    if !record_detail::open_shell(shell, instance, visit, layout_generation) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let detail = record_detail::presentation(view)?;
    let (_, exact_row, _) = record_detail::delete_target(view, detail.generation)?;
    (exact_row == row).then_some(detail.generation)
}
fn detail_target(shell: &ShellState, generation: u64, column: &str) -> Option<u64> {
    let view = shell.slack_lists.view.as_ref()?;
    if !checkbox_visit_current(shell, view.generation)
        || view.editor.is_some()
        || reminder_reason(shell).is_some()
    {
        return None;
    }
    let (_, row, _) = record_detail::delete_target(view, generation)?;
    reminder_for(shell, &row, column).then_some(generation)
}
pub(crate) fn open_reminder(
    mut state: Signal<ShellState>,
    instance: String,
    column: String,
    visit: u64,
    layout_generation: u64,
) {
    let target = cell_target(
        &mut state.write(),
        &instance,
        &column,
        visit,
        layout_generation,
    );
    if let Some(target) = target {
        crate::slack_lists::reminder::open(state, target);
    }
}
pub(crate) fn open_detail_reminder(state: Signal<ShellState>, generation: u64, column: String) {
    let target = detail_target(&state.peek(), generation, &column);
    if let Some(target) = target {
        crate::slack_lists::reminder::open(state, target);
    }
}

#[cfg(test)]
mod tests;
