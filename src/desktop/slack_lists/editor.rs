//! Local editor ownership and explicit Save. Drafts never dispatch during typing,
//! cancel, refresh, or readback. In-flight receipts live outside this local draft.
use super::*;
use std::sync::Arc;
pub(crate) mod assignment;
mod channels;
pub(crate) mod date;
mod link;
pub(crate) use link::normalization_slot as link_normalization_slot;
pub(crate) mod entities;
mod users;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorKind {
    BoardMove,
    RichText,
    Number,
    Rating,
    Date,
    Link,
    Select,
    MultiSelect,
    User,
    MultiUser,
    Channel,
    MultiChannel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorChoice {
    pub id: String,
    pub label: String,
}
#[derive(Clone, PartialEq)]
pub(crate) struct EditorView {
    pub generation: u64,
    pub layout_generation: u64,
    pub row_id: String,
    pub column_id: String,
    pub row_label: String,
    pub column_label: String,
    pub kind: EditorKind,
    pub original: Arc<CellEditValue>,
    pub rating: Option<super_platinum_core::slack_lists::RatingOptions>,
    pub link_name_seed: String,
    pub choices: Arc<Vec<EditorChoice>>,
    pub directory: entities::Presentation,
    pub disabled: bool,
    pub saving: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub assignment: Option<assignment::Presentation>,
}
#[derive(Clone)]
pub(super) struct Draft {
    pub(super) generation: u64,
    visit: u64,
    revision: u64,
    pub(super) authority: EditAuthority,
    row_id: String,
    column_id: String,
    document_raw: Arc<Value>,
    record_raw: Arc<Value>,
    column_raw: Arc<Value>,
    date_policy: Option<date::Policy>,
    row_label: String,
    column_label: String,
    kind: EditorKind,
    original: Arc<CellEditValue>,
    rating: Option<super_platinum_core::slack_lists::RatingOptions>,
    link_name_seed: String,
    choices: Arc<Vec<EditorChoice>>,
    directory: entities::State,
    pub(super) attempt: Option<u64>,
    error: Option<String>,
}
pub(super) fn presentation(shell: &ShellState) -> Option<EditorView> {
    let view = shell.slack_lists.view.as_ref()?;
    let draft = view.editor.as_ref()?;
    let mutation = shell.slack_lists.operations.get(&view.id);
    let stale = view.stale
        || view.unavailable
        || view.revision != draft.revision
        || view.generation != draft.visit;
    let date_changed = draft.date_policy.is_some_and(|policy| {
        view.loaded
            .as_ref()
            .is_none_or(|loaded| !policy.current(loaded, &draft.row_id))
    });
    let own_mutation = mutation.filter(|mutation| draft.attempt == Some(mutation.attempt.serial));
    let outcome = own_mutation
        .filter(|mutation| mutation.phase == Phase::Uncertain)
        .map(|_| {
            if view.loading {
                "Checking this save. Do not submit another change."
            } else {
                "This save is unconfirmed. Refresh to check its result before editing again."
            }
            .to_owned()
        })
        .or_else(|| draft.attempt.and_then(|_| view.error.clone()))
        .or_else(|| {
            date_changed
                .then(|| "Your task date settings changed. Cancel and reopen the item.".into())
        });
    let hidden = !draft
        .authority
        .admits(view, &draft.row_id, &draft.column_id);
    let error = outcome.or_else(|| draft.error.clone())
        .or_else(|| stale.then(|| "This cell changed while the editor was open. Cancel and reopen it to review the current value.".into()))
        .or_else(|| hidden.then(||"This cell is hidden in the current view. Return to a view showing it to save your draft.".into()))
        .or_else(|| view.error.clone());
    Some(EditorView {
        generation: draft.generation,
        layout_generation: view.layout.generation,
        row_id: draft.row_id.as_str().into(),
        column_id: draft.column_id.as_str().into(),
        row_label: draft.row_label.clone(),
        column_label: draft.column_label.clone(),
        kind: draft.kind,
        original: draft.original.clone(),
        rating: draft.rating.clone(),
        link_name_seed: draft.link_name_seed.clone(),
        choices: draft.choices.clone(),
        directory: draft.directory.presentation(),
        disabled: stale
            || date_changed
            || hidden
            || view.loading
            || view.retry_at > now()
            || mutation.is_some()
            || assignment::blocks_mutation(shell),
        saving: mutation.is_some_and(|mutation| mutation.phase != Phase::Uncertain),
        stale,
        error,
        assignment: assignment::presentation(shell),
    })
}
pub(crate) fn begin_cell_editor(
    mut state: Signal<ShellState>,
    row: String,
    column: String,
    visit: u64,
    layout_generation: u64,
) {
    let Some(row) = layout::instance_record(&state.peek(), &row, &column, visit, layout_generation)
    else {
        return;
    };
    let result = begin_shell(&mut state.write(), &row, &column, visit);
    if let Err(error) = result
        && checkbox_visit_current(&state.peek(), visit)
    {
        if let Some(view) = state.write().slack_lists.view.as_mut() {
            view.error = Some(error.into());
        }
    }
}
pub(super) fn begin_shell(
    shell: &mut ShellState,
    row: &str,
    column: &str,
    visit: u64,
) -> Result<(), &'static str> {
    begin_with_authority(shell, row, column, visit, EditAuthority::Layout)
}
pub(super) fn begin_with_authority(
    shell: &mut ShellState,
    row: &str,
    column: &str,
    visit: u64,
    authority: EditAuthority,
) -> Result<(), &'static str> {
    poll(shell);
    if !checkbox_visit_current(shell, visit) {
        return Err("Open the list again.");
    }
    let view = shell.slack_lists.view.as_ref().unwrap();
    if view.stale || view.loading || view.unavailable || view.retry_at > now() {
        return Err("Refresh this list before editing it.");
    }
    if rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || !assignment::can_open(shell, row, column)
        || view.editor.is_some()
        || shell.slack_lists.operations.contains_key(&view.id)
    {
        return Err("Finish the current edit before opening another cell.");
    }
    cell_receipts::capacity(shell)?;
    let visible = view
        .all_rows
        .iter()
        .find(|candidate| candidate.id == row)
        .ok_or("This item is no longer visible.")?;
    if !authority.admits(view, row, column) {
        return Err("This cell is no longer visible.");
    }
    let loaded = view.loaded.as_ref().ok_or("Wait for the list to load.")?;
    let column = loaded
        .document
        .column(column)
        .ok_or("This column is no longer available.")?;
    let record = loaded
        .records
        .records
        .iter()
        .find(|record| record.id == row)
        .ok_or("This item is no longer available.")?;
    let board = matches!(authority, EditAuthority::BoardMove(_));
    let kind = if board {
        EditorKind::BoardMove
    } else {
        match column.kind.as_str() {
            "text" | "rich_text" => EditorKind::RichText,
            "number" | "currency" => EditorKind::Number,
            "rating" => EditorKind::Rating,
            "date" | "due_date" | "todo_due_date" => EditorKind::Date,
            "select" | "multi_select" if column.is_multi_select() => EditorKind::MultiSelect,
            "select" => EditorKind::Select,
            "user" | "assignee" | "todo_assignee" if column.is_multi_user() => {
                EditorKind::MultiUser
            }
            "user" | "assignee" | "todo_assignee" => EditorKind::User,
            "channel" if column.is_multi_channel() => EditorKind::MultiChannel,
            "channel" => EditorKind::Channel,
            "link" => EditorKind::Link,
            _ => return Err("This field does not have an editor yet."),
        }
    };
    let original = if board {
        board_move::input(loaded, record, column)
            .ok_or("This lane value cannot be safely moved.")?
    } else if matches!(column.kind.as_str(), "assignee" | "todo_assignee") {
        column
            .task_edit_value(
                record,
                loaded
                    .personal_task_lists
                    .as_ref()
                    .ok_or("Refresh your task preferences before assigning.")?,
            )
            .map_err(|_| "This assignee cannot be safely edited.")?
    } else if date::supports(column) {
        date::input(loaded, record, column)?
    } else {
        column
            .edit_value(
                record,
                &loaded.document.id,
                loaded.personal_task_lists.as_ref(),
            )
            .map_err(|_| "This cell cannot be safely edited.")?
    };
    if !loaded.document.editable(loaded.allow_lists.as_deref()) {
        return Err("This cell cannot be edited. Check your access.");
    }
    let mut choices = if board {
        board_move::choices(column)
    } else {
        Vec::new()
    };
    if matches!(kind, EditorKind::Select | EditorKind::MultiSelect) {
        let mut seen = std::collections::HashSet::new();
        for choice in column
            .raw
            .pointer("/options/choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = choice
                .get("value")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or("This field has an unsupported choice.")?;
            let label = choice
                .get("label")
                .and_then(Value::as_str)
                .ok_or("This field has an unsupported choice.")?;
            if !seen.insert(id) {
                return Err("This field has ambiguous choices.");
            }
            choices.push(EditorChoice {
                id: id.into(),
                label: label.into(),
            });
        }
    }
    if let CellEditValue::Users(ids) = &original {
        choices = users::selected_labels(shell, ids);
    }
    if let CellEditValue::Channels(ids) = &original {
        choices = channels::selected_labels(shell, ids);
    }
    let directory = match kind {
        EditorKind::User | EditorKind::MultiUser => {
            entities::State::new(shell, entities::Kind::Users)
        }
        EditorKind::Channel | EditorKind::MultiChannel => {
            entities::State::new(shell, entities::Kind::Channels)
        }
        _ => entities::State::default(),
    };
    let link_name_seed = link::name_seed(
        &original,
        record
            .fields
            .get(&column.key)
            .and_then(|field| field.get("link")),
    );
    let rating = if kind == EditorKind::Rating {
        Some(
            super_platinum_core::slack_lists::RatingOptions::from_column(column)
                .map_err(|_| "This Rating configuration cannot be safely edited.")?,
        )
    } else {
        None
    };
    let draft = Draft {
        generation: next_id(),
        visit,
        revision: view.revision,
        authority,
        row_id: row.into(),
        column_id: column.id.clone(),
        document_raw: Arc::new(loaded.document.raw.clone()),
        record_raw: Arc::new(record.raw.clone()),
        column_raw: Arc::new(column.raw.clone()),
        date_policy: date::calendar_policy(loaded, record, column)?,
        row_label: visible.label.clone(),
        column_label: column.name.clone(),
        kind,
        original: Arc::new(
            assignment::selected(shell, row, &column.id)
                .map(CellEditValue::Users)
                .unwrap_or(original),
        ),
        rating,
        link_name_seed,
        choices: Arc::new(choices),
        directory,
        attempt: None,
        error: None,
    };
    let view = shell.slack_lists.view.as_mut().unwrap();
    layout::protect_selection(view);
    view.board_move_return = None;
    view.editor = Some(draft);
    view.error = None;
    Ok(())
}
pub(crate) fn save_editor(
    mut state: Signal<ShellState>,
    generation: u64,
    layout_generation: u64,
    value: CellEditValue,
) {
    if state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .is_none_or(|view| view.layout.generation != layout_generation)
    {
        return;
    }
    if assignment::is_assignee(&state.peek(), generation) {
        assignment::start(state, generation, value);
        return;
    }
    let result = begin_save(&mut state.write(), generation, value);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => {
            if let Some(draft) = state
                .write()
                .slack_lists
                .view
                .as_mut()
                .and_then(|view| view.editor.as_mut())
                .filter(|draft| draft.generation == generation)
            {
                draft.error = Some(error.into());
            }
        }
    }
}
pub(super) fn begin_save(
    shell: &mut ShellState,
    generation: u64,
    value: CellEditValue,
) -> Result<EditAttempt, &'static str> {
    begin_save_verified(shell, generation, value, None)
}
fn begin_save_verified(
    shell: &mut ShellState,
    generation: u64,
    value: CellEditValue,
    verified: Option<CellEdit>,
) -> Result<EditAttempt, &'static str> {
    poll(shell);
    if verified.is_none() && assignment::blocks_mutation(shell) {
        return Err("Check the retained assignment before saving another edit.");
    }
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open the editor again.")?;
    let draft = view
        .editor
        .as_ref()
        .filter(|draft| draft.generation == generation)
        .ok_or("Open the editor again.")?;
    if !checkbox_visit_current(shell, draft.visit)
        || view.revision != draft.revision
        || view.stale
        || view.loading
        || view.unavailable
        || view.retry_at > now()
    {
        return Err("This editor is out of date. Cancel and reopen the cell before saving.");
    }
    if rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&view.id)
    {
        return Err("Check the previous change before saving another edit.");
    }
    cell_receipts::capacity(shell)?;
    if !draft
        .authority
        .admits(view, &draft.row_id, &draft.column_id)
    {
        return Err("This item is no longer visible. Reopen it before saving.");
    }
    let loaded = view.loaded.as_ref().ok_or("Wait for the list to load.")?;
    if loaded.document.raw != *draft.document_raw
        || loaded
            .document
            .column(&draft.column_id)
            .is_none_or(|column| column.raw != *draft.column_raw)
        || loaded
            .records
            .records
            .iter()
            .find(|row| row.id == draft.row_id)
            .is_none_or(|row| row.raw != *draft.record_raw)
    {
        return Err("This cell or your access changed. Reopen it before saving.");
    }
    if draft
        .date_policy
        .is_some_and(|policy| !policy.current(loaded, &draft.row_id))
    {
        return Err("Your task date settings changed. Cancel and reopen the item.");
    }
    let edit = if let Some(edit) = verified {
        edit.revalidate(
            &loaded.document,
            &loaded.records,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .map_err(|_| "The checked assignment is no longer current.")?;
        edit
    } else if matches!(draft.authority, EditAuthority::BoardMove(_)) {
        board_move::capture(loaded, &draft.row_id, &draft.column_id, value)?
    } else if draft.kind == EditorKind::Date {
        date::capture(loaded, &draft.row_id, &draft.column_id, value)?
    } else {
        CellEdit::capture(
            &loaded.document,
            &loaded.records,
            draft.row_id.as_str(),
            draft.column_id.as_str(),
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
            value,
        )
        .map_err(|_| "This value cannot be saved. Check the value and choices.")?
    };
    let attempt = EditAttempt {
        selection_generation: view.layout.selection_generation,
        authority: draft.authority,
        date_policy: draft.date_policy,
        scope: shell
            .slack_lists
            .scope
            .clone()
            .ok_or("Open the list again.")?,
        serial: next_id(),
        generation: view.generation,
        revision: view.revision,
        edit,
    };
    shell.slack_lists.operations.insert(
        view.id.clone(),
        Mutation {
            attempt: attempt.clone(),
            phase: Phase::Preparing,
        },
    );
    let draft = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .editor
        .as_mut()
        .unwrap();
    draft.attempt = Some(attempt.serial);
    draft.error = None;
    Ok(attempt)
}
pub(crate) fn cancel_editor(mut state: Signal<ShellState>, generation: u64) {
    cancel_shell(&mut state.write(), generation);
}
pub(super) fn cancel_shell(shell: &mut ShellState, generation: u64) -> bool {
    if shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.editor.as_ref())
        .is_some_and(|d| d.generation == generation)
    {
        assignment::cancel(shell);
    }
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return false;
    };
    let Some(draft) = view
        .editor
        .as_ref()
        .filter(|draft| draft.generation == generation)
    else {
        return false;
    };
    if shell
        .slack_lists
        .operations
        .get(&view.id)
        .is_some_and(|mutation| {
            mutation.phase == Phase::Preparing && draft.attempt == Some(mutation.attempt.serial)
        })
    {
        shell.slack_lists.operations.remove(&view.id);
    }
    view.editor = None;
    true
}

#[cfg(test)]
mod tests;
