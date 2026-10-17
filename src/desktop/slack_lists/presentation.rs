use super::*;
mod content;
mod timestamps;
pub(crate) use content::{CellContent, PeopleReference};
use std::sync::Arc;
use super_platinum_core::{slack_lists::ParentRelation, state::Workspace};
pub(super) use timestamps::project_timestamps;

pub(crate) struct Presentation {
    pub generation: u64,
    pub layout_generation: u64,
    pub selected_view: Option<String>,
    pub find_input: String,
    pub find_applied: String,
    pub find_revision: u64,
    pub grouping_revision: u64,
    pub find_available: bool,
    pub view_choices: Arc<Vec<layout::ViewChoice>>,
    pub view_notice: Option<String>,
    pub entries: Arc<Vec<layout::TableEntry>>,
    pub layout_kind: super_platinum_core::slack_lists::saved_views::ProjectionLayout,
    pub lanes: Arc<Vec<layout::BoardLane>>,
    pub completion_column: Option<String>,
    pub matched_rows: usize,
    pub title: String,
    pub list_id: String,
    pub columns: Arc<Vec<ColumnView>>,
    pub rows: Arc<Vec<Arc<RowView>>>,
    pub total_rows: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub incomplete: bool,
    pub read_only_reason: Option<String>,
    pub stale: bool,
    pub mutation: Option<MutationView>,
    pub editor: Option<EditorView>,
    pub board_move_return: Option<board_move::Return>,
    pub detail: Option<RecordDetailView>,
}
pub(crate) struct MutationView {
    pub record_id: String,
    pub column_id: String,
    pub uncertain: bool,
}
#[derive(Clone)]
pub(crate) struct ColumnView {
    pub id: String,
    pub label: String,
    pub primary: bool,
    pub pinned: bool,
    pub size: &'static str,
}
impl ColumnView {
    pub(super) fn size(kind: &str) -> &'static str {
        match kind {
            "checkbox" | "completed" | "todo_completed" => "checkbox",
            "number" | "currency" => "number",
            "rating" => "rating",
            "vote" => "number",
            _ => "standard",
        }
    }
}
#[derive(Clone)]
pub(crate) struct RowView {
    pub id: String,
    pub instance_id: String,
    pub label: String,
    pub cells: Arc<Vec<CellView>>,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
    pub parent_id: Option<String>,
    pub parent_notice: Option<String>,
    pub subtree_end: usize,
}
#[derive(Clone)]
pub(crate) struct CellView {
    pub column_id: String,
    pub text: String,
    pub checkbox: Option<bool>,
    pub editable: bool,
    pub can_edit: bool,
    pub content: CellContent,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))?;
    let view = shell.slack_lists.view.as_ref()?;
    let unavailable = view.unavailable || scope.deleted(shell, &view.id);
    let read_only_reason = if unavailable {
        Some("This list is no longer available.".into())
    } else {
        view.loaded.as_ref().and_then(|loaded| {
            loaded
                .document
                .read_only_reason(loaded.allow_lists.as_deref())
                .map(str::to_owned)
        })
    };
    Some(Presentation {
        editor: editor::presentation(shell),
        board_move_return: board_move::returned(shell),
        detail: record_detail::presentation(view),
        generation: view.generation,
        layout_generation: view.layout.generation,
        selected_view: view.layout.selected.clone(),
        find_input: view.layout.find.input.clone(),
        find_applied: view.layout.find.applied.clone(),
        find_revision: view.layout.find.revision,
        grouping_revision: view.layout.grouping_revision,
        find_available: find::available(view),
        view_choices: view.layout.choices.clone(),
        view_notice: view.layout.notice.clone(),
        entries: view.layout.entries.clone(),
        layout_kind: view.layout.kind,
        lanes: view.layout.lanes.clone(),
        completion_column: view.layout.completion_column.clone(),
        matched_rows: view.layout.matched,
        title: view.title.clone(),
        list_id: view.id.clone(),
        columns: view.layout.columns.clone(),
        rows: view.rows.clone(),
        total_rows: view.all_rows.len(),
        loading: view.loading,
        error: view
            .error
            .clone()
            .or_else(|| cell_receipts::notice(shell, &view.id)),
        incomplete: view
            .loaded
            .as_ref()
            .is_some_and(|loaded| !loaded.records.complete()),
        read_only_reason,
        stale: view.stale || unavailable,
        mutation: shell
            .slack_lists
            .operations
            .get(&view.id)
            .map(|mutation| MutationView {
                record_id: mutation.attempt.edit.record_id().into(),
                column_id: mutation.attempt.edit.column_id().into(),
                uncertain: mutation.phase == Phase::Uncertain,
            }),
    })
}

