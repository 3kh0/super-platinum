//! Existing-cell mutation ownership, preflight and one-attempt receipts.
use super::*;

fn observed_checkbox(loaded: &Loaded, row: &str, column: &str) -> Option<bool> {
    let column = loaded.document.columns.iter().find(|c| c.id == column)?;
    let row = loaded.records.records.iter().find(|r| r.id == row)?;
    match column.cell(row) {
        super_platinum_core::slack_lists::CellValue::Boolean(value) => Some(value),
        _ => None,
    }
}
// UI gestures carry their rendered visit. Never adopt a delayed event into a
// reopened list merely because the new list also contains the same row IDs.
pub(super) fn checkbox_visit_current(shell: &ShellState, generation: u64) -> bool {
    shell.overlay == Some(Overlay::SlackList)
        && shell
            .slack_lists
            .scope
            .as_ref()
            .is_some_and(|scope| scope.current(shell))
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|view| view.generation == generation)
}
pub(super) fn begin_edit(
    shell: &mut ShellState,
    row: &str,
    column: &str,
) -> Result<EditAttempt, &'static str> {
    begin_edit_with_authority(shell, row, column, EditAuthority::Layout)
}
pub(super) fn begin_edit_with_authority(
    shell: &mut ShellState,
    row: &str,
    column: &str,
    authority: EditAuthority,
) -> Result<EditAttempt, &'static str> {
    poll(shell);
    let scope = shell
        .slack_lists
        .scope
        .clone()
        .ok_or("Open the list again.")?;
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open the list again.")?;
    if view.stale || view.loading || view.unavailable || view.retry_at > now() {
        return Err("Refresh this list before changing it.");
    }
    if create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&view.id)
        || view.editor.is_some()
    {
        return Err("Finish the current edit before changing another checkbox.");
    }
    cell_receipts::capacity(shell)?;
    if shell.overlay != Some(Overlay::SlackList) {
        return Err("This item is no longer visible. Open it again before changing it.");
    }
    let loaded = view.loaded.as_ref().ok_or("Wait for the list to load.")?;
    if !authority.admits(view, row, column) {
        return Err("This cell is no longer visible.");
    }
    let checked =
        observed_checkbox(loaded, row, column).ok_or("This checkbox is no longer available.")?;
    let edit = CheckboxEdit::capture_with_policy(
        &loaded.document,
        &loaded.records,
        row,
        column,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        !checked,
    )
    .map_err(|_| "This checkbox cannot be changed. Refresh and check your access.")?;
    let attempt = EditAttempt {
        selection_generation: view.layout.selection_generation,
        authority,
        scope,
        serial: next_id(),
        generation: view.generation,
        revision: view.revision,
        edit: edit.into(),
    };
    let id = view.id.clone();
    layout::protect_selection(shell.slack_lists.view.as_mut().unwrap());
    shell.slack_lists.operations.insert(
        id,
        Mutation {
            attempt: attempt.clone(),
            phase: Phase::Preparing,
        },
    );
    shell.slack_lists.view.as_mut().unwrap().error = None;
    Ok(attempt)
}
impl EditAttempt {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell
                .slack_lists
                .operations
                .get(self.edit.list_id())
                .is_some_and(|m| m.attempt.serial == self.serial)
    }
    pub(super) fn visible(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| v.id == self.edit.list_id() && v.generation == self.generation)
    }
}
pub(super) fn validate_edit(
    shell: &mut ShellState,
    attempt: &EditAttempt,
    loaded: Loaded,
) -> Result<(), &'static str> {
    if !attempt.visible(shell)
        || delete::blocks_mutation(shell)
        || shell.overlay != Some(Overlay::SlackList)
        || attempt.scope.deleted(shell, attempt.edit.list_id())
        || shell.slack_lists.view.as_ref().is_none_or(|v| {
            v.revision != attempt.revision
                || v.unavailable
                || v.stale
                || v.layout.selection_generation != attempt.selection_generation
                || !attempt.authority.permits_dispatch(
                    v,
                    attempt.edit.record_id(),
                    attempt.edit.column_id(),
                )
        })
        || shell
            .slack_lists
            .operations
            .get(attempt.edit.list_id())
            .is_none_or(|m| m.phase != Phase::Preparing)
    {
        return Err("The list changed before this cell could be saved.");
    }
    attempt
        .edit
        .revalidate(
            &loaded.document,
            &loaded.records,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .map_err(|_| {
            edit_trace::mismatch(shell, attempt, &loaded);
            "The cell or your access changed. Refresh before trying again."
        })?;
    shell
        .slack_lists
        .operations
        .get_mut(attempt.edit.list_id())
        .unwrap()
        .phase = Phase::Saving;
    Ok(())
}
pub(super) fn finish_edit(
    shell: &mut ShellState,
    attempt: &EditAttempt,
    result: Result<Value, Error>,
) {
    if cell_receipts::get(shell, attempt).is_none_or(|m| m.phase != Phase::Saving) {
        return;
    }
    let visible = attempt.visible(shell);
    let result = result.and_then(|response| {
        attempt
            .edit
            .validate_response(&response)
            .map_err(|_| Error::Transport("Unconfirmed cell response".into()))
            .map(|_| response)
    });
    let uncertain = result.as_ref().is_ok() || result.as_ref().err().is_some_and(outcome_uncertain);
    cell_receipts::settle(shell, attempt, uncertain);
    // A late response can settle its own retained receipt, but not write UI
    // state into a replacement account, credentials, or dialog visit.
    if !visible {
        return;
    }
    if let Some(view) = shell
        .slack_lists
        .view
        .as_mut()
        .filter(|v| v.id == attempt.edit.list_id())
    {
        view.stale = true;
        view.retry_at = result.as_ref().err().map(retry_at).unwrap_or(0);
        view.error = Some(
            if uncertain {
                "Refresh to check this cell change before editing again."
            } else {
                "Slack could not save this cell. Refresh before trying again."
            }
            .into(),
        );
    }
}
pub(super) fn cancel_preflight(shell: &mut ShellState, attempt: &EditAttempt, error: &str) {
    if !attempt.current(shell) {
        return;
    }
    shell.slack_lists.operations.remove(attempt.edit.list_id());
    if let Some(view) = shell
        .slack_lists
        .view
        .as_mut()
        .filter(|v| v.id == attempt.edit.list_id())
    {
        view.stale = true;
        view.error = Some(error.into());
    }
}
