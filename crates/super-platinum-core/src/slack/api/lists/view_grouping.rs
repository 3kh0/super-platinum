use super::*;
use crate::slack_lists::ViewGroupingPermit;
/// Exact simple grouping replacement; {group_by:""} explicitly clears grouping.
pub fn list_view_grouping(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewGroupingPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.views.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.view_id().into()),
            ("grouping", intent.grouping().to_string()),
            ("_x_reason", "update-view".into()),
        ],
    )
}
