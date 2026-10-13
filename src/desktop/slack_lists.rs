//! Scoped native Lists reads and one-attempt checkbox writes. An ambiguous write
//! remains locked until a fresh read observes its cell; it is never replayed.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
use super_platinum_core::{
    slack::{Error, models::File},
    slack_lists::{CellEdit, CheckboxEdit, Document, PersonalTaskLists, RecordsPage},
};
mod cell_receipts;
pub(crate) mod check;
pub(crate) mod create;
pub(crate) mod date_context;
pub(crate) mod delete;
mod edit_trace;
mod editor;
pub(crate) mod events;
pub(crate) mod fixture;
pub(crate) mod labels;
mod layout;
pub(crate) mod lifecycle;
mod mutations;
mod network;
use mutations::{
    begin_edit, begin_edit_with_authority, cancel_preflight, checkbox_visit_current, finish_edit,
    validate_edit,
};
mod read_state;
mod record_detail;
pub(crate) use editor::entities;
pub(crate) use editor::{
    EditorKind, EditorView, begin_cell_editor, cancel_editor, link_normalization_slot, save_editor,
};
pub(crate) use layout::{TableEntry, select_view, toggle_group};
use record_detail::EditAuthority;
pub(crate) use record_detail::{
    RecordDetailView, begin_detail_cell_editor, close_record_detail, open_record_detail,
    toggle_detail_checkbox, track_open,
};
pub(crate) use super_platinum_core::slack_lists::{CellEditValue, saved_views::ProjectionLayout};
#[cfg(test)]
mod numeric_tests;
mod presentation;
mod scope;
#[cfg(test)]
mod tests;
pub(crate) use presentation::{
    CellContent, CellView, ColumnView, Presentation, RowView, locale_changed, presentation,
};
use scope::Scope;

