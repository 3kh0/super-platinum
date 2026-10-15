use super::*;
use crate::slack_lists::{ListTimestamp, SubitemCreatePermit};
/// Stock detail passes empty viewId and only the parent. No implicit field seed.
pub fn list_subitem_create(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    permit: SubitemCreatePermit,
    timestamp: &ListTimestamp,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        workspace,
        "lists.records.create",
        vec![
            ("list_id", intent.list_id().into()),
            ("position", "".into()),
            ("updated_timestamp", timestamp.to_string()),
            ("_x_reason", "create-record".into()),
            ("parent_record_id", intent.parent_id().into()),
        ],
    )
}
