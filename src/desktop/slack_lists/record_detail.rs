//! A record-detail visit grants access to the canonical record's full schema.
//! It is separate from layout authority: a hidden grid/card callback cannot adopt
//! this access, and a closed detail cannot authorize an unsent mutation.
use super::*;
use std::sync::Arc;
use super_platinum_core::slack_lists::Record;

#[derive(Clone)]
pub(crate) struct RecordDetailView {
    pub open_generation: u64,
    pub generation: u64,
    pub record_id: String,
    pub title: String,
    pub columns: Arc<Vec<ColumnView>>,
    pub row: Arc<RowView>,
    pub unavailable: bool,
}
#[derive(Clone)]
pub(super) struct Detail {
    open_generation: u64,
    generation: u64,
    selection_generation: u64,
    revision: u64,
    record_id: String,
    created: Option<Value>,
    creator: Option<Value>,
    columns: Arc<Vec<ColumnView>>,
    row: Arc<RowView>,
    unavailable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditAuthority {
    Layout,
    RecordDetail(u64),
}
impl EditAuthority {
    /// A layout collapse after an explicit gesture does not cancel that intent;
    /// selection/revision and canonical preflight checks still apply. Detail
    /// authority, unlike geometry, must remain open through actual dispatch.
    pub(super) fn permits_dispatch(self, view: &View, row: &str, column: &str) -> bool {
        match self {
            Self::Layout => view.detail.is_none(),
            Self::RecordDetail(_) => self.admits(view, row, column),
        }
    }
    pub(super) fn admits(self, view: &View, row: &str, column: &str) -> bool {
        match self {
            Self::Layout => view.detail.is_none() && layout::cell_visible(view, row, column),
            Self::RecordDetail(generation) => view.detail.as_ref().is_some_and(|detail| {
                detail.generation == generation
                    && detail.record_id == row
                    && detail.selection_generation == view.layout.selection_generation
                    && detail.revision == view.revision
                    && !detail.unavailable
                    && canonical(view, detail).is_some()
                    && view
                        .loaded
                        .as_ref()
                        .is_some_and(|loaded| loaded.document.column(column).is_some())
            }),
        }
    }
}
fn available(record: &Record) -> bool {
    !record.archived()
        && record
            .raw
            .get("is_deleted")
            .is_none_or(|value| value.is_null() || value == &Value::Bool(false))
}
fn canonical<'a>(view: &'a View, detail: &Detail) -> Option<&'a Record> {
    let record = view.loaded.as_ref()?.records.record(&detail.record_id)?;
    (available(record)
        && record.raw.get("date_created") == detail.created.as_ref()
        && record.raw.get("created_by") == detail.creator.as_ref())
    .then_some(record)
}
fn columns(loaded: &Loaded) -> Arc<Vec<ColumnView>> {
    Arc::new(
        loaded
            .document
            .columns
            .iter()
            .map(|column| ColumnView {
                id: column.id.clone(),
                label: column.name.clone(),
                primary: loaded
                    .document
                    .primary_column()
                    .is_some_and(|primary| primary.id == column.id),
            })
            .collect(),
    )
}
pub(super) fn presentation(view: &View) -> Option<RecordDetailView> {
    let detail = view.detail.as_ref()?;
    let unavailable = detail.unavailable
        || view.unavailable
        || detail.selection_generation != view.layout.selection_generation;
    // Canonical reads and label events reconcile the cached Arc. Rendering the
    // detail does not scan the loaded record bag or reparse rich cell content.
    let row = detail.row.clone();
    Some(RecordDetailView {
        open_generation: detail.open_generation,
        generation: detail.generation,
        record_id: detail.record_id.clone(),
        title: row.label.clone(),
        columns: detail.columns.clone(),
        row,
        unavailable,
    })
}
pub(super) fn reconcile(view: &mut View) {
    let Some(detail) = view.detail.as_ref() else {
        return;
    };
    if detail.unavailable {
        return;
    }
    let row = canonical(view, detail)
        .and_then(|_| view.all_rows.iter().find(|row| row.id == detail.record_id))
        .cloned();
    let identity_current = detail.selection_generation == view.layout.selection_generation;
    let columns = (detail.revision != view.revision)
        .then(|| view.loaded.as_ref().map(columns))
        .flatten();
    let detail = view.detail.as_mut().unwrap();
    if !identity_current || row.is_none() {
        detail.unavailable = true;
        detail.generation = next_id();
        return;
    }
    // A fresh canonical read invalidates queued detail callbacks, even when the
    // same row survived. An open editor retains its old authority and draft.
    if detail.revision != view.revision {
        detail.generation = next_id();
        detail.revision = view.revision;
        detail.columns = columns.unwrap();
    }
    detail.row = row.unwrap();
}
pub(crate) fn open_record_detail(
    mut state: Signal<ShellState>,
    instance: String,
    visit: u64,
    layout: u64,
) {
    open_shell(&mut state.write(), &instance, visit, layout);
}
pub(super) fn open_shell(shell: &mut ShellState, instance: &str, visit: u64, layout: u64) -> bool {
    poll(shell);
    if !layout::current(shell, visit, layout) {
        return false;
    }
    let view = shell.slack_lists.view.as_ref().unwrap();
    if view.detail.is_some()
        || view.editor.is_some()
        || view.stale
        || view.loading
        || view.unavailable
        || shell
            .slack_lists
            .operations
            .get(&view.id)
            .is_some_and(|mutation| mutation.phase == Phase::Preparing)
    {
        return false;
    }
    let Some(visible) = view.rows.iter().find(|row| row.instance_id == instance) else {
        return false;
    };
    let record_id = visible.id.clone();
    install_detail(shell, &record_id)
}
/// Only the create receipt reconciler may open a newly read canonical row that
/// the current saved view filters out. Ordinary row gestures still need a
/// visible instance above. This does not change the selected saved view.
pub(super) fn open_created(shell: &mut ShellState, record_id: &str) -> bool {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if !checkbox_visit_current(shell, view.generation)
        || view.stale
        || view.loading
        || view.unavailable
        || view.editor.is_some()
        || view.detail.is_some()
    {
        return false;
    }
    install_detail(shell, record_id)
}
fn install_detail(shell: &mut ShellState, record_id: &str) -> bool {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    let Some(record) = loaded
        .records
        .record(record_id)
        .filter(|record| available(record))
    else {
        return false;
    };
    let Some(row) = view
        .all_rows
        .iter()
        .find(|row| row.id == record.id)
        .cloned()
    else {
        return false;
    };
    let generation = next_id();
    let detail = Detail {
        open_generation: generation,
        generation,
        selection_generation: view.layout.selection_generation,
        revision: view.revision,
        record_id: record.id.clone(),
        created: record.raw.get("date_created").cloned(),
        creator: record.raw.get("created_by").cloned(),
        columns: columns(loaded),
        row,
        unavailable: false,
    };
    let view = shell.slack_lists.view.as_mut().unwrap();
    layout::protect_selection(view);
    view.detail = Some(detail);
    true
}
pub(crate) fn close_record_detail(mut state: Signal<ShellState>, generation: u64) {
    close_shell(&mut state.write(), generation);
}
pub(super) fn close_shell(shell: &mut ShellState, generation: u64) -> bool {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if !checkbox_visit_current(shell, view.generation)
        || view
            .detail
            .as_ref()
            .is_none_or(|detail| detail.generation != generation)
    {
        return false;
    }
    dismiss_shell(shell);
    true
}
/// Back, list close/reopen and saved-view selection revoke only local authority.
/// Sent/uncertain receipts are retained independently until guarded readback.
pub(super) fn dismiss_shell(shell: &mut ShellState) {
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return;
    };
    if view.detail.take().is_none() {
        return;
    }
    if view
        .editor
        .as_ref()
        .is_some_and(|draft| matches!(draft.authority, EditAuthority::RecordDetail(_)))
    {
        view.editor = None;
    }
    if shell
        .slack_lists
        .operations
        .get(&view.id)
        .is_some_and(|mutation| {
            mutation.phase == Phase::Preparing
                && matches!(mutation.attempt.authority, EditAuthority::RecordDetail(_))
        })
    {
        shell.slack_lists.operations.remove(&view.id);
    }
}
fn current_record(shell: &ShellState, generation: u64) -> Option<(u64, String)> {
    let view = shell.slack_lists.view.as_ref()?;
    if !checkbox_visit_current(shell, view.generation) {
        return None;
    }
    let detail = view
        .detail
        .as_ref()
        .filter(|detail| detail.generation == generation)?;
    Some((view.generation, detail.record_id.clone()))
}
pub(crate) fn begin_detail_cell_editor(
    mut state: Signal<ShellState>,
    generation: u64,
    column: String,
) {
    let result = begin_editor_shell(&mut state.write(), generation, &column);
    if let Err(error) = result {
        report(&mut state.write(), generation, error);
    }
}
pub(super) fn begin_editor_shell(
    shell: &mut ShellState,
    generation: u64,
    column: &str,
) -> Result<(), &'static str> {
    let (visit, row) = current_record(shell, generation).ok_or("Open this item again.")?;
    editor::begin_with_authority(
        shell,
        &row,
        column,
        visit,
        EditAuthority::RecordDetail(generation),
    )
}
pub(crate) fn toggle_detail_checkbox(
    mut state: Signal<ShellState>,
    generation: u64,
    column: String,
) {
    let result = begin_checkbox_shell(&mut state.write(), generation, &column);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => report(&mut state.write(), generation, error),
    }
}
pub(super) fn begin_checkbox_shell(
    shell: &mut ShellState,
    generation: u64,
    column: &str,
) -> Result<EditAttempt, &'static str> {
    let (_, row) = current_record(shell, generation).ok_or("Open this item again.")?;
    begin_edit_with_authority(shell, &row, column, EditAuthority::RecordDetail(generation))
}
fn report(shell: &mut ShellState, generation: u64, error: &str) {
    if current_record(shell, generation).is_some() {
        shell.slack_lists.view.as_mut().unwrap().error = Some(error.into());
    }
}

#[cfg(test)]
mod tests;
