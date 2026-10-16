//! Captured TC0j update-column request, narrowed to a lossless Text/Number name edit.
use super::*;
use crate::slack_lists::ColumnRenamePermit;

pub fn list_column_rename(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ColumnRenamePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.columns.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("key", intent.column_key().into()),
            ("name", intent.desired_name().into()),
            ("options", intent.unchanged_options().to_string()),
            ("backfill_missing_cells_with_default", "true".into()),
            ("_x_reason", "update-column".into()),
        ],
    )
}
