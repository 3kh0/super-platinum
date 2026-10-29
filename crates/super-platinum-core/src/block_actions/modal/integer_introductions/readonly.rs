//! Closed display admission, not the full Slack display schema.
//! Explicit identity/history is checked by the caller before classification.
use super::{Value, only_keys};

// Local defensive budgets, measured in UTF-8 bytes / entries, not Slack limits.
const TEXT_BYTES: usize = 3000;
const CONTEXT_ENTRIES: usize = 10;

fn text(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        only_keys(value, &["type", "text", "emoji"])
            && value["type"] == "plain_text"
            && value
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| text.len() <= TEXT_BYTES)
            && matches!(value.get("emoji"), None | Some(Value::Bool(true)))
    })
}

pub(super) fn block(value: &Value) -> bool {
    match value["type"].as_str() {
        Some("divider") => only_keys(value, &["type", "block_id"]),
        Some("header" | "section") => {
            only_keys(value, &["type", "block_id", "text"]) && text(value.get("text"))
        }
        Some("context") => {
            only_keys(value, &["type", "block_id", "elements"])
                && value
                    .get("elements")
                    .and_then(Value::as_array)
                    .is_some_and(|elements| {
                        !elements.is_empty()
                            && elements.len() <= CONTEXT_ENTRIES
                            && elements.iter().all(|element| text(Some(element)))
                    })
        }
        _ => false,
    }
}
