//! Clock-only projection updates cached timestamp cells without reparsing rich
//! fields, changing canonical records, or retiring editor/detail authority.
use super::*;
use std::collections::HashMap;
use super_platinum_core::slack_lists::{
    saved_views::DateContext,
    timestamps::{self, TimestampContext, TimestampPresentation},
};

pub(in crate::slack_lists) fn project_timestamps(
    loaded: &Loaded,
    rows: &mut Arc<Vec<Arc<RowView>>>,
    workspace: Option<&Workspace>,
    calendar: Option<&DateContext>,
) {
    let Some(calendar) = calendar else {
        return;
    };
    let columns: HashMap<_, _> = loaded
        .document
        .columns
        .iter()
        .filter(|column| timestamps::supports(column))
        .map(|column| (column.id.as_str(), column))
        .collect();
    if columns.is_empty() {
        return;
    }
    let records: HashMap<_, _> = loaded
        .records
        .records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect();
    let context = TimestampContext::new(
        calendar.clone(),
        workspace.and_then(|workspace| workspace.locale.as_deref()),
        workspace
            .and_then(|workspace| workspace.notifications.user.get("time24"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );
    for index in 0..rows.len() {
        let row = &rows[index];
        let Some(record) = records.get(row.id.as_str()) else {
            continue;
        };
        let mut replacement = None;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let Some(column) = columns.get(cell.column_id.as_str()) else {
                continue;
            };
            let Some(value) = timestamps::project(column, record, &context) else {
                continue;
            };
            let (text, content) = match value {
                TimestampPresentation::Empty => (String::new(), CellContent::Plain),
                TimestampPresentation::Value { label, tooltip } => {
                    (label, CellContent::Timestamp { tooltip })
                }
                TimestampPresentation::Unsupported { fallback } => (fallback, CellContent::Plain),
            };
            if text != cell.text || content != cell.content {
                let cells = replacement.get_or_insert_with(|| (*row.cells).clone());
                cells[cell_index].text = text;
                cells[cell_index].content = content;
            }
        }
        if let Some(cells) = replacement {
            let mut updated = (**row).clone();
            updated.cells = Arc::new(cells);
            if let Some(primary) = display_primary(&loaded.document).and_then(|column| {
                updated
                    .cells
                    .iter()
                    .find(|cell| cell.column_id == column.id)
            }) {
                updated.label = if primary.text.trim().is_empty() {
                    "Untitled item".into()
                } else {
                    primary.text.clone()
                };
            }
            Arc::make_mut(rows)[index] = Arc::new(updated);
        }
    }
}
