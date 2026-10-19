//! Original socket-update evidence, separate from the legacy typed projection.
use std::sync::Arc;

use serde_json::Value;

use super::super::models::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageUpdateKind {
    Changed,
    Replied,
}

/// Captured only by the realtime parser, before its channel fallback.
///
/// `raw` is the original nested JSON value, including omitted versus explicit
/// null/empty fields. It is not a serialization of `Message`, an HTTP receipt,
/// or authority to update a particular loaded surface. Existing generic socket
/// consumers continue using their legacy typed message and merge behavior.
#[derive(Debug, Clone)]
pub struct MessageUpdate {
    raw: Arc<Value>,
    kind: MessageUpdateKind,
    outer_channel: String,
    typed_ts: Option<String>,
    typed_channel: Option<String>,
}

impl MessageUpdate {
    pub(super) fn capture(
        kind: MessageUpdateKind,
        outer_channel: &str,
        raw: Value,
        decoded: &Message,
    ) -> Self {
        Self {
            raw: Arc::new(raw),
            kind,
            outer_channel: outer_channel.to_owned(),
            typed_ts: decoded.ts.clone(),
            // Freeze the identity of the existing delivered projection without
            // inserting this fallback into the original nested payload.
            typed_channel: Some(
                decoded
                    .channel
                    .clone()
                    .unwrap_or_else(|| outer_channel.to_owned()),
            ),
        }
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn kind(&self) -> MessageUpdateKind {
        self.kind
    }

    pub fn outer_channel(&self) -> &str {
        &self.outer_channel
    }

    pub fn typed_ts(&self) -> Option<&str> {
        self.typed_ts.as_deref()
    }

    pub fn typed_channel(&self) -> Option<&str> {
        self.typed_channel.as_deref()
    }

    /// Tests only the original event/typed identity, not owner, root, content
    /// authority or body equality. A conflicting nested channel remains visible
    /// in `raw` and must be rejected by any consumer requiring agreement.
    pub fn matches_identity(&self, outer_channel: &str, message: &Message) -> bool {
        self.outer_channel == outer_channel
            && self.typed_ts == message.ts
            && self.typed_channel == message.channel
    }
}

#[cfg(test)]
mod tests;
