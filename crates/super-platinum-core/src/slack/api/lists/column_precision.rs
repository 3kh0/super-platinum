//! Captured TC0j options-only update, narrowed to ordinary Number precision.
use super::*;
use crate::slack_lists::ColumnPrecisionPermit;

pub fn list_column_precision(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ColumnPrecisionPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.columns.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("key", intent.column_key().into()),
            (
                "options",
                format!("{{\"precision\":{}}}", intent.desired_precision()),
            ),
            ("backfill_missing_cells_with_default", "true".into()),
            ("_x_reason", "update-column".into()),
        ],
    )
}
