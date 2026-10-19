//! Identity admission for typed socket replies, independent of HTTP page evidence.
use super::{Message, Page, timestamp};
use serde_json::Value;

impl Page {
    /// Check whether a typed socket message can belong to this acquired thread.
    ///
    /// Requires an explicit matching channel and a valid nonroot reply identity.
    /// Only unconditional hidden/deletion predicates are checked here. The caller
    /// still owns the actual observed `Projection`, duplicate/visit guards and
    /// newer deletion evidence. This predicate never mutates the Page or treats
    /// a typed event as a new HTTP receipt or an acknowledgement of a local send.
    pub fn accepts_live_reply(&self, message: &Message) -> bool {
        let (Some(root), Some(ts), Some(thread)) = (
            self.target.root_ts(),
            message.ts.as_deref(),
            message.thread_ts.as_deref(),
        ) else {
            return false;
        };
        message.channel.as_deref() == Some(self.target.channel_id())
            && timestamp(ts)
            && timestamp(thread)
            && ts != root
            && thread == root
            && !self
                .raw
                .get("deleted")
                .and_then(Value::as_array)
                .is_some_and(|deleted| deleted.iter().any(|value| value.as_str() == Some(ts)))
            && !matches!(
                message.subtype.as_deref(),
                Some("tombstone" | "message_deleted")
            )
            && ![
                "isNonExistent",
                "no_display",
                "_hidden_stub",
                "is_deleted",
                "is_tombstoned",
            ]
            .iter()
            .any(|key| truthy(message.extra.get(*key)))
    }
}

// Same unconditional JS truthiness used by Projection and reaction admission.
// Experiment-dependent list_edit visibility belongs to the observed caller.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests;
