use super::*;
use crate::slack_lists::{CreationIntent, ListTimestamp};

/// Captured header New item: only the completed-view checkbox is seeded.
/// No view position, filter/group defaults, parent or client ID is invented.
/// Call only through Transport::execute_list_record_create after fresh preflight.
pub fn list_record_create(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    intent: &CreationIntent,
    timestamp: &ListTimestamp,
) -> PreparedRequest {
    let mut fields = vec![
        ("list_id", intent.list_id().into()),
        ("position", String::new()),
        ("updated_timestamp", timestamp.to_string()),
        ("_x_reason", "create-record".into()),
    ];
    if let Some(initial_fields) = intent.initial_fields() {
        fields.push(("initial_fields", initial_fields.to_string()));
    }
    client.rest_form(workspace, "lists.records.create", fields)
}
