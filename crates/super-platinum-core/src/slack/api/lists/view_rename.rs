//! Captured existing-view editor -> 0Kzd update-view wire, not a full view rewrite.
use super::*;
use crate::slack_lists::ViewRenamePermit;
pub fn list_view_rename(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ViewRenamePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    let mut fields = vec![
        ("list_id", intent.list_id().into()),
        ("id", intent.view_id().into()),
        ("name", intent.desired_name().into()),
        ("type", intent.kind().into()),
    ];
    if let Some(grouping) = intent.grouping() {
        fields.push(("grouping", grouping.to_string()));
    }
    fields.push(("_x_reason", "update-view".into()));
    client.rest_form(session, "lists.views.update", fields)
}
