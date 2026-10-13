//! Receipt comparison only. A live new-cell readback on 2026-10-13 added a
//! top-level rich_text.block_id without changing content. Payloads and original
//! snapshots stay untouched; no other normalization has been established.
use super::*;

pub(super) fn value_matches(left: &CellEditValue, right: &CellEditValue) -> bool {
    match (left, right) {
        (CellEditValue::RichText(left), CellEditValue::RichText(right)) => {
            rich_matches(left, right)
        }
        _ => left == right,
    }
}
pub(super) fn wire_matches(member: &str, left: &Value, right: &Value) -> bool {
    if member == "rich_text" {
        return left
            .as_array()
            .zip(right.as_array())
            .is_some_and(|(left, right)| rich_matches(left, right));
    }
    left == right
}
fn rich_matches(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| root_matches(left, right))
}
fn root_matches(left: &Value, right: &Value) -> bool {
    if left["type"] != "rich_text" || right["type"] != "rich_text" {
        return left == right;
    }
    let (Some(left), Some(right)) = (left.as_object(), right.as_object()) else {
        return false;
    };
    // Only an absent or string server-assigned root ID is presentation metadata.
    // Nested IDs, unknown/opaque nodes and every other attribute stay exact.
    if left.get("block_id").is_some_and(|id| !id.is_string())
        || right.get("block_id").is_some_and(|id| !id.is_string())
    {
        return false;
    }
    let count = |object: &serde_json::Map<String, Value>| {
        object.len() - usize::from(object.contains_key("block_id"))
    };
    count(left) == count(right)
        && left
            .iter()
            .filter(|(key, _)| *key != "block_id")
            .all(|(key, value)| right.get(key) == Some(value))
}
