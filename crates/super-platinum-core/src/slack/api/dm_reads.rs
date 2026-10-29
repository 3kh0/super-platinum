//! Existing DM feed and conversation reads, injectable at the actual executor.
use super::*;
use std::future::Future;

pub async fn fetch_client_dms_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    count: u32,
    cursor: Option<String>,
    execute: F,
) -> Result<ClientDmsPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let value = execute(client_dms(client, workspace, count, cursor)).await?;
    if value["ok"] == false {
        return Err(Error::Api(
            value["error"]
                .as_str()
                .unwrap_or("client_dms_failed")
                .into(),
        ));
    }
    if !value.get("dms").is_some_and(serde_json::Value::is_array) {
        return Err(Error::Transport(
            "decode client.dms: dms must be an array".into(),
        ));
    }
    decode(value, "client.dms")
}

pub async fn fetch_conversation_info_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel_id: ChannelId,
    execute: F,
) -> Result<Channel, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    #[derive(serde::Deserialize)]
    struct ConversationInfoPage {
        channel: Channel,
    }
    let value = execute(conversations_info(client, workspace, channel_id)).await?;
    if value["ok"] == false {
        return Err(Error::Api(
            value["error"]
                .as_str()
                .unwrap_or("conversation_info_failed")
                .into(),
        ));
    }
    let page: ConversationInfoPage = decode(value, "conversations.info")?;
    Ok(page.channel)
}

#[cfg(test)]
mod tests;
