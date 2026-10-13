//! Session-token scheduling uses `drafts.*`, not the bot-token chat API.
use super::*;
use crate::scheduled::{ScheduledMessage, ScheduledPage};

#[derive(Debug, Clone)]
pub struct ScheduleMessage {
    pub destinations: Vec<serde_json::Value>,
    pub blocks: Vec<serde_json::Value>,
    pub file_ids: Vec<String>,
    pub post_at: i64,
    pub client_msg_id: String,
}
impl ScheduleMessage {
    pub fn new(
        channel: String,
        thread_ts: Option<String>,
        blocks: Vec<serde_json::Value>,
        post_at: i64,
    ) -> Self {
        let mut destination = serde_json::json!({"channel_id":channel});
        if let Some(root) = thread_ts {
            destination["thread_ts"] = root.into();
        }
        Self {
            destinations: vec![destination],
            blocks,
            file_ids: vec![],
            post_at,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
        }
    }
    pub fn from_draft(draft: &ScheduledMessage) -> Self {
        Self {
            destinations: draft.destinations.clone(),
            blocks: draft.blocks.clone(),
            file_ids: draft.file_ids.clone(),
            post_at: draft.post_at,
            client_msg_id: draft.client_msg_id.clone(),
        }
    }
    /// Explicit conflict recovery preserves all content but cannot inherit a
    /// send schedule or reuse the original draft's message identity.
    pub fn unscheduled_copy(&self) -> Self {
        Self {
            post_at: 0,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            ..self.clone()
        }
    }
}
fn mutation_time() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis().to_string()
}
pub fn drafts_schedule(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    message: &ScheduleMessage,
    id: Option<&str>,
) -> PreparedRequest {
    let mut fields = vec![
        (
            "destinations",
            serde_json::to_string(&message.destinations).unwrap(),
        ),
        ("blocks", serde_json::to_string(&message.blocks).unwrap()),
        (
            "file_ids",
            serde_json::to_string(&message.file_ids).unwrap(),
        ),
        ("date_scheduled", message.post_at.to_string()),
        ("is_from_composer", "true".into()),
    ];
    let method = if let Some(id) = id {
        fields.push(("draft_id", id.into()));
        fields.push(("client_last_updated_ts", mutation_time()));
        "drafts.update"
    } else {
        fields.push(("client_msg_id", message.client_msg_id.clone()));
        "drafts.create"
    };
    client.rest_form(workspace, method, fields)
}
pub fn drafts_list(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "drafts.list",
        vec![("limit", limit.to_string()), ("is_active", "true".into())],
    )
}
pub fn drafts_delete(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "drafts.delete",
        vec![
            ("draft_id", id.into()),
            ("client_last_updated_ts", mutation_time()),
            ("skip_file_deletion", "true".into()),
        ],
    )
}
pub async fn fetch_scheduled_messages(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
) -> Result<ScheduledPage, Error> {
    decode(
        transport
            .execute(drafts_list(client, workspace, limit))
            .await?,
        "scheduled drafts",
    )
}
pub async fn schedule_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    message: &ScheduleMessage,
    id: Option<&str>,
) -> Result<ScheduledMessage, Error> {
    decode_scheduled_mutation(
        transport
            .execute_draft_mutation(drafts_schedule(client, workspace, message, id))
            .await?,
    )
}
fn decode_scheduled_mutation(value: serde_json::Value) -> Result<ScheduledMessage, Error> {
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(Error::Api(
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_error")
                .into(),
        ));
    }
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(Error::Transport(
            "Slack did not explicitly confirm the scheduled draft".into(),
        ));
    }
    #[derive(serde::Deserialize)]
    struct Created {
        draft: ScheduledMessage,
    }
    let response: Created = decode(value, "scheduled draft")?;
    Ok(response.draft)
}

pub async fn delete_scheduled_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
) -> Result<(), Error> {
    transport
        .execute(drafts_delete(client, workspace, id))
        .await
        .map(|_| ())
}

/// Cancel just the schedule while retaining the complete current draft.
/// Slack requires the blocks and destinations even for a time-only update.
pub fn drafts_unschedule(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    draft: &ScheduledMessage,
) -> PreparedRequest {
    let mut args = ScheduleMessage::from_draft(draft);
    args.post_at = 0;
    drafts_schedule(client, workspace, &args, Some(&draft.id))
}
pub async fn unschedule_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    draft: &ScheduledMessage,
) -> Result<ScheduledMessage, Error> {
    #[derive(serde::Deserialize)]
    struct Updated {
        draft: ScheduledMessage,
    }
    let response: Updated = decode(
        transport
            .execute(drafts_unschedule(client, workspace, draft))
            .await?,
        "unscheduled draft",
    )?;
    Ok(response.draft)
}
pub async fn fetch_active_draft(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    id: &str,
    limit: u32,
) -> Result<ScheduledMessage, Error> {
    let page = fetch_scheduled_messages(transport, client, workspace, limit.max(100)).await?;
    page.drafts
        .into_iter()
        .find(|d| d.id == id && d.active())
        .ok_or_else(|| {
            Error::Api(
                if page.has_more {
                    "draft_not_loaded"
                } else {
                    "draft_not_active"
                }
                .into(),
            )
        })
}

