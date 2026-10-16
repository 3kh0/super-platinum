//! Source g0Tw/a42q: minimal Text/Number field creation, without position or a client ID.
use super::*;
use crate::slack_lists::ColumnCreatePermit;
pub fn list_column_create(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: ColumnCreatePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    let mut fields = vec![
        ("list_id", intent.list_id().into()),
        ("view_id", intent.view_id().into()),
        ("name", intent.desired_name().into()),
        ("type", intent.kind().as_str().into()),
    ];
    if let Some(options) = intent.options() {
        fields.push(("options", options.to_string()));
    }
    fields.push(("_x_reason", "create-column".into()));
    client.rest_form(session, "lists.columns.create", fields)
}
