use super::*;
use crate::slack_lists::ViewFilterPermit;
/// Complete filter replacement; [] removes the existing supported rule.
pub fn list_view_filter(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewFilterPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.views.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.view_id().into()),
            ("filters", intent.filters().to_string()),
            (
                "info_column_filters",
                intent.info_column_filters().to_string(),
            ),
            ("_x_reason", "update-view".into()),
        ],
    )
}
