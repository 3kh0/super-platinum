//! Request shapes verified against Slack's shipped thread subscription fetchers.
use super::*;

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
