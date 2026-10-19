//! Retained D8Sp.makeMsgObject/makeThreadObject, bounded ordinary branch.
use super::*;

pub(super) fn unsupported(raw: &Map<String, Value>) -> bool {
    matches!(
        raw.get("type").and_then(Value::as_str),
        Some("channel_topic" | "channel_purpose" | "channel_join" | "channel_leave")
    ) || truthy(raw.get("list_edit"))
        || HIDDEN.iter().any(|key| truthy(raw.get(*key)))
        || ["files", "attachments"].iter().any(|key| {
            raw.get(*key)
                .is_some_and(|value| value.as_array().is_none_or(|a| !a.is_empty()))
        })
        || ["room", "schedule", "file", "comment", "root", "x_files"]
            .iter()
            .any(|key| truthy(raw.get(*key)))
}

pub(super) fn project(raw: &Map<String, Value>) -> BTreeMap<String, Option<Value>> {
    let mut writes = BTreeMap::new();
    // These own keys exist even when the corresponding raw field is omitted.
    for field in [
        "user",
        "subtype",
        "text",
        "thread_ts",
        "latest_reply",
        "reply_users",
        "reply_users_count",
        "replies",
    ] {
        writes.insert(field.into(), raw.get(field).cloned());
    }
    for field in ["blocks", "files", "attachments"] {
        // Native Vec fields represent source undefined/empty as an empty Vec.
        writes.insert(
            field.into(),
            Some(
                raw.get(field)
                    .cloned()
                    .unwrap_or_else(|| Value::Array(vec![])),
            ),
        );
    }
    writes.insert("type".into(), Some(Value::String("message".into())));
    writes.insert("no_display".into(), Some(Value::Bool(false)));
    writes.insert("_hidden_reply".into(), Some(Value::Bool(true)));
    writes.insert(
        "reply_count".into(),
        Some(
            raw.get("reply_count")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or_else(|| Value::from(0)),
        ),
    );
    writes.insert(
        "slackbot_feels".into(),
        Some(
            raw.get("slackbot_feels")
                .filter(|v| truthy(Some(v)))
                .cloned()
                .unwrap_or(Value::Null),
        ),
    );
    for field in ["language", "crm_signal"] {
        writes.insert(
            field.into(),
            raw.get(field).filter(|v| !v.is_null()).cloned(),
        );
    }
    // Source pick/hasIn: absent preserves, present null writes null. Values are
    // shallow replacements; arbitrary incoming extras are not source fields.
    for field in PICKED {
        if let Some(value) = raw.get(*field) {
            writes.insert((*field).into(), Some(value.clone()));
        }
    }
    if let Some(value) = raw.get("reply_user_icons") {
        writes.insert("reply_user_icons".into(), Some(value.clone()));
    }
    // Ordinary branch auxiliary pickBy(identity), not the presence whitelist.
    for (input, output) in [
        ("parent_ts", "parent_ts"),
        ("parent_user_id", "parent_user_id"),
        ("subscribed", "_subscribed"),
        ("unread_count", "_unread_count"),
        ("last_read", "_last_read"),
        ("pinned_info", "pinned_info"),
        ("suppress_notification", "suppress_notification"),
    ] {
        if let Some(value) = raw.get(input).filter(|v| truthy(Some(v))) {
            writes.insert(output.into(), Some(value.clone()));
        }
    }
    if let Some(value) = raw
        .get("source_team")
        .filter(|v| truthy(Some(v)))
        .or_else(|| raw.get("source_team_id").filter(|v| truthy(Some(v))))
    {
        writes.insert("source_team_id".into(), Some(value.clone()));
    }
    if let Some(value) = raw.get("document_mention").filter(|v| truthy(Some(v))) {
        writes.insert("document_mention".into(), Some(value.clone()));
    }
    // Connected, nondesynced ordinary socket policy only; caller owns that
    // admission. Reactions are a separate source store, not message-body fields.
    if let Some(value) = raw.get("reactions") {
        writes.insert("reactions".into(), Some(value.clone()));
    }
    writes
}

// Exact makeMsgObject M whitelist (duplicate slack_list entry collapsed).
const PICKED: &[&str] = &[
    "_ephemeral_type",
    "alert_type",
    "bot_id",
    "bot_profile",
    "client_msg_id",
    "dnd_suppressed",
    "document_comment",
    "document_unfurl",
    "canvas_original_message_ts",
    "edited",
    "hidden",
    "icons",
    "inviter",
    "is_ephemeral",
    "is_locked",
    "is_starred",
    "reminder",
    "locked_by_team_id",
    "task",
    "mrkdwn",
    "metadata",
    "name",
    "no_notifications",
    "old_name",
    "pinned_to",
    "purpose",
    "story_topic",
    "story_description",
    "container",
    "topic",
    "upload",
    "huddle_call_id",
    "canvas_id",
    "canvas_root_thread_channel_id",
    "canvas_root_thread_ts",
    "saved",
    "slack_list",
    "media_reactions_summary",
    "mpdm_move",
    "file_permission_status",
    "list_edit",
    "file_annotation",
    "schedule",
    "is_auto_send",
    "function_execution_id",
    "version",
    "ai_context",
    "streaming_state",
    "is_beyond_free_limit",
    "record_context",
    "canvas_update_ai_summary_blocks",
    "canvas_update_user_ids",
    "canvas_update_section_ids",
    "canvas_update_canvas_id",
    "assistant_app_thread",
    "trigger_id",
    "workflow_id",
    "agentforce_help_request",
    "show_agentforce_feedback",
    "is_muted_for_bot",
    "tab_id",
    "tab_added_user_id",
    "folder_id",
    "folder_ids",
    "folder_item_id",
    "item_added_user_id",
    "item_added_user_ids",
    "agentforce_auto_reply_status",
    "agent_session",
    "welcome_party_user_id",
    "markdown_text",
    "delegate_sender_user_id",
    "show_delegate_attribution",
];
