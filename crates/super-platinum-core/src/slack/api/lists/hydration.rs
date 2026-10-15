use super::*;
use crate::slack_lists::ListRecordHydration;

/// Retained record-details Jkle/t7 (32405): exact known-parent read.
/// This request has no continuation cursor or records.open side effect.
pub fn list_record_subtasks(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    list: &str,
    parent: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "lists.records.info",
        vec![
            ("list_id", list.into()),
            ("id", parent.into()),
            ("include_is_subscribed", "true".into()),
            ("_x_reason", "get-record-subtasks".into()),
        ],
    )
}

pub async fn fetch_list_record_subtasks(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    list: &str,
    parent: &str,
) -> Result<ListRecordHydration, Error> {
    let raw = transport
        .execute_list_record_info(list_record_subtasks(client, workspace, list, parent))
        .await?;
    ListRecordHydration::from_response(list, parent, &raw)
        .map_err(|error| Error::Api(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    #[test]
    fn known_parent_info_has_only_captured_fields() {
        let workspace = WorkspaceSession {
            team_id: "T_TEST".into(),
            user_id: "U_TEST".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        let request =
            list_record_subtasks(&SlackClient::default(), &workspace, "F_LIST", "R_PARENT");
        assert!(request.url.contains("/api/lists.records.info?"));
        let RequestBody::Form(fields) = request.body else {
            panic!("form required")
        };
        assert_eq!(
            fields
                .into_iter()
                .filter(|(key, _)| key != "token")
                .collect::<Vec<_>>(),
            vec![
                ("list_id".into(), "F_LIST".into()),
                ("id".into(), "R_PARENT".into()),
                ("include_is_subscribed".into(), "true".into()),
                ("_x_reason".into(), "get-record-subtasks".into()),
            ]
        );
    }
}
