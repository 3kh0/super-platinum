//! Captured new-view editor: optional ordinary grouping, no copied settings or guessed ID.
use super::*;
use crate::slack_lists::ViewCreatePermit;
pub fn list_view_create(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewCreatePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    let mut fields = vec![
        ("list_id", intent.list_id().into()),
        ("name", intent.desired_name().into()),
        ("type", intent.layout().as_str().into()),
    ];
    if let Some(key) = intent.grouping_key() {
        fields.push(("grouping", serde_json::json!({"group_by":key}).to_string()));
    }
    fields.push(("_x_reason", "create-view".into()));
    client.rest_form(session, "lists.views.create", fields)
}
