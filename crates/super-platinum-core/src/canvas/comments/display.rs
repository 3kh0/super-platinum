//! QuipCommentRoot fallback body selection (NKsY / lby4 / iub5 / ZgSJ).
//! This borrows an already-owned message; it grants no channel, thread, read or
//! mutation authority. Canonical first-file link override is not supplied here.
use crate::slack::models::Message;
use serde_json::Value;
mod entities;
pub use entities::decode_entities;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RootBody<'a> {
    Hidden,
    /// A file-bearing root has an unsupported native fallback value. Paint no
    /// body rather than coercing it or traversing generic file/attachment data.
    /// This is a native refusal, not an inferred upstream rendering result.
    Unavailable,
    /// Nonempty original top-level blocks win even if their renderer emits no
    /// content. Do not fall back to text after an empty/unsupported block render.
    Blocks(&'a [Value]),
    Text {
        text: &'a str,
        /// False selects Slack's NOMRKDWN mode: formatting markers are literal,
        /// while entities, typed mentions, links and emoji still use text parsing.
        mrkdwn: bool,
    },
}

/// Select original blocks/text when no eligible canonical file override exists.
/// `None` preserves the existing path for nonroots and malformed fileless roots;
/// it is not permission to adopt an unvalidated message.
/// No nested document_comment rich-text/request-snippet mapping is inferred.
pub fn root_body(message: &Message) -> Option<RootBody<'_>> {
    if !super::root(message) {
        return None;
    }
    if message.extra.get("document_comment")?.get("is_visible") == Some(&Value::Bool(false)) {
        return Some(RootBody::Hidden);
    }
    if !message.blocks.is_empty() {
        return Some(RootBody::Blocks(&message.blocks));
    }
    let text = match message.text.as_deref().filter(|text| !text.is_empty()) {
        Some(text) => text,
        None => match message.extra.get("markdown_text") {
            None | Some(Value::Null) => "",
            Some(Value::String(text)) => text,
            // The source's truthy fallback is a text-layer input. Do not invent
            // string coercion for an unsupported native decoded field shape.
            _ if !message.files.is_empty() => return Some(RootBody::Unavailable),
            _ => return None,
        },
    };
    Some(RootBody::Text {
        text,
        mrkdwn: message.extra.get("mrkdwn") != Some(&Value::Bool(false)),
    })
}

#[cfg(test)]
mod tests;