/// Locale changes affect cached labels, never canonical cells or edit drafts.
pub(crate) fn locale_changed(shell: &mut ShellState, team: &str) {
    if shell
        .slack_lists
        .scope
        .as_ref()
        .is_some_and(|scope| scope.team == team && scope.current(shell))
    {
        super::rebuild_rows(shell);
    }
}
pub(super) fn project_rows(
    loaded: &Loaded,
    workspace: Option<&Workspace>,
    media: &crate::media::MediaRegistry,
) -> Vec<Arc<RowView>> {
    loaded
        .records
        .hierarchy()
        .into_iter()
        .map(|entry| {
            let row = entry.record;
            let (cells, label) = project_cells(loaded, row, workspace, media);
            let parent_notice = match entry.relation {
                ParentRelation::Root | ParentRelation::Child(_) => None,
                ParentRelation::Missing(_) => Some("Parent item is not loaded."),
                ParentRelation::Archived(_) => Some("Parent item is archived."),
                ParentRelation::Deleted(_) => Some("Parent item is deleted."),
                ParentRelation::Cycle(_) => Some("Parent relationship contains a cycle."),
                ParentRelation::Invalid => Some("Parent relationship is unavailable."),
            }
            .map(str::to_owned);
            Arc::new(RowView {
                id: row.id.clone(),
                instance_id: row.id.clone(),
                label,
                cells: Arc::new(cells),
                depth: entry.depth,
                has_children: entry.has_children,
                expanded: false,
                parent_id: row.parent_id().map(str::to_owned),
                parent_notice,
                subtree_end: entry.subtree_end,
            })
        })
        .collect()
}

// A renderer-only anchor keeps hierarchy controls reachable for schemas with
// no declared/text primary column. This never changes canonical schema identity.
fn display_primary(document: &Document) -> Option<&super_platinum_core::slack_lists::Column> {
    document
        .primary_column()
        .or_else(|| document.columns.first())
}

pub(super) fn project_cells(
    loaded: &Loaded,
    row: &super_platinum_core::slack_lists::Record,
    workspace: Option<&Workspace>,
    media: &crate::media::MediaRegistry,
) -> (Vec<CellView>, String) {
    let cells: Vec<_> = loaded
        .document
        .columns
        .iter()
        .map(|column| {
            // Preserve actual booleans even when readonly. Only an eligible editable
            // missing ordinary Checkbox gets an input default; raw cells stay Empty.
            let input = loaded
                .document
                .editable(loaded.allow_lists.as_deref())
                .then(|| {
                    column.checkbox_input_value_with_policy(
                        row,
                        &loaded.document.id,
                        loaded.personal_task_lists.as_ref(),
                    )
                })
                .flatten();
            // Reading checkbox state must not clone unrelated rich JSON.
            let checkbox = matches!(
                column.kind.as_str(),
                "checkbox" | "completed" | "todo_completed"
            )
            .then(|| row.fields.get(&column.key)?.get("checkbox")?.as_bool())
            .flatten()
            .or(input);
            let (text, content) = content::project(workspace, media, column, row);
            CellView {
                column_id: column.id.clone(),
                text,
                checkbox,
                editable: input.is_some()
                    || (column.kind == "vote"
                        && loaded.document.editable(loaded.allow_lists.as_deref())
                        && workspace.is_some_and(|w| !w.self_user_id.is_empty())
                        && column
                            .vote_value(
                                row,
                                &loaded.document.id,
                                loaded.personal_task_lists.as_ref(),
                            )
                            .is_ok()),
                can_edit: loaded.document.editable(loaded.allow_lists.as_deref())
                    && column.editable_value(
                        row,
                        &loaded.document.id,
                        loaded.personal_task_lists.as_ref(),
                    ),
                content,
            }
        })
        .collect();
    let primary = display_primary(&loaded.document)
        .and_then(|column| cells.iter().find(|cell| cell.column_id == column.id));
    let label = primary
        .map(|cell| cell.text.as_str())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("Untitled item")
        .to_owned();
    (cells, label)
}