/// The web client supplies draft_id to the message/file send itself. Slack then
/// retires the draft; deleting it first could lose content if the send failed.
pub fn drafts_send(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    draft: &ScheduledMessage,
) -> Result<PreparedRequest, Error> {
    if !draft.active() {
        return Err(Error::Api("draft_not_active".into()));
    }
    if draft.destinations.len() != 1 {
        return Err(Error::Api("draft_multiple_destinations".into()));
    }
    let channel = draft
        .channel()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Error::Api("draft_missing_destination".into()))?;
    let mut fields = vec![
        ("channel", channel.into()),
        ("draft_id", draft.id.clone()),
        ("client_msg_id", draft.client_msg_id.clone()),
        ("blocks", serde_json::to_string(&draft.blocks).unwrap()),
    ];
    if let Some(root) = draft.thread_ts() {
        fields.push(("thread_ts", root.into()));
    }
    if let Some(broadcast) = draft.destinations[0]
        .get("broadcast")
        .and_then(|v| v.as_bool())
    {
        fields.push((
            if draft.file_ids.is_empty() {
                "reply_broadcast"
            } else {
                "broadcast"
            },
            broadcast.to_string(),
        ));
    }
    let method = if draft.file_ids.is_empty() {
        "chat.postMessage"
    } else {
        fields.push(("files", draft.file_ids.join(",")));
        fields.push(("resharing_aware", "true".into()));
        fields.push(("skip_dlp_user_warning", "false".into()));
        fields.push(("from_share_modal", "false".into()));
        "files.share"
    };
    Ok(client.rest_form(workspace, method, fields))
}
pub async fn send_draft(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    draft: &ScheduledMessage,
) -> Result<String, Error> {
    let value = transport
        .execute(drafts_send(client, workspace, draft)?)
        .await?;
    confirmed_send_timestamp(&value, !draft.file_ids.is_empty())
}
fn confirmed_send_timestamp(value: &serde_json::Value, files: bool) -> Result<String, Error> {
    value
        .get(if files { "file_msg_ts" } else { "ts" })
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::Transport("Slack did not confirm the sent message.".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflict_copy_preserves_content_and_files_but_gets_an_unscheduled_identity() {
        let mut original = ScheduleMessage::new(
            "C1".into(),
            Some("1.2".into()),
            vec![
                serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Keep both 🚀","style":{"bold":true}}]}]}),
            ],
            1800000000,
        );
        original.file_ids = vec!["F1".into()];
        original.destinations[0]["broadcast"] = true.into();
        let copy = original.unscheduled_copy();
        assert_eq!(copy.post_at, 0);
        assert_ne!(copy.client_msg_id, original.client_msg_id);
        assert!(uuid::Uuid::parse_str(&copy.client_msg_id).is_ok());
        assert_eq!(copy.destinations, original.destinations);
        assert_eq!(copy.blocks, original.blocks);
        assert_eq!(copy.file_ids, original.file_ids);
        assert_eq!(original.post_at, 1800000000);
    }
    #[test]
    fn send_requires_a_message_receipt() {
        assert!(confirmed_send_timestamp(&serde_json::json!({"ok":true}), false).is_err());
        assert!(
            confirmed_send_timestamp(&serde_json::json!({"ok":true,"file_msg_ts":""}), true)
                .is_err()
        );
        assert_eq!(
            confirmed_send_timestamp(&serde_json::json!({"file_msg_ts":"123.456"}), true).unwrap(),
            "123.456"
        );
    }
    #[tokio::test]
    async fn broadcast_schedule_offline_is_definitively_not_dispatched_without_holding() {
        let transport = Transport::new("synthetic-cookie").unwrap();
        transport.set_health(crate::slack::transport::Health::Offline);
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic-token".into(),
        };
        let mut message =
            ScheduleMessage::new("C1".into(), Some("100.000001".into()), vec![], 2000000000);
        message.destinations[0]["broadcast"] = true.into();
        for id in [None, Some("DrFixture")] {
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                schedule_message(
                    &transport,
                    &SlackClient::default(),
                    &workspace,
                    &message,
                    id,
                ),
            )
            .await
            .unwrap();
            assert!(
                matches!(result, Err(Error::Api(code)) if code == "draft_mutation_not_sent_offline")
            );
        }
    }
    #[test]
    fn scheduled_mutation_requires_explicit_success_and_preserves_rejection() {
        assert!(
            matches!(decode_scheduled_mutation(serde_json::json!({"ok":false,"error":"time_in_past","draft":{}})),Err(Error::Api(code)) if code=="time_in_past")
        );
        for value in [
            serde_json::json!({"draft":{}}),
            serde_json::json!({"ok":null,"draft":{}}),
        ] {
            assert!(matches!(
                decode_scheduled_mutation(value),
                Err(Error::Transport(_))
            ));
        }
    }
}
