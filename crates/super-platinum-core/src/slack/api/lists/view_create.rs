//! Captured new-view editor: omitted grouping, no copied settings or guessed ID.
use super::*;
use crate::slack_lists::ViewCreatePermit;
pub fn list_view_create(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewCreatePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.views.create",
        vec![
            ("list_id", intent.list_id().into()),
            ("name", intent.desired_name().into()),
            ("type", intent.layout().as_str().into()),
            ("_x_reason", "create-view".into()),
        ],
    )
}
