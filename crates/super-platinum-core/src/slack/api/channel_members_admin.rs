use super::*;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMemberAction {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMemberResult {
    Changed,
    AlreadyPresent,
}

/// Workspace policy is enforced by Slack. These are the channel-type and
/// membership prerequisites, not a claim that the user is an administrator.
pub fn channel_members_manageable(channel: &Channel) -> bool {
    !channel.is_archived
        && !channel.is_im
        && !channel.is_mpim
        && channel.extra.get("is_member").and_then(|v| v.as_bool()) == Some(true)
}

pub fn conversation_member_change(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    user: &str,
    action: ChannelMemberAction,
) -> Result<PreparedRequest, Error> {
    // A single identity per request gives each selected person an unambiguous
    // outcome, without bulk invite's all-or-nothing/force partial semantics.
    if user.len() < 2
        || !matches!(user.as_bytes()[0], b'U' | b'W')
        || !user.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(Error::Api("user_not_found".into()));
    }
    if user == session.user_id {
        return Err(Error::Api(
            if action == ChannelMemberAction::Add {
                "cant_invite_self"
            } else {
                "cant_kick_self"
            }
            .into(),
        ));
    }
    Ok(client.rest_form(
        session,
        if action == ChannelMemberAction::Add {
            "conversations.invite"
        } else {
            "conversations.kick"
        },
        vec![
            ("channel", channel.into()),
            (
                if action == ChannelMemberAction::Add {
                    "users"
                } else {
                    "user"
                },
                user.into(),
            ),
        ],
    ))
}

pub async fn change_conversation_member(
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    user: &str,
    action: ChannelMemberAction,
) -> Result<ChannelMemberResult, Error> {
    let request = conversation_member_change(client, session, channel, user, action)?;
    let value = match transport.execute(request).await {
        Err(Error::Api(code))
            if action == ChannelMemberAction::Add && code == "already_in_channel" =>
        {
            return Ok(ChannelMemberResult::AlreadyPresent);
        }
        result => result?,
    };
    if value.get("errors").is_some_and(|errors| match errors {
        Value::Object(values) => !values.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Null => false,
        _ => true,
    }) {
        return Err(Error::Api("membership_result_unconfirmed".into()));
    }
    if action == ChannelMemberAction::Add
        && value
            .get("channel")
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            != Some(channel)
    {
        return Err(Error::Api("membership_result_mismatch".into()));
    }
    Ok(ChannelMemberResult::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn session(url: String) -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url,
            token: String::new(),
        }
    }

    #[test]
    fn member_actions_require_membership_and_one_other_identity() {
        for value in [
            json!({"id":"C1"}),
            json!({"id":"C1","is_member":false}),
            json!({"id":"C1","is_member":true,"is_archived":true}),
            json!({"id":"D1","is_member":true,"is_im":true}),
            json!({"id":"G1","is_member":true,"is_mpim":true}),
        ] {
            assert!(!channel_members_manageable(
                &serde_json::from_value(value).unwrap()
            ));
        }
        assert!(channel_members_manageable(
            &serde_json::from_value(json!({"id":"C1","is_member":true,"is_private":true})).unwrap()
        ));
        for user in ["", "U", "U2,U3", "U2&user=U3", "U1"] {
            for action in [ChannelMemberAction::Add, ChannelMemberAction::Remove] {
                assert!(
                    conversation_member_change(
                        &SlackClient::default(),
                        &session("https://example.invalid".into()),
                        "C1",
                        user,
                        action
                    )
                    .is_err()
                );
            }
        }
    }

    #[tokio::test]
    async fn member_mutations_use_exact_targets_and_do_not_retry_or_hide_failures() {
        for (action, body, expected) in [
            (
                ChannelMemberAction::Add,
                json!({"ok":true,"channel":{"id":"C1"}}),
                "changed",
            ),
            (ChannelMemberAction::Remove, json!({"ok":true}), "changed"),
            (
                ChannelMemberAction::Remove,
                json!({"ok":true,"errors":{"U2":"restricted_action"}}),
                "membership_result_unconfirmed",
            ),
            (
                ChannelMemberAction::Add,
                json!({"ok":false,"error":"already_in_channel"}),
                "present",
            ),
            (
                ChannelMemberAction::Add,
                json!({"ok":true,"channel":{"id":"OTHER"}}),
                "membership_result_mismatch",
            ),
            (
                ChannelMemberAction::Remove,
                json!({"ok":false,"error":"not_in_channel"}),
                "not_in_channel",
            ),
            (
                ChannelMemberAction::Remove,
                json!({"ok":false,"error":"restricted_action"}),
                "restricted_action",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 8192];
                let count = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                assert!(request.contains("channel=C1"));
                assert!(request.contains(if action == ChannelMemberAction::Add {
                    "users=U2"
                } else {
                    "user=U2"
                }));
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let session = session(format!("http://{address}"));
            let client = SlackClient::default();
            assert!(
                !conversation_member_change(&client, &session, "C1", "U2", action)
                    .unwrap()
                    .retry_safe()
            );
            let result = change_conversation_member(
                &Transport::new("").unwrap(),
                &client,
                &session,
                "C1",
                "U2",
                action,
            )
            .await;
            match expected {
                "changed" => assert_eq!(result.unwrap(), ChannelMemberResult::Changed),
                "present" => assert_eq!(result.unwrap(), ChannelMemberResult::AlreadyPresent),
                code => assert!(
                    matches!(result,Err(Error::Api(ref actual)) if actual==code),
                    "{result:?}"
                ),
            }
            server.await.unwrap();
        }
    }
}
