//! Read-only lookup from retained Slack I3nz/HxuJ, never conversations.open.
use super::*;
pub fn file_share_im_list(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    // I3nz defaults both true; omitted limit asks for its normal complete list.
    client.rest_form(
        workspace,
        "im.list",
        vec![
            ("get_latest", "true".into()),
            ("get_read_state", "true".into()),
        ],
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    #[test]
    fn im_lookup_is_the_literal_read_form_without_open_or_peer_fields() {
        let session = WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "fixture".into(),
            url: "https://example.slack.com".into(),
            token: "synthetic".into(),
        };
        let request = file_share_im_list(&SlackClient::default(), &session);
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with("/api/im.list")
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "synthetic".into()),
                ("get_latest".into(), "true".into()),
                ("get_read_state".into(), "true".into())
            ]
        );
    }
}