#[derive(Clone, Default)]
pub(crate) struct State {
    scope: Option<Scope>,
    view: Option<View>,
    operations: BTreeMap<String, Mutation>,
    retained_cells: cell_receipts::Retained,
    viewing: lifecycle::State,
    create: create::State,
    delete: delete::State,
    pub(crate) fixture: Option<fixture::Fixture>,
}
#[derive(Clone)]
struct View {
    id: String,
    title: String,
    generation: u64,
    layout: layout::Layout,
    read: u64,
    revision: u64,
    loaded: Option<Loaded>,
    all_rows: std::sync::Arc<Vec<std::sync::Arc<presentation::RowView>>>,
    rows: std::sync::Arc<Vec<std::sync::Arc<presentation::RowView>>>,
    expanded: std::collections::BTreeSet<String>,
    label_rows: labels::Index,
    editor: Option<editor::Draft>,
    detail: Option<record_detail::Detail>,
    loading: bool,
    stale: bool,
    unavailable: bool,
    error: Option<String>,
    retry_at: i64,
}
#[derive(Clone)]
pub(super) struct Loaded {
    pub document: Document,
    pub records: RecordsPage,
    pub allow_lists: Option<String>,
    pub personal_task_lists: Option<PersonalTaskLists>,
    pub list_experiments: Option<super_platinum_core::slack::api::ListExperiments>,
}
#[derive(Clone)]
pub(super) struct ReadAttempt {
    scope: Scope,
    pub id: String,
    generation: u64,
    read: u64,
    revision: u64,
}
#[derive(Clone)]
pub(super) struct EditAttempt {
    scope: Scope,
    selection_generation: u64,
    authority: EditAuthority,
    serial: u64,
    generation: u64,
    revision: u64,
    pub edit: CellEdit,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Saving,
    Uncertain,
}
#[derive(Clone)]
struct Mutation {
    attempt: EditAttempt,
    phase: Phase,
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn now() -> i64 {
    super_platinum_core::state::now_secs()
}

pub(crate) fn open(mut state: Signal<ShellState>, file: File) {
    if !open_shell(&mut state.write(), file) {
        return;
    }
    refresh(state);
}
pub(super) fn open_shell(shell: &mut ShellState, file: File) -> bool {
    poll(shell);
    let Some(id) = file.id.as_ref().filter(|id| !id.is_empty()).cloned() else {
        return false;
    };
    let Some(scope) = Scope::capture(shell) else {
        return false;
    };
    if !cell_receipts::restore(shell, &scope) {
        return false;
    }
    let unavailable = scope.deleted(shell, &id)
        || super_platinum_core::file_actions::deleted(&file)
        || !super_platinum_core::slack_lists::is_list(&file);
    create::close_visit(shell);
    delete::close_visit(shell);
    record_detail::dismiss_shell(shell);
    shell.slack_lists.scope = Some(scope);
    shell.slack_lists.view = Some(View {
        id,
        title: super_platinum_core::state::file_title(&file),
        generation: next_id(),
        layout: Default::default(),
        read: 0,
        revision: 0,
        loaded: None,
        all_rows: Default::default(),
        rows: Default::default(),
        expanded: Default::default(),
        label_rows: Default::default(),
        editor: None,
        detail: None,
        loading: false,
        stale: true,
        unavailable,
        error: unavailable.then(|| "This list is no longer available.".into()),
        retry_at: 0,
    });
    shell.overlay = Some(Overlay::SlackList);
    true
}
pub(crate) fn close_shell(shell: &mut ShellState) {
    create::close_visit(shell);
    delete::close_visit(shell);
    record_detail::dismiss_shell(shell);
    // In-flight mutations retain their ownership and receipt independently of
    // the dialog visit; reopening cannot repeat an uncertain request.
    shell.slack_lists.view = None;
    if shell.overlay == Some(Overlay::SlackList) {
        shell.overlay = None;
    }
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let creation_changed = create::poll(shell);
    let deletion_changed = delete::poll(shell);
    if shell
        .slack_lists
        .scope
        .as_ref()
        .is_some_and(|scope| !scope.current(shell))
    {
        shell.slack_lists.view = None;
        cell_receipts::retire(shell);
        shell.slack_lists.scope = None;
        if shell.overlay == Some(Overlay::SlackList) {
            shell.overlay = None;
        }
        return true;
    }
    let deleted = shell
        .slack_lists
        .scope
        .as_ref()
        .zip(shell.slack_lists.view.as_ref())
        .is_some_and(|(scope, view)| scope.deleted(shell, &view.id) && !view.unavailable);
    if deleted {
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.unavailable = true;
        view.stale = true;
        view.loading = false;
        view.revision = next_id();
        view.error = Some("This list is no longer available.".into());
    }
    let expired = shell
        .slack_lists
        .view
        .as_ref()
        .is_some_and(|v| v.retry_at > 0 && v.retry_at <= now());
    if expired {
        shell.slack_lists.view.as_mut().unwrap().retry_at = 0;
    }
    let calendar_changed = date_context::poll(shell);
    creation_changed || deletion_changed || deleted || expired || calendar_changed
}
/// A verified realtime event names the file/list. Its payload is not trusted as
/// a complete document or cell response: invalidate and require a fresh read.
pub(crate) fn invalidate(shell: &mut ShellState, team: &str, id: &str) -> bool {
    if shell
        .slack_lists
        .scope
        .as_ref()
        .is_none_or(|scope| scope.team != team || !scope.current(shell))
    {
        return false;
    }
    let Some(view) = shell.slack_lists.view.as_mut().filter(|v| v.id == id) else {
        return false;
    };
    view.stale = true;
    view.revision = next_id();
    // Keep an active read as the single worker. Its completion will discard
    // this obsolete revision and request one replacement for the newest state.
    view.error = Some("This list changed. Refresh to see the latest values.".into());
    true
}

/// Socket/outage recovery must supersede an older in-flight read instead of
/// silently skipping it because the dialog still says it is loading.
pub(crate) fn invalidate_visible_for_reconnect(shell: &mut ShellState) -> bool {
    poll(shell);
    let Some(view) = shell
        .slack_lists
        .view
        .as_mut()
        .filter(|view| !view.unavailable)
    else {
        return false;
    };
    view.stale = true;
    view.revision = next_id();
    view.loading = false;
    view.error = None;
    true
}
pub(super) fn begin_read(shell: &mut ShellState) -> Option<ReadAttempt> {
    poll(shell);
    let scope = shell.slack_lists.scope.clone()?;
    let view = shell.slack_lists.view.as_ref()?;
    if view.loading
        || view.unavailable
        || view.retry_at > now()
        || delete::blocks_read(shell)
        || shell
            .slack_lists
            .operations
            .get(&view.id)
            .is_some_and(|m| m.phase != Phase::Uncertain)
    {
        return None;
    }
    let view = shell.slack_lists.view.as_mut()?;
    view.read = next_id();
    view.loading = true;
    view.error = None;
    Some(ReadAttempt {
        scope,
        id: view.id.clone(),
        generation: view.generation,
        read: view.read,
        revision: view.revision,
    })
}
impl ReadAttempt {
    fn owns_read(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && !self.scope.deleted(shell, &self.id)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.id == self.id
                    && v.generation == self.generation
                    && v.read == self.read
                    && !v.unavailable
            })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owns_read(shell)
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|view| view.revision == self.revision)
    }
}
/// True asks the network worker for one replacement read after a coalesced
/// realtime invalidation. A different visit/account/read never schedules work.
pub(super) fn finish_read(
    shell: &mut ShellState,
    attempt: &ReadAttempt,
    result: Result<Loaded, Error>,
) -> bool {
    if !attempt.owns_read(shell) {
        return false;
    }
    if !attempt.current(shell) {
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.loading = false;
        view.read = 0;
        if let Some(error) = result
            .as_ref()
            .err()
            .filter(|error| retry_at(error) > now())
        {
            read_failure(shell, error.clone());
            return false;
        }
        return true;
    }
    // Consume the receipt once, including errors. A duplicate completion cannot
    // apply data or start another replacement after the first completion.
    shell.slack_lists.view.as_mut().unwrap().read = 0;
    match result {
        Ok(loaded) if loaded.document.id == attempt.id => {
            let mut notice = None;
            let resolved=shell.slack_lists.operations.get(&attempt.id).filter(|m|m.phase==Phase::Uncertain).is_some_and(|mutation| {
                let edit=&mutation.attempt.edit;
                if edit.readback_matches(&loaded.document,&loaded.records) {true}
                else if edit.readback_value(&loaded.document,&loaded.records).is_some() {
                    notice=Some("Slack currently shows a different value. Review it before changing it again.".into());true
                } else {false}
            });
            if resolved {
                let mutation = shell.slack_lists.operations.remove(&attempt.id).unwrap();
                if mutation
                    .attempt
                    .edit
                    .readback_matches(&loaded.document, &loaded.records)
                    && let Some(view) = shell.slack_lists.view.as_mut()
                    && view
                        .editor
                        .as_ref()
                        .is_some_and(|draft| draft.attempt == Some(mutation.attempt.serial))
                {
                    view.editor = None;
                }
            }
            let view = shell.slack_lists.view.as_mut().unwrap();
            view.title = loaded.document.title.clone();
            view.loaded = Some(loaded);
            view.loading = false;
            view.stale = false;
            view.error = notice;
            view.revision = next_id();
            rebuild_rows(shell);
            create::reconcile(shell);
            // Page absence never settles deletion; this only retires a stale
            // unsent confirmation/preflight after the canonical read changed.
            delete::poll(shell);
        }
        Ok(_) => read_failure(shell, Error::Api("list_id_mismatch".into())),
        Err(error) => read_failure(shell, error),
    }
    false
}
fn read_failure(shell: &mut ShellState, error: Error) {
    if let Some(view) = shell.slack_lists.view.as_mut() {
        view.loading = false;
        view.stale = true;
        view.retry_at = retry_at(&error);
        view.error = Some(
            if view.loaded.is_some() {
                "Could not refresh this list. Previously loaded values are still shown."
            } else {
                "Could not load this list. Refresh to try again."
            }
            .into(),
        );
    }
}
fn retry_at(error: &Error) -> i64 {
    match error {
        Error::RateLimited {
            retry_after_secs: Some(seconds),
        }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(seconds),
        } => now().saturating_add(i64::try_from(*seconds).unwrap_or(i64::MAX)),
        _ => 0,
    }
}
fn outcome_uncertain(error: &Error) -> bool {
    match error {
        Error::Api(code) => {
            matches!(
                code.as_str(),
                "internal_error" | "fatal_error" | "service_unavailable"
            ) || code.starts_with("decode_")
        }
        Error::RateLimited { .. } | Error::TransportNotConfigured | Error::MissingWorkspace(_) => {
            false
        }
        Error::HttpStatus { status, .. } => *status >= 500 || *status == 408,
        _ => true,
    }
}
pub(crate) use network::{refresh, toggle_checkbox};

