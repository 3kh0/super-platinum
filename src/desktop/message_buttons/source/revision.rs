//! Exact render-source freshness, separate from wire action identity and the
//! narrower core revision. Capture owns the snapshot; checks borrow message data.
use super::Message;
use serde_json::{Value, json};

pub(super) fn capture(message: &Message) -> Value {
    json!({"blocks":message.blocks,"edited":message.edited,"bot_id":message.bot_id,
        "bot_profile":message.bot_profile,"thread_ts":message.thread_ts,"channel":message.channel,
        "subtype":message.subtype,"type":message.kind,"user":message.user,
        "function_execution_id":message.extra.get("function_execution_id"),
        "is_ephemeral":message.extra.get("is_ephemeral"),"app_id":message.extra.get("app_id")})
}

/// Equivalent to `capture(message) == *snapshot`, without cloning the blocks.
/// Keep profile serialization: its flattened fields (including unknown nested
/// values and colliding keys) must retain the serializer's exact semantics.
pub(super) fn matches(message: &Message, snapshot: &Value) -> bool {
    snapshot
        .as_object()
        .is_some_and(|object| object.len() == 12)
        && snapshot.get("blocks").and_then(Value::as_array) == Some(&message.blocks)
        && optional_value(snapshot, "edited", message.edited.as_ref())
        && optional_string(snapshot, "bot_id", message.bot_id.as_deref())
        && optional_string(snapshot, "thread_ts", message.thread_ts.as_deref())
        && optional_string(snapshot, "channel", message.channel.as_deref())
        && optional_string(snapshot, "subtype", message.subtype.as_deref())
        && optional_string(snapshot, "type", message.kind.as_deref())
        && optional_string(snapshot, "user", message.user.as_deref())
        && optional_value(
            snapshot,
            "function_execution_id",
            message.extra.get("function_execution_id"),
        )
        && optional_value(snapshot, "is_ephemeral", message.extra.get("is_ephemeral"))
        && optional_value(snapshot, "app_id", message.extra.get("app_id"))
        && serde_json::to_value(&message.bot_profile)
            .is_ok_and(|profile| snapshot.get("bot_profile") == Some(&profile))
}

fn optional_value(snapshot: &Value, key: &str, value: Option<&Value>) -> bool {
    snapshot.get(key) == Some(value.unwrap_or(&Value::Null))
}

fn optional_string(snapshot: &Value, key: &str, value: Option<&str>) -> bool {
    match value {
        Some(value) => snapshot.get(key).and_then(Value::as_str) == Some(value),
        None => snapshot.get(key) == Some(&Value::Null),
    }
}

#[cfg(test)]
mod tests;
