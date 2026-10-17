//! Captured Jd3h/L34r delete-column request. No row, cell or task-bundle mutation.
use super::*;
use crate::slack_lists::ColumnDeletePermit;
pub fn list_column_delete(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ColumnDeletePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.columns.delete",
        vec![
            ("list_id", intent.list_id().into()),
            ("key", intent.column_key().into()),
            ("_x_reason", "delete-column".into()),
        ],
    )
}
