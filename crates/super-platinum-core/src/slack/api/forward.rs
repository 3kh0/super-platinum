use super::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ForwardMessage {
    pub source: String,
    pub timestamp: String,
    pub destination: String,
    pub blocks: Vec<Value>,
    pub client_msg_id: String,
}
impl ForwardMessage {
    pub fn new(source: String, timestamp: String, destination: String, blocks: Vec<Value>) -> Self {
        Self {
            source,
            timestamp,
            destination,
            blocks,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}
#[derive(Debug, Deserialize)]
pub struct ForwardedMessage {
    pub channel: String,
    pub ts: String,
}

/// Matches the web client's private-message forward request, including its
/// rich-text note and stable client id. Never retry writes automatically.
pub fn chat_share_message(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: &ForwardMessage,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "chat.shareMessage",
        vec![
            ("channel", args.source.clone()),
            ("timestamp", args.timestamp.clone()),
            ("share_channel", args.destination.clone()),
            ("client_msg_id", args.client_msg_id.clone()),
            ("text", String::new()),
            (
                "blocks",
                serde_json::to_string(&args.blocks).unwrap_or_else(|_| "[]".into()),
            ),
        ],
    )
}
pub async fn forward_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: &ForwardMessage,
) -> Result<ForwardedMessage, Error> {
    decode(
        transport
            .execute(chat_share_message(client, workspace, args))
            .await?,
        "forwarded message",
    )
}
pub fn chat_get_permalink(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    ts: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "chat.getPermalink",
        vec![("channel", channel.into()), ("message_ts", ts.into())],
    )
}
pub async fn fetch_permalink(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    ts: &str,
    thread_ts: Option<&str>,
) -> Result<String, Error> {
    // The public method rejects enterprise-context web tokens. The web
    // client's archive link uses the workspace host and the full timestamp,
    // with the parent timestamp retained for thread navigation.
    if uses_enterprise_permalink(workspace) {
        if let Some(link) = workspace_permalink(&workspace.url, channel, ts, thread_ts) {
            return Ok(link);
        }
    }
    #[derive(Deserialize)]
    struct Link {
        permalink: String,
    }
    let link: Link = decode(
        transport
            .execute(chat_get_permalink(client, workspace, channel, ts))
            .await?,
        "message link",
    )?;
    Ok(link.permalink)
}

fn uses_enterprise_permalink(workspace: &WorkspaceSession) -> bool {
    // Enterprise entries imported from Slack localConfig can omit
    // enterprise_id even though their URL is the enterprise host.
    workspace.enterprise_id.is_some()
        || workspace.url.parse::<wreq::Uri>().ok().is_some_and(|url| {
            url.host()
                .is_some_and(|host| host.ends_with(".enterprise.slack.com"))
        })
}

pub fn workspace_permalink(
    workspace_url: &str,
    channel: &str,
    ts: &str,
    thread_ts: Option<&str>,
) -> Option<String> {
    fn timestamp(value: &str) -> Option<(&str, &str)> {
        let (seconds, fraction) = value.split_once('.')?;
        (!seconds.is_empty()
            && fraction.len() == 6
            && seconds.bytes().all(|b| b.is_ascii_digit())
            && fraction.bytes().all(|b| b.is_ascii_digit()))
        .then_some((seconds, fraction))
    }
    let (seconds, fraction) = timestamp(ts)?;
    if channel.is_empty() || !channel.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let url: wreq::Uri = workspace_url.parse().ok()?;
    let authority = url.authority()?;
    let host = authority.host();
    if url.scheme_str() != Some("https")
        || !host.ends_with(".slack.com")
        || authority.as_str().contains('@')
        || authority.port().is_some()
    {
        return None;
    }
    let host = host
        .strip_suffix(".enterprise.slack.com")
        .map(|prefix| format!("{prefix}.slack.com"))
        .unwrap_or_else(|| host.to_owned());
    let mut link = format!("https://{host}/archives/{channel}/p{seconds}{fraction}");
    if let Some(root) = thread_ts {
        timestamp(root)?;
        link.push_str(&format!("?thread_ts={root}&cid={channel}"));
    }
    Some(link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_enterprise_session_does_not_require_enterprise_id() {
        let mut workspace = WorkspaceSession {
            team_id: "E123".into(),
            enterprise_id: None,
            user_id: "U123".into(),
            name: "Example".into(),
            url: "https://example.enterprise.slack.com".into(),
            token: "test-only".into(),
        };
        assert!(uses_enterprise_permalink(&workspace));
        workspace.url = "https://hackclub.slack.com".into();
        assert!(!uses_enterprise_permalink(&workspace));
        workspace.enterprise_id = Some("E123".into());
        assert!(uses_enterprise_permalink(&workspace));
    }
}
