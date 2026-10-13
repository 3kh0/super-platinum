//! Captured B t0NX Open record -> V wCCe lists.records.open.
use super::*;

/// Track a current explicit detail visit, without inventing read timestamps,
/// view/thread context or a separate close endpoint. This mutates server state:
/// use execute_list_read_state after validating current detail ownership.
pub fn lists_records_open(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    list_id: &str,
    record_id: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "lists.records.open",
        vec![
            ("list_id", list_id.into()),
            ("id", record_id.into()),
            ("_x_reason", "open-record".into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;

    #[test]
    fn record_open_uses_only_captured_identity_fields_and_is_not_retry_safe() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T_TEST".into(),
            enterprise_id: None,
            user_id: "U_TEST".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        };
        let request = lists_records_open(&client, &workspace, "F_LIST", "R_RECORD");
        assert_eq!(request.method, "POST");
        assert!(request.url.contains("/api/lists.records.open?"));
        assert!(!request.retry_safe());
        let RequestBody::Form(fields) = request.body else {
            panic!("expected form")
        };
        // Authentication is supplied by the existing SlackClient request builder.
        let args = fields
            .into_iter()
            .filter(|(key, _)| key != "token")
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                ("list_id".into(), "F_LIST".into()),
                ("id".into(), "R_RECORD".into()),
                ("_x_reason".into(), "open-record".into()),
            ]
        );
    }
}
