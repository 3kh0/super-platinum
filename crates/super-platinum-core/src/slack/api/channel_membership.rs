use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMembershipAction {
    Join,
    Leave,
    Archive,
}
impl ChannelMembershipAction {
    pub fn member(self) -> bool {
        self == Self::Join
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Join => "Join channel",
            Self::Leave => "Leave channel",
            Self::Archive => "Archive channel",
        }
    }
}

/// Only offer actions supported by the known channel type and membership.
/// Unknown membership must be resolved with conversations.info first.
pub fn channel_membership_action(channel: &Channel) -> Option<ChannelMembershipAction> {
    if channel.is_archived || channel.is_im || channel.is_mpim {
        return None;
    }
    match channel.extra.get("is_member").and_then(|v| v.as_bool()) {
        Some(true) if channel.extra.get("is_general").and_then(|v| v.as_bool()) != Some(true) => {
            Some(
                if channel.extra.get("num_members").and_then(|v| v.as_u64()) == Some(1) {
                    ChannelMembershipAction::Archive
                } else {
                    ChannelMembershipAction::Leave
                },
            )
        }
        Some(false) if !channel.is_private && !channel.is_group => {
            Some(ChannelMembershipAction::Join)
        }
        _ => None,
    }
}

pub fn conversation_membership(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    action: ChannelMembershipAction,
) -> PreparedRequest {
    client.rest_form(
        session,
        match action {
            ChannelMembershipAction::Join => "conversations.join",
            ChannelMembershipAction::Leave => "conversations.leave",
            ChannelMembershipAction::Archive => "conversations.archive",
        },
        vec![("channel", channel.into())],
    )
}

pub async fn change_conversation_membership(
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    action: ChannelMembershipAction,
) -> Result<Option<Channel>, Error> {
    let result = transport
        .execute(conversation_membership(client, session, channel, action))
        .await;
    if action == ChannelMembershipAction::Leave
        && matches!(&result, Err(Error::Api(code)) if code == "not_in_channel")
    {
        return Ok(None);
    }
    let value = result?;
    if action.member() {
        #[derive(serde::Deserialize)]
        struct Joined {
            channel: Channel,
        }
        let joined: Joined = decode(value, "join channel")?;
        if joined.channel.id != channel {
            return Err(Error::Api("membership_result_mismatch".into()));
        }
        Ok(Some(joined.channel))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn membership_actions_require_known_membership_and_respect_channel_type() {
        let action = |value| channel_membership_action(&serde_json::from_value(value).unwrap());
        assert_eq!(
            action(json!({"id":"C1","is_member":true,"is_private":true,"num_members":1})),
            Some(ChannelMembershipAction::Archive)
        );
        assert_eq!(
            action(json!({"id":"C1","is_member":false})),
            Some(ChannelMembershipAction::Join)
        );
        assert_eq!(
            action(json!({"id":"C1","is_member":true,"is_private":true})),
            Some(ChannelMembershipAction::Leave)
        );
        for value in [
            json!({"id":"C1"}),
            json!({"id":"C1","is_member":false,"is_private":true}),
            json!({"id":"G1","is_member":false,"is_group":true}),
            json!({"id":"C1","is_member":true,"is_archived":true}),
            json!({"id":"C1","is_member":true,"is_general":true}),
            json!({"id":"D1","is_member":true,"is_im":true}),
            json!({"id":"G1","is_member":true,"is_mpim":true}),
        ] {
            assert_eq!(action(value), None);
        }
    }

    #[tokio::test]
    async fn membership_round_trips_decode_only_confirmed_results_and_never_retry_mutations() {
        for (action, body, expected) in [
            (ChannelMembershipAction::Archive, json!({"ok":true}), "left"),
            (
                ChannelMembershipAction::Join,
                json!({"ok":true,"channel":{"id":"C1","is_member":true},"warning":"already_in_channel"}),
                "joined",
            ),
            (
                ChannelMembershipAction::Join,
                json!({"ok":true,"channel":{"id":"OTHER"}}),
                "membership_result_mismatch",
            ),
            (ChannelMembershipAction::Leave, json!({"ok":true}), "left"),
            (
                ChannelMembershipAction::Leave,
                json!({"ok":false,"not_in_channel":true}),
                "left",
            ),
            (
                ChannelMembershipAction::Leave,
                json!({"ok":false,"error":"last_member"}),
                "last_member",
            ),
            (
                ChannelMembershipAction::Join,
                json!({"ok":false,"not_in_channel":true}),
                "unknown_error",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let body = body.to_string();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                socket.read(&mut [0; 8192]).await.unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            let session = WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "test".into(),
                url: format!("http://{address}"),
                token: String::new(),
            };
            let client = SlackClient::default();
            let request = conversation_membership(&client, &session, "C1", action);
            assert!(!request.retry_safe());
            let result = change_conversation_membership(
                &Transport::new("").unwrap(),
                &client,
                &session,
                "C1",
                action,
            )
            .await;
            match expected {
                "joined" => assert_eq!(result.unwrap().unwrap().id, "C1"),
                "left" => assert!(result.unwrap().is_none()),
                code => assert!(
                    matches!(result,Err(Error::Api(ref actual)) if actual == code),
                    "{result:?}"
                ),
            }
            server.await.unwrap();
        }
    }
}
