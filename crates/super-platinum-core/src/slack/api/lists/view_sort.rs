use super::*;
use crate::slack_lists::ViewSortPermit;
/// Complete sort replacement; [] removes the existing supported rule.
pub fn list_view_sort(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewSortPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.views.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.view_id().into()),
            ("sorts", intent.sorts().to_string()),
            ("_x_reason", "update-view".into()),
        ],
    )
}
