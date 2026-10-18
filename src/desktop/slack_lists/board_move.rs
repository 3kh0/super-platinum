//! Renderer-owned authority for one top-level card's grouping cell only.
//! No drag position, lane ordering, hidden-cell override or separate executor.
use super::*;
use super_platinum_core::slack_lists::{Column, Record, ViewGrouping, ViewRename};

fn column(view: &View) -> Option<&Column> {
    if view.layout.kind != ProjectionLayout::Board
        || view.layout.notice.is_some()
        || view.detail.is_some()
    {
        return None;
    }
    let loaded = view.loaded.as_ref()?;
    let id = view.layout.selected.as_deref()?;
    let mut views = loaded.document.views.iter().filter(|v| v["id"] == id);
    let raw = views.next()?;
    if views.next().is_some() || raw["type"] != "board" || !ViewRename::supports(raw) {
        return None;
    }
    let key = raw
        .pointer("/grouping/group_by")?
        .as_str()
        .filter(|key| !key.is_empty())?;
    let mut columns = loaded.document.columns.iter().filter(|c| c.key == key);
    let column = columns.next()?;
    if columns.next().is_some()
        || loaded
            .document
            .columns
            .iter()
            .filter(|c| c.id == column.id)
            .count()
            != 1
        || !ViewGrouping::supports_column(column)
    {
        return None;
    }
    if raw
        .pointer("/grouping/group_by_column_id")
        .is_some_and(|id| id.as_str() != Some(column.id.as_str()))
    {
        return None;
    }
    Some(column)
}
fn ordinary(record: &Record) -> bool {
    record
        .raw
        .get("parent_record_id")
        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
        && ["archived", "is_deleted", "is_suggested", "suggested"]
            .iter()
            .all(|key| {
                record
                    .raw
                    .get(*key)
                    .is_none_or(|v| v.is_null() || v == &Value::Bool(false))
            })
}
pub(super) fn input(loaded: &Loaded, record: &Record, column: &Column) -> Option<CellEditValue> {
    if !ordinary(record)
        || !ViewGrouping::supports_column(column)
        || loaded
            .personal_task_lists
            .as_ref()
            .is_none_or(|p| p.contains(&loaded.document.id))
        || record.fields.iter().any(|(key, field)| {
            key != &column.key
                && field
                    .get("column_id")
                    .is_some_and(|id| id.as_str() == Some(column.id.as_str()))
        })
    {
        return None;
    }
    if column.kind == "checkbox" {
        column
            .checkbox_input_value_with_policy(
                record,
                &loaded.document.id,
                loaded.personal_task_lists.as_ref(),
            )
            .map(CellEditValue::Boolean)
    } else {
        match column
            .edit_value(
                record,
                &loaded.document.id,
                loaded.personal_task_lists.as_ref(),
            )
            .ok()?
        {
            value @ CellEditValue::Select(_) => Some(value),
            _ => None,
        }
    }
}
pub(crate) fn key(value: &CellEditValue) -> Option<&str> {
    match value {
        CellEditValue::Boolean(value) => Some(if *value { "true" } else { "" }),
        CellEditValue::Select(v) if v.len() <= 1 => Some(v.first().map_or("", String::as_str)),
        _ => None,
    }
}
pub(super) fn choices(column: &Column) -> Vec<editor::EditorChoice> {
    let mut out = vec![editor::EditorChoice {
        id: String::new(),
        label: if column.kind == "checkbox" {
            "Unchecked"
        } else {
            "Ungrouped"
        }
        .into(),
    }];
    if column.kind == "checkbox" {
        out.push(editor::EditorChoice {
            id: "true".into(),
            label: "Checked".into(),
        });
    } else {
        for c in column
            .raw
            .pointer("/options/choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(id), Some(label)) = (c["value"].as_str(), c["label"].as_str()) {
                out.push(editor::EditorChoice {
                    id: id.into(),
                    label: label.into(),
                });
            }
        }
    }
    out
}
pub(super) fn admits(view: &View, row: &str, column_id: &str, layout: u64) -> bool {
    if view.layout.generation != layout {
        return false;
    }
    let Some(column) = column(view).filter(|c| c.id == column_id) else {
        return false;
    };
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    let Some(record) = loaded.records.record(row) else {
        return false;
    };
    let Some(value) = input(loaded, record, column) else {
        return false;
    };
    let Some(key) = key(&value) else {
        return false;
    };
    let mut found = view.layout.lanes.iter().flat_map(|lane| {
        lane.rows
            .iter()
            .filter(move |r| r.id == row)
            .map(move |r| (lane, r))
    });
    found
        .next()
        .is_some_and(|(lane, r)| lane.key == key && r.depth == 0 && r.parent_id.is_none())
        && found.next().is_none()
}
pub(crate) fn supported(shell: &ShellState) -> bool {
    shell.slack_lists.view.as_ref().and_then(column).is_some()
}
pub(crate) fn can_open(shell: &ShellState, instance: &str) -> bool {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if !checkbox_visit_current(shell, view.generation)
        || view.loading
        || view.stale
        || view.unavailable
        || view.retry_at > now()
        || view.editor.is_some()
        || rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&view.id)
        || cell_receipts::capacity(shell).is_err()
    {
        return false;
    }
    let Some(column) = column(view) else {
        return false;
    };
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    loaded.document.editable(loaded.allow_lists.as_deref())
        && view
            .layout
            .lanes
            .iter()
            .flat_map(|l| l.rows.iter())
            .find(|r| r.instance_id == instance)
            .is_some_and(|r| admits(view, &r.id, &column.id, view.layout.generation))
}
pub(crate) fn open(mut state: Signal<ShellState>, instance: String, visit: u64, layout: u64) {
    let _ = open_shell(&mut state.write(), &instance, visit, layout);
}
pub(super) fn open_shell(
    shell: &mut ShellState,
    instance: &str,
    visit: u64,
    layout: u64,
) -> Result<(), &'static str> {
    poll(shell);
    if !can_open(shell, instance) {
        return Err("This card cannot be moved in the current Board.");
    }
    let view = shell.slack_lists.view.as_ref().unwrap();
    if view.generation != visit || view.layout.generation != layout {
        return Err("This Board changed. Open the card action again.");
    }
    let column = column(view).unwrap().id.clone();
    let row = view
        .layout
        .lanes
        .iter()
        .flat_map(|l| l.rows.iter())
        .find(|r| r.instance_id == instance)
        .unwrap()
        .id
        .clone();
    editor::begin_with_authority(
        shell,
        &row,
        &column,
        visit,
        EditAuthority::BoardMove(layout),
    )
}
pub(super) fn capture(
    loaded: &Loaded,
    row: &str,
    column_id: &str,
    value: CellEditValue,
) -> Result<CellEdit, &'static str> {
    let column = loaded
        .document
        .column(column_id)
        .ok_or("The grouping field changed.")?;
    let record = loaded
        .records
        .record(row)
        .ok_or("The item is unavailable.")?;
    let original =
        input(loaded, record, column).ok_or("This lane value cannot be safely moved.")?;
    if original == value || key(&value).is_none() {
        return Err("Choose a different destination lane.");
    }
    match (&original, value) {
        (CellEditValue::Boolean(_), CellEditValue::Boolean(value)) => {
            CheckboxEdit::capture_with_policy(
                &loaded.document,
                &loaded.records,
                row,
                column_id,
                loaded.allow_lists.as_deref(),
                loaded.personal_task_lists.as_ref(),
                value,
            )
            .map(Into::into)
        }
        (CellEditValue::Select(_), value @ CellEditValue::Select(_)) => CellEdit::capture(
            &loaded.document,
            &loaded.records,
            row,
            column_id,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
            value,
        ),
        _ => return Err("Choose a supported destination lane."),
    }
    .map_err(|_| "This destination cannot be saved. Refresh and check your access.")
}
pub(crate) fn destination(original: &CellEditValue, id: &str) -> Option<CellEditValue> {
    match original {
        CellEditValue::Boolean(_) if id.is_empty() || id == "true" => {
            Some(CellEditValue::Boolean(id == "true"))
        }
        CellEditValue::Select(_) => Some(CellEditValue::Select(if id.is_empty() {
            vec![]
        } else {
            vec![id.into()]
        })),
        _ => None,
    }
}

