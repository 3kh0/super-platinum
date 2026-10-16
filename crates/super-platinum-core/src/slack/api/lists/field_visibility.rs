//! Captured 0Kzd columns array, JSON form encoding, no other view settings.
use super::*;
use crate::slack_lists::ViewFieldVisibilityPermit;
pub fn list_field_visibility(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewFieldVisibilityPermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(session,"lists.views.update",vec![
        ("list_id",intent.list_id().into()),("id",intent.view_id().into()),
        ("columns",serde_json::json!([{"key":intent.column_key(),"visible":intent.desired_visible(),"id":intent.column_id()}]).to_string()),
        ("_x_reason","update-view".into()),
    ])
}
