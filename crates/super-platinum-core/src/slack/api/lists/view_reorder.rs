use super::*;
use crate::slack_lists::ViewReorderPermit;
/// One adjacent saved-view move; only the target position is sent.
pub fn list_view_reorder(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewReorderPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.views.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.view_id().into()),
            ("position", intent.desired_position().into()),
            ("_x_reason", "update-view".into()),
        ],
    )
}