/// One pending focus return, never an authority to change data or reveal pages.
#[derive(Clone, PartialEq, serde::Serialize)]
pub(crate) struct Return {
    pub serial: u64,
    pub editor: u64,
    pub visit: u64,
    pub selection: u64,
    pub layout: u64,
    revision: u64,
    pub row: String,
    pub instance: Option<String>,
    pub fallback: Option<bool>,
}
pub(super) struct PendingReturn {
    editor: u64,
    visit: u64,
    selection: u64,
    row: String,
    column: String,
    key: String,
}
pub(super) fn pending_return(shell: &ShellState, loaded: &Loaded) -> Option<PendingReturn> {
    let view = shell.slack_lists.view.as_ref()?;
    let mutation = shell.slack_lists.operations.get(&view.id)?;
    if mutation.phase != Phase::Uncertain
        || !matches!(mutation.attempt.authority, EditAuthority::BoardMove(_))
        || !mutation
            .attempt
            .edit
            .readback_matches(&loaded.document, &loaded.records)
    {
        return None;
    }
    let draft = view
        .editor
        .as_ref()
        .filter(|d| d.attempt == Some(mutation.attempt.serial))?;
    Some(PendingReturn {
        editor: draft.generation,
        visit: mutation.attempt.generation,
        selection: mutation.attempt.selection_generation,
        row: mutation.attempt.edit.record_id().into(),
        column: mutation.attempt.edit.column_id().into(),
        key: key(&mutation.attempt.edit.value())?.into(),
    })
}
pub(super) fn observed(shell: &mut ShellState, pending: Option<PendingReturn>) {
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return;
    };
    view.board_move_return = None;
    let Some(p) = pending else {
        return;
    };
    if view.generation != p.visit
        || view.layout.selection_generation != p.selection
        || view.editor.is_some()
    {
        return;
    }
    let instance = column(view)
        .filter(|c| c.id == p.column)
        .and_then(|_| view.layout.lanes.iter().find(|l| l.key == p.key))
        .and_then(|l| l.rows.iter().find(|r| r.id == p.row))
        .map(|r| r.instance_id.clone());
    view.board_move_return = Some(Return {
        serial: next_id(),
        editor: p.editor,
        visit: p.visit,
        selection: p.selection,
        layout: view.layout.generation,
        revision: view.revision,
        row: p.row,
        instance,
        fallback: None,
    });
}
pub(crate) fn returned(shell: &ShellState) -> Option<Return> {
    let view = shell.slack_lists.view.as_ref()?;
    let r = view.board_move_return.as_ref()?;
    (checkbox_visit_current(shell, r.visit)
        && view.editor.is_none()
        && view.layout.kind == ProjectionLayout::Board
        && r.selection == view.layout.selection_generation
        && r.layout == view.layout.generation
        && r.revision == view.revision)
        .then(|| r.clone())
}
pub(crate) fn focus_result(mut state: Signal<ShellState>, serial: u64, fallback: bool) {
    if returned(&state.peek()).is_some_and(|r| r.serial == serial) {
        if let Some(r) = state
            .write()
            .slack_lists
            .view
            .as_mut()
            .and_then(|v| v.board_move_return.as_mut())
        {
            r.fallback = Some(fallback);
        }
    }
}
#[cfg(test)]
mod tests;
