//! Pure saved-table projection from captured B czID/+E6Z/useGetRowItems.
//! No mutations, implicit fallback, or cursor continuation.
use super::{Column, Document, Record, RecordsPage};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod board;
mod dates;
mod defaults;
mod evaluate;
mod grouping;
mod model;
pub use dates::{DateContext, DateDependencies, date_dependencies};
pub use defaults::{CompletionPolicy, DefaultSelection, SelectionOrigin};
pub use model::{SavedView, UnsupportedView, catalog};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default)]
pub struct ProjectionContext {
    /// Captured !viewOnly controls empty group presentation. This flag does not
    /// authorize a mutation; callers must still enforce canonical permissions.
    pub editable: bool,
    /// Ephemeral local Find over the full loaded schema, after rules and sort.
    pub find: Option<super::find::Query>,
    pub date: Option<DateContext>,
    /// Verified personal List + todos experiment + !rollback decoder policy.
    pub date_uses_timestamps: Option<bool>,
    pub viewer_id: Option<String>,
    pub user_real_names: BTreeMap<String, String>,
    pub user_display_names: BTreeMap<String, String>,
    pub channel_names: BTreeMap<String, String>,
    pub collapsed_groups: BTreeSet<String>,
    pub todo_mode: bool,
    /// Explicit caller feature/personal-list policy, never inferred from a name.
    pub apply_todo_completion_rules: Option<bool>,
    /// Verified local/personal-task override, before the saved view boolean.
    pub effective_show_completed: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowInstance {
    pub instance_id: String,
    pub record_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// None for an ungrouped table; Some("") for its Ungrouped bucket.
    pub key: Option<String>,
    pub label: String,
    pub rows: Vec<RowInstance>,
    pub count: usize,
    pub collapsed: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectionLayout {
    #[default]
    Table,
    Board,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub layout: ProjectionLayout,
    /// Todo Board primary-header checkbox, separate from visible body fields.
    pub board_completion_column_id: Option<String>,
    pub view_id: Option<String>,
    pub title: String,
    pub column_ids: Vec<String>,
    pub groups: Vec<Group>,
    pub unique_record_count: usize,
    pub incomplete: bool,
}
type Result<T> = std::result::Result<T, UnsupportedView>;
fn unsupported(reason: impl Into<String>) -> UnsupportedView {
    UnsupportedView {
        reason: reason.into(),
    }
}
fn row_instance(group: Option<&str>, record: &Record) -> RowInstance {
    RowInstance {
        instance_id: serde_json::json!([group, record.id]).to_string(),
        record_id: record.id.clone(),
    }
}
fn base_rows<'a>(page: &'a RecordsPage, view: Option<&str>) -> Vec<&'a Record> {
    let mut rows = page
        .records
        .iter()
        .filter(|r| {
            !r.archived()
                && r.raw["is_deleted"] != true
                && r.raw["suggested"] != true
                && r.parent_id().is_none()
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| super::ordering::compare(a, b, view));
    rows
}
/// A separately named native fallback. It makes no saved-view parity claim.
pub fn all_loaded(document: &Document, page: &RecordsPage) -> Projection {
    let rows = base_rows(page, None);
    Projection {
        layout: ProjectionLayout::Table,
        board_completion_column_id: None,
        view_id: None,
        title: "All loaded items".into(),
        column_ids: document.columns.iter().map(|c| c.id.clone()).collect(),
        unique_record_count: rows.len(),
        incomplete: page.next_cursor.is_some(),
        groups: vec![grouping::group(
            None,
            String::new(),
            rows,
            &ProjectionContext::default(),
        )],
    }
}
pub fn project(
    document: &Document,
    page: &RecordsPage,
    view: &SavedView,
    context: &ProjectionContext,
) -> Result<Projection> {
    if !matches!(view.kind.as_str(), "table" | "board") {
        return Err(unsupported(format!(
            "The {} layout is not supported yet.",
            view.kind
        )));
    }
    if document.views.iter().find(|v| v["id"] == view.id) != Some(&view.raw) {
        return Err(unsupported(
            "This saved view changed or was removed. Refresh it before projecting.",
        ));
    }
    if view.raw.get("filter_set").is_some_and(|v| !v.is_null()) {
        return Err(unsupported(
            "Grouped saved filters have not been established.",
        ));
    }
    let grouping = model::group_column(document, &view.raw)?;
    let layout = if view.kind == "board" {
        ProjectionLayout::Board
    } else {
        ProjectionLayout::Table
    };
    let (columns, board_completion_column_id) = if layout == ProjectionLayout::Board {
        board::columns(document, view, grouping, context)?
    } else {
        (model::columns(document, &view.raw, grouping)?, None)
    };
    let filters = model::filters(&view.raw)?;
    let sorts = model::sorts(&view.raw)?;
    // Validate even for an empty page, so unsupported rules never masquerade as
    // an empty supported view. Cache field values once per row/rule evaluation.
    for filter in &filters {
        evaluate::validate_filter(document, filter, context)?;
    }
    for sort in &sorts {
        evaluate::validate_sort(document, sort, context)?;
    }
    evaluate::validate_completion_policy(context)?;
    let mut rows = Vec::new();
    for row in base_rows(page, Some(&view.id)) {
        let mut keep = true;
        for filter in &filters {
            keep &= evaluate::filter(document, row, filter, context)?;
        }
        if keep && evaluate::completion(document, row, view, context)? {
            rows.push(row);
        }
    }
    evaluate::sort(document, &mut rows, &sorts, context)?;
    if let Some(query) = context.find.as_ref().filter(|query| !query.text.is_empty()) {
        let search = super::find::Search::new(document, query).map_err(unsupported)?;
        let mut found = Vec::with_capacity(rows.len());
        for row in rows {
            if search.matches(row).map_err(unsupported)? {
                found.push(row);
            }
        }
        rows = found;
    }
    let unique_record_count = rows.len();
    let groups = if layout == ProjectionLayout::Board {
        board::project(grouping, view, rows, &filters, &sorts, context)?
    } else {
        grouping::project(grouping, rows, &filters, &sorts, context)?
    };
    Ok(Projection {
        layout,
        board_completion_column_id,
        view_id: Some(view.id.clone()),
        title: view.name.clone(),
        column_ids: columns,
        groups,
        unique_record_count,
        incomplete: page.next_cursor.is_some(),
    })
}

/// New-view grouping is bounded to ordinary field families already projected.
pub(super) fn supports_new_grouping(column: &Column) -> bool {
    column.kind == "checkbox" || (column.kind == "select" && evaluate::choices(column).is_ok())
}
