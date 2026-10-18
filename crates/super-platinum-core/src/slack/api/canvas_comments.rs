//! The desktop client's read-only document stream, separate from channel history.
use super::*;
mod archive;
mod bootstrap;
mod reply;
use crate::canvas::comments::{self, Filter};
pub use archive::{canvas_comment_archive, execute_canvas_comment_archive};
pub use bootstrap::{canvas_comment_bootstrap, execute_canvas_comment_bootstrap};
pub use reply::{canvas_comment_reply, execute_canvas_comment_reply};

fn invalid(_: comments::Invalid) -> Error {
    Error::Api("invalid_canvas_comment_read".into())
}
pub fn canvas_comments(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    file: &str,
    filter: Filter,
    cursor: Option<&str>,
) -> Result<PreparedRequest, Error> {
    let mut fields = vec![
        ("channel", comments::channel(file).map_err(invalid)?),
        ("limit", "20".into()),
        ("document_stream", filter.wire().into()),
        ("ignore_replies", "true".into()),
        ("inclusive", "true".into()),
        ("include_pin_count", "true".into()),
        ("no_user_profile", "true".into()),
        ("include_stories", "true".into()),
        ("include_free_team_extra_messages", "true".into()),
        ("include_date_joined", "false".into()),
        ("cached_latest_updates", "{}".into()),
    ];
    if let Some(cursor) = comments::cursor(cursor).map_err(invalid)? {
        fields.push(("cursor", cursor.into()));
    }
    Ok(client.rest_form(workspace, "conversations.history", fields))
}
pub fn canvas_comment_replies(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    file: &str,
    root: &str,
    cursor: Option<&str>,
) -> Result<PreparedRequest, Error> {
    if !comments::timestamp(root) {
        return Err(invalid(comments::Invalid::Identity));
    }
    let mut request = conversations_replies(
        client,
        workspace,
        RepliesArgs {
            channel: comments::channel(file).map_err(invalid)?,
            ts: root.into(),
            cursor: comments::cursor(cursor)
                .map_err(invalid)?
                .map(str::to_owned),
            limit: Some(28),
            inclusive: true,
            ..Default::default()
        },
    );
    if let super::super::client::RequestBody::Form(fields) = &mut request.body {
        fields.push(("cached_latest_updates".into(), "{}".into()));
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    #[test]
    fn canvas_comment_reads_use_exact_virtual_channel_filter_and_cursor() {
        let client = SlackClient::default();
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let request = canvas_comments(
            &client,
            &workspace,
            "F123",
            Filter::Resolved,
            Some("opaque:+/="),
        )
        .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with("/api/conversations.history")
        );
        for (key, value) in [
            ("channel", "C123"),
            ("document_stream", "archived"),
            ("limit", "20"),
            ("cursor", "opaque:+/="),
        ] {
            assert!(fields.iter().any(|(k, v)| k == key && v == value));
        }
        assert!(
            !fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "text" | "markdown" | "read"))
        );
        assert!(canvas_comment_replies(&client, &workspace, "F123", "Qpc:12345678", None).is_err());
    }
}
