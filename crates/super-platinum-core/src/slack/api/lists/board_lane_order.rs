use super::*;
use crate::slack_lists::BoardLaneOrderPermit;
/// Exact retained Board grouping replacement; all unrelated members are retained.
pub fn list_board_lane_order(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: BoardLaneOrderPermit,
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