fn rebuild_rows(shell: &mut ShellState) {
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|scope| shell.core.workspaces.get(&scope.team));
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return;
    };
    view.all_rows = std::sync::Arc::new(
        view.loaded
            .as_ref()
            .map(|loaded| presentation::project_rows(loaded, workspace, &shell.media))
            .unwrap_or_default(),
    );
    view.label_rows = labels::Index::build(view.loaded.as_ref(), &view.all_rows, workspace);
    let parents = view
        .all_rows
        .iter()
        .filter(|row| row.has_children)
        .map(|row| row.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    view.expanded.retain(|id| parents.contains(id.as_str()));
    drop(parents);
    layout::rebuild(view, workspace);
}
fn rebuild_visible(view: &mut View) {
    let mut visible = Vec::new();
    let mut index = 0;
    while let Some(row) = view.all_rows.get(index) {
        if row.has_children && view.expanded.contains(&row.id) {
            let mut expanded = (**row).clone();
            expanded.expanded = true;
            visible.push(std::sync::Arc::new(expanded));
            index += 1;
        } else {
            visible.push(row.clone());
            index = if row.has_children {
                row.subtree_end
            } else {
                index + 1
            };
        }
    }
    view.rows = std::sync::Arc::new(visible);
    layout::fallback_entries(view);
}
pub(crate) fn toggle_expanded(mut state: Signal<ShellState>, row: String, generation: u64) {
    toggle_expanded_shell(&mut state.write(), &row, generation);
}
pub(super) fn toggle_expanded_shell(shell: &mut ShellState, row: &str, generation: u64) -> bool {
    poll(shell);
    let Some(view) = shell.slack_lists.view.as_mut().filter(|view| {
        view.generation == generation && view.layout.selected.is_none() && view.detail.is_none()
    }) else {
        return false;
    };
    if !view
        .rows
        .iter()
        .any(|candidate| candidate.id == row && candidate.has_children)
    {
        return false;
    }
    if !view.expanded.remove(row) {
        view.expanded.insert(row.into());
    }
    layout::protect_selection(view);
    view.layout.generation = next_id();
    rebuild_visible(view);
    true
}
pub(crate) fn generation(shell: &ShellState) -> Option<u64> {
    shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|scope| scope.current(shell))?;
    Some(shell.slack_lists.view.as_ref()?.generation)
}
