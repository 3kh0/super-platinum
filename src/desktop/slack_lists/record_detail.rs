//! A record-detail visit grants access to the canonical record's full schema.
//! It is separate from layout authority: a hidden grid/card callback cannot adopt
//! this access, and a closed detail cannot authorize an unsent mutation.
use super::*;
use std::sync::Arc;
use super_platinum_core::slack_lists::Record;
mod hydration;
pub(super) use hydration::ensure as ensure_hydration;
pub(crate) use hydration::native_fixture as hydration_fixture;
mod read_state;
pub(crate) use read_state::track_open;

#[derive(Clone)]
pub(crate) struct RecordDetailView {
    pub open_generation: u64,
    pub generation: u64,
    pub record_id: String,
    pub title: String,
    pub columns: Arc<Vec<ColumnView>>,
    pub row: Arc<RowView>,
    pub unavailable: bool,
    pub parent_return: bool,
    pub return_child: Option<String>,
}
#[derive(Clone)]
pub(super) struct Detail {
    open_generation: u64,
    read_started: bool,
    hydration: hydration::State,
    generation: u64,
    selection_generation: u64,
    revision: u64,
    record_id: String,
    created: Option<Value>,
    creator: Option<Value>,
    columns: Arc<Vec<ColumnView>>,
    row: Arc<RowView>,
    unavailable: bool,
    duplicate_reason: Option<&'static str>,
    parent_return: Option<ParentReturn>,
    return_child: Option<String>,
    subitems: Arc<Vec<subitem::Child>>,
    subitem_reason: Option<&'static str>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditAuthority {
    Layout,
    RecordDetail(u64),
    BoardMove(u64),
}
impl EditAuthority {
    /// A layout collapse after an explicit gesture does not cancel that intent;
    /// selection/revision and canonical preflight checks still apply. Detail
    /// authority, unlike geometry, must remain open through actual dispatch.
    pub(super) fn permits_dispatch(self, view: &View, row: &str, column: &str) -> bool {
        match self {
            Self::Layout => view.detail.is_none(),
            Self::RecordDetail(_) | Self::BoardMove(_) => self.admits(view, row, column),
        }
    }
    pub(super) fn admits(self, view: &View, row: &str, column: &str) -> bool {
        match self {
            Self::Layout => view.detail.is_none() && layout::cell_visible(view, row, column),
            Self::BoardMove(layout) => board_move::admits(view, row, column, layout),
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

/// Read-only comment ownership comes from this exact open canonical item.
/// A hidden grid callback cannot borrow a detail visit or its refreshed root.
pub(super) fn comments_target(
    view: &View,
    generation: u64,
) -> Option<(
    u64,
    super_platinum_core::slack_lists::comments::Target,
    String,
)> {
    let detail = view.detail.as_ref()?;
    if detail.generation != generation
        || detail.unavailable
        || detail.revision != view.revision
        || detail.selection_generation != view.layout.selection_generation
        || view.editor.is_some()
        || view.loading
        || view.stale
        || view.unavailable
    {
        return None;
    }
    let record = canonical(view, detail)?;
    let target = super_platinum_core::slack_lists::comments::Target::capture(
        &view.loaded.as_ref()?.document,
        record,
    )
    .ok()?;
    Some((detail.open_generation, target, detail.row.label.clone()))
}
/// Narrow delete authority: the same open detail and exact canonical record
/// must remain current, even if a saved view hides its underlying row.
pub(super) fn delete_target(view: &View, generation: u64) -> Option<(u64, String, String)> {
    let detail = view.detail.as_ref()?;
    if detail.generation != generation
        || detail.unavailable
        || detail.revision != view.revision
        || detail.selection_generation != view.layout.selection_generation
    {
        return None;
    }
    let record = canonical(view, detail)?;
    Some((
        detail.open_generation,
        record.id.clone(),
        detail.row.label.clone(),
    ))
}
pub(super) fn matches_delete_origin(view: &View, open: u64, record: &str) -> bool {
    view.detail
        .as_ref()
        .is_some_and(|detail| detail.open_generation == open && detail.record_id == record)
}
pub(super) fn duplicate_reason(view: &View) -> Option<&'static str> {
    view.detail
        .as_ref()
        .and_then(|detail| detail.duplicate_reason)
}
fn compute_duplicate_reason(view: &View, record: &str) -> Option<&'static str> {
    let loaded = view.loaded.as_ref()?;
    super_platinum_core::slack_lists::CreationIntent::duplicate_eligible(
        &loaded.document,
        &loaded.records,
        record,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        view.layout.selected.as_deref(),
    )
    .err()
    .map(|error| error.0)
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
                pinned: false,
                size: ColumnView::size(&column.kind),
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
        parent_return: detail.parent_return.is_some(),
        return_child: detail.return_child.clone(),
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
    let duplicate_reason = compute_duplicate_reason(view, &detail.record_id);
    let subitems = subitem::project_children(view, &detail.record_id);
    let subitem_reason = subitem::project_reason(view, &detail.record_id);
    let detail = view.detail.as_mut().unwrap();
    detail.duplicate_reason = duplicate_reason;
    detail.subitems = subitems;
    detail.subitem_reason = subitem_reason;
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
        read_started: false,
        hydration: hydration::State::Idle,
        generation,
        selection_generation: view.layout.selection_generation,
        revision: view.revision,
        record_id: record.id.clone(),
        created: record.raw.get("date_created").cloned(),
        creator: record.raw.get("created_by").cloned(),
        columns: columns(loaded),
        row,
        unavailable: false,
        duplicate_reason: compute_duplicate_reason(view, &record.id),
        parent_return: None,
        return_child: None,
        subitems: subitem::project_children(view, &record.id),
        subitem_reason: subitem::project_reason(view, &record.id),
    };
    let view = shell.slack_lists.view.as_mut().unwrap();
    layout::protect_selection(view);
    view.detail = Some(detail);
    true
}
pub(crate) fn close_record_detail(mut state: Signal<ShellState>, generation: u64) {
    if !return_parent(&mut state.write(), generation) {
        close_shell(&mut state.write(), generation);
    }
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
    delete::close_visit(shell);
    create::duplicate::close_detail(shell);
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
pub(crate) fn toggle_vote(
    mut state: Signal<ShellState>,
    generation: u64,
    column: String,
    intent: super_platinum_core::slack_lists::VoteIntent,
) {
    let result = begin_vote_shell(&mut state.write(), generation, &column, intent);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => report(&mut state.write(), generation, error),
    }
}
pub(super) fn begin_vote_shell(
    shell: &mut ShellState,
    generation: u64,
    column: &str,
    intent: super_platinum_core::slack_lists::VoteIntent,
) -> Result<EditAttempt, &'static str> {
    let (_, row) = current_record(shell, generation).ok_or("Open this item again.")?;
    vote::begin(
        shell,
        &row,
        column,
        EditAuthority::RecordDetail(generation),
        intent,
    )
}
pub(crate) fn toggle_detail_checkbox(
    mut state: Signal<ShellState>,
    generation: u64,
    column: String,
) {
    if completion::is_personal_cell(&state.peek(), &column) {
        let row = current_record(&state.peek(), generation).map(|(_, row)| row);
        if let Some(row) = row {
            completion::toggle(state, row, column, EditAuthority::RecordDetail(generation));
        }
        return;
    }
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

#[derive(Clone)]
struct ParentReturn {
    id: String,
    created: Option<Value>,
    creator: Option<Value>,
    visit: u64,
    selection: u64,
}
pub(super) fn subitem_origin(view: &View, parent: &str) -> Option<(u64, u64)> {
    let d = view.detail.as_ref()?;
    if d.record_id != parent
        || d.unavailable
        || d.revision != view.revision
        || d.selection_generation != view.layout.selection_generation
    {
        return None;
    }
    canonical(view, d)?;
    Some((d.open_generation, d.generation))
}
pub(super) fn subitem_target(view: &View, generation: u64) -> Option<String> {
    let d = view.detail.as_ref()?;
    if d.generation != generation || matches!(d.hydration, hydration::State::Pending(_)) {
        return None;
    }
    subitem_origin(view, &d.record_id)?;
    let row = canonical(view, d)?;
    if row.parent_id().is_some() {
        return None;
    }
    Some(row.id.clone())
}
pub(super) fn navigate_subitem(shell: &mut ShellState, parent: &str, child: &str) -> bool {
    let Some(v) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if !checkbox_visit_current(shell, v.generation)
        || subitem_origin(v, parent).is_none()
        || v.editor.is_some()
        || v.stale
        || v.loading
        || v.unavailable
    {
        return false;
    }
    let Some(l) = v.loaded.as_ref() else {
        return false;
    };
    let Some(p) = l.records.record(parent) else {
        return false;
    };
    if l.records
        .record(child)
        .is_none_or(|r| !available(r) || r.parent_id() != Some(parent))
    {
        return false;
    }
    let target = ParentReturn {
        id: parent.into(),
        created: p.raw.get("date_created").cloned(),
        creator: p.raw.get("created_by").cloned(),
        visit: v.generation,
        selection: v.layout.selection_generation,
    };
    if !install_detail(shell, child) {
        return false;
    }
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .detail
        .as_mut()
        .unwrap()
        .parent_return = Some(target);
    true
}
pub(crate) fn open_subitem(mut state: Signal<ShellState>, generation: u64, child: String) {
    let mut shell = state.write();
    if create::blocks_mutation(&shell)
        || delete::blocks_mutation(&shell)
        || reminder::blocks_mutation(&shell)
        || completion::blocks_mutation(&shell)
        || editor::assignment::blocks_mutation(&shell)
        || rename::blocks_mutation(&shell)
    {
        return;
    }
    let Some(v) = shell.slack_lists.view.as_ref() else {
        return;
    };
    let Some(parent) = subitem_target(v, generation) else {
        return;
    };
    navigate_subitem(&mut shell, &parent, &child);
}
fn return_parent(shell: &mut ShellState, generation: u64) -> bool {
    let Some(v) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if !checkbox_visit_current(shell, v.generation) || v.stale || v.loading || v.editor.is_some() {
        return false;
    }
    let Some(d) = v.detail.as_ref().filter(|d| d.generation == generation) else {
        return false;
    };
    let Some(p) = d.parent_return.clone() else {
        return false;
    };
    if p.visit != v.generation || p.selection != v.layout.selection_generation {
        return false;
    }
    if v.loaded
        .as_ref()
        .and_then(|l| l.records.record(&p.id))
        .is_none_or(|r| {
            !available(r)
                || r.raw.get("date_created") != p.created.as_ref()
                || r.raw.get("created_by") != p.creator.as_ref()
        })
    {
        return false;
    }
    let child = d.record_id.clone();
    if !install_detail(shell, &p.id) {
        return false;
    }
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .detail
        .as_mut()
        .unwrap()
        .return_child = Some(child);
    true
}

pub(super) fn subitem_children(view: &View, parent: &str) -> Option<Arc<Vec<subitem::Child>>> {
    let d = view.detail.as_ref()?;
    (d.record_id == parent).then(|| d.subitems.clone())
}

pub(super) fn subitem_reason(view: &View) -> Option<&'static str> {
    view.detail.as_ref().and_then(|d| d.subitem_reason)
}
