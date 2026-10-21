use super::*;

pub(super) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
fn unavailable(raw: &Value) -> bool {
    [
        "isUnknown",
        "isNonExistent",
        "isEKMError",
        "is_ephemeral",
        "no_display",
        "_hidden_stub",
        "is_deleted",
        "is_tombstoned",
    ]
    .iter()
    .any(|k| truthy(raw.get(*k)))
}
// Retained h3YX automated classification. The bounded ordinary-reply slice
// deliberately excludes its special channel/room/broadcast whitelist as well.
const AUTOMATED: &[&str] = &[
    "agent_view_added",
    "code_channel_review_submitted",
    "code_channel_lifecycle",
    "channel_join",
    "channel_leave",
    "channel_topic",
    "channel_purpose",
    "channel_archive",
    "channel_unarchive",
    "channel_convert_to_private",
    "channel_convert_to_public",
    "channel_convert_to_linked_private",
    "channel_convert_to_linked_private_read",
    "channel_convert_to_linked_private_edit",
    "channel_convert_to_linked_private_transfer",
    "channel_convert_shared",
    "group_join",
    "group_leave",
    "group_topic",
    "group_purpose",
    "group_archive",
    "group_unarchive",
    "group_name",
    "channel_name",
    "play_sound",
    "pinned_item",
    "unpinned_item",
    "sh_room_shared",
    "sh_room_created",
    "bot_enable",
    "bot_disable",
    "bot_add",
    "bot_remove",
    "bot_updated",
    "reminder_add",
    "reminder_delete",
    "reply_broadcast",
    "app_conversation_join",
    "app_conversation_leave",
    "app_conversation_created",
    "channel_posting_permissions",
    "retention_threshold",
    "joiner_notification",
    "joiner_notification_for_inviter",
    "prejoin_dm_welcome_party_notification",
    "slack_connect_dm_acceptance",
    "canvas",
    "mpdm_move",
    "canvas_open_by_default",
    "canvas_editing_permissions_changed",
    "channel_canvas_updated",
    "channel_canvas_updated_unread_ignore",
    "tabbed_canvas_updated",
    "tabbed_canvas_updated_unread_ignore",
    "automatic_ai_huddle_notes_enabled",
    "automatic_ai_huddle_notes_disabled",
    "workflow_channel_opt_in_enabled",
    "workflow_channel_opt_in_disabled",
    "welcome_party",
];
pub(super) fn ordinary(message: &Message, target: &Target, user: &str) -> Result<(), ListError> {
    let raw = serde_json::to_value(message).map_err(|_| ListError("The comment is malformed."))?;
    let subtype = message.subtype.as_deref().unwrap_or("");
    if message.user.as_deref() != Some(user)
        || !message.ts.as_deref().is_some_and(timestamp)
        || target.root_ts().is_none()
        || message.ts.as_deref() == target.root_ts()
        || message.thread_ts.as_deref() != target.root_ts()
        || message
            .channel
            .as_deref()
            .is_some_and(|c| c != target.channel_id())
        || !matches!(message.kind.as_deref(), None | Some("message"))
        || unavailable(&raw)
        || AUTOMATED.contains(&subtype)
        || matches!(
            subtype,
            "tombstone"
                | "retention"
                | "bookmark_created"
                | "bookmark_removed"
                | "huddle_thread"
                | "moderated"
                | "thread_hidden"
                | "thread_broadcast"
        )
    {
        return Err(ListError(
            "Only an ordinary own loaded reply can be deleted here.",
        ));
    }
    Ok(())
}
pub(super) fn deleted(raw: &Value, ts: &str) -> bool {
    raw["deleted"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(ts)))
}
pub(super) fn original<'a>(http: &'a Value, ts: &str) -> Result<&'a Value, ListError> {
    let rows = http["messages"]
        .as_array()
        .ok_or(ListError("The original HTTP comments are unavailable."))?;
    let mut rows = rows.iter().filter(|v| v["ts"].as_str() == Some(ts));
    let value = rows
        .next()
        .ok_or(ListError("The original HTTP comment is absent."))?;
    if rows.next().is_some() {
        return Err(ListError("The original HTTP comment is ambiguous."));
    }
    Ok(value)
}
pub(super) fn root_available(page: &Page) -> Result<(), ListError> {
    let root = page
        .root()
        .ok_or(ListError("The original comment root is unavailable."))?;
    let raw =
        serde_json::to_value(root).map_err(|_| ListError("The original root is malformed."))?;
    if !item_root(root, page.target())
        || root.ts.as_deref() != page.target().root_ts()
        || root
            .channel
            .as_deref()
            .is_some_and(|v| v != page.target().channel_id())
        || unavailable(&raw)
        || truthy(raw.get("is_locked"))
        || deleted(page.raw(), page.target().root_ts().unwrap_or(""))
    {
        return Err(ListError(
            "The original comment root does not allow deletion.",
        ));
    }
    Ok(())
}
