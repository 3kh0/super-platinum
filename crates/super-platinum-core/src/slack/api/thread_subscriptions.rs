//! Request shapes verified against Slack's shipped thread subscription fetchers.
use super::*;

/// The ordinary no-bound Mark-all call from the shipped Threads banner.
/// No loaded-row timestamp is a workspace-wide high-water or Undo receipt.
/// SDK provider defaults and live server acceptance are separate boundaries.
pub fn clear_thread_unreads_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> PreparedRequest {
    client.rest_form(workspace, "subscriptions.thread.clearAll", Vec::new())
}

pub fn thread_subscription_request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root: MessageTs,
    active: bool,
    last_read: Option<MessageTs>,
) -> PreparedRequest {
    let mut fields = vec![("channel", channel), ("thread_ts", root)];
    if let Some(cursor) = last_read {
        fields.push(("last_read", cursor));
    }
    client.rest_form(
        workspace,
        if active {
            "subscriptions.thread.add"
        } else {
            "subscriptions.thread.remove"
        },
        fields,
    )
}

pub async fn set_thread_subscription(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root: MessageTs,
    active: bool,
    last_read: Option<MessageTs>,
) -> Result<(), Error> {
    transport
        .execute(thread_subscription_request(
            client, workspace, channel, root, active, last_read,
        ))
        .await?;
    Ok(())
}

pub async fn fetch_thread_subscription(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root: MessageTs,
) -> Result<bool, Error> {
    #[derive(serde::Deserialize)]
    struct Page {
        subscriptions: Vec<String>,
    }
    let value = transport
        .execute(client.rest_form(
            workspace,
            "subscriptions.thread.get",
            vec![("channel", channel), ("thread_ts", root.clone())],
        ))
        .await?;
    let page: Page = decode(value, "subscriptions.thread.get")?;
    Ok(page.subscriptions.contains(&root))
}

#[cfg(test)]
mod clear_tests {
    use super::*;

    #[test]
    fn ordinary_clear_all_has_no_guessed_bound_or_undo_and_is_not_retry_safe() {
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Offline fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic-token".into(),
        };
        let request = clear_thread_unreads_request(&SlackClient::default(), &workspace);
        assert!(request.url.contains("/api/subscriptions.thread.clearAll?"));
        assert!(!request.retry_safe());
        let crate::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("ordinary Slack form recipe");
        };
        assert_eq!(fields, vec![("token".into(), "synthetic-token".into())]);
    }
}
