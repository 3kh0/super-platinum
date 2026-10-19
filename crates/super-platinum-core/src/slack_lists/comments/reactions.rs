//! Realtime reaction deltas on an already acquired List comment page.
use super::Page;
use serde_json::Value;

impl Page {
    /// Apply an original-channel delta to a known live reply under this root.
    ///
    /// This checks the unconditional hidden/deletion predicates. The caller must
    /// additionally require visibility under its actual observed `Projection`:
    /// assignment-dependent `list_edit` filtering is not a fact held by a Page.
    /// Only typed reactions change; `raw()` remains the original source receipt.
    /// The result uses the shared message reaction reducer's return semantics.
    pub fn apply_reaction(
        &mut self,
        channel: &str,
        ts: &str,
        user: &str,
        name: &str,
        added: bool,
    ) -> bool {
        let Some(root) = self.target.root_ts() else {
            return false;
        };
        if channel != self.target.channel_id()
            || ts == root
            || user.is_empty()
            || name.is_empty()
            || self
                .raw
                .get("deleted")
                .and_then(Value::as_array)
                .is_some_and(|deleted| deleted.iter().any(|value| value.as_str() == Some(ts)))
        {
            return false;
        }
        let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(ts))
        else {
            return false;
        };
        if message.thread_ts.as_deref() != Some(root)
            || message.channel.as_deref().is_some_and(|c| c != channel)
            || matches!(
                message.subtype.as_deref(),
                Some("tombstone" | "message_deleted")
            )
            || [
                "isNonExistent",
                "no_display",
                "_hidden_stub",
                "is_deleted",
                "is_tombstoned",
            ]
            .iter()
            .any(|key| truthy(message.extra.get(*key)))
        {
            return false;
        }
        crate::state::apply_message_reaction(message, user, name, added)
    }
}

// Same JS truthiness as Projection's unconditional hidden-reply predicates.
// Do not construct a synthetic release-assignment observation to call visible().
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests;
