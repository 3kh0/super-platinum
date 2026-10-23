//! Native capture content is explicitly typed. Delta operations never masquerade
//! as rich blocks, and capture alone does not grant encoding or send authority.
use serde_json::Value;
use super_platinum_core::{slack::Error, slack_lists::comments::PlainDelta};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    RichBlocks(Vec<Value>),
    PlainDelta(PlainDelta),
}

fn unavailable() -> Error {
    Error::Api("list_comment_edit_native_capture_unavailable".into())
}

impl Content {
    pub(crate) fn representation(&self) -> &'static str {
        match self {
            Self::RichBlocks(_) => "rich-blocks",
            Self::PlainDelta(_) => "plain-delta",
        }
    }

    pub(crate) fn rich_blocks(&self) -> Option<&[Value]> {
        match self {
            Self::RichBlocks(blocks) => Some(blocks),
            Self::PlainDelta(_) => None,
        }
    }

    pub(crate) fn plain_delta(&self) -> Option<&PlainDelta> {
        match self {
            Self::RichBlocks(_) => None,
            Self::PlainDelta(delta) => Some(delta),
        }
    }

    /// Decode only the selected mount representation. A null alternate key is
    /// still ambiguous. Legacy rich captures alone may omit `pending`.
    pub(crate) fn decode_capture(value: &Value, expected: &str) -> Result<(Self, bool), Error> {
        let object = value.as_object().ok_or_else(unavailable)?;
        let pending = match object.get("pending") {
            Some(Value::Bool(value)) => *value,
            None if expected == "rich-blocks" => false,
            _ => return Err(unavailable()),
        };
        let content = match expected {
            "rich-blocks" if !object.contains_key("delta") => Self::RichBlocks(
                object
                    .get("blocks")
                    .and_then(Value::as_array)
                    .ok_or_else(unavailable)?
                    .clone(),
            ),
            "plain-delta" if !object.contains_key("blocks") => {
                let delta = object
                    .get("delta")
                    .and_then(Value::as_object)
                    .ok_or_else(unavailable)?;
                if delta.len() != 2
                    || delta.get("mode").and_then(Value::as_str) != Some("plain-text")
                {
                    return Err(unavailable());
                }
                let ops = delta
                    .get("ops")
                    .and_then(Value::as_array)
                    .ok_or_else(unavailable)?;
                Self::PlainDelta(PlainDelta::capture(ops.clone()).map_err(|_| unavailable())?)
            }
            _ => return Err(unavailable()),
        };
        Ok((content, pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_preserves_rich_opaque_values_and_legacy_pending() {
        let blocks = json!([{"type":"rich_text","opaque":{"original":[2,1]}}]);
        let (content, pending) =
            Content::decode_capture(&json!({"blocks":blocks}), "rich-blocks").unwrap();
        assert!(!pending);
        assert_eq!(content.rich_blocks().unwrap(), blocks.as_array().unwrap());
        assert_eq!(content.representation(), "rich-blocks");
        assert!(content.plain_delta().is_none());
    }

    #[test]
    fn content_plain_requires_explicit_mode_and_pending_without_converting_ops() {
        let ops = json!([{"insert":"original\n"}]);
        let raw = json!({"delta":{"mode":"plain-text","ops":ops},"pending":true});
        let (content, pending) = Content::decode_capture(&raw, "plain-delta").unwrap();
        assert!(pending);
        assert_eq!(
            content.plain_delta().unwrap().ops(),
            ops.as_array().unwrap()
        );
        assert_eq!(content.representation(), "plain-delta");
        assert!(content.rich_blocks().is_none());
        let mut missing = raw.clone();
        missing.as_object_mut().unwrap().remove("pending");
        assert!(Content::decode_capture(&missing, "plain-delta").is_err());
    }

    #[test]
    fn content_refuses_ambiguous_wrong_mode_and_malformed_operations() {
        for raw in [
            json!({"blocks":[],"delta":null}),
            json!({"blocks":null,"delta":{"mode":"plain-text","ops":[]},"pending":false}),
            json!({"delta":{"mode":"rich-text","ops":[]},"pending":false}),
            json!({"delta":{"mode":"plain-text","ops":[],"extra":true},"pending":false}),
            json!({"delta":{"mode":"plain-text","ops":[{"delete":1}]},"pending":false}),
            json!({"blocks":[],"pending":null}),
            json!({"blocks":{},"pending":false}),
            json!({"delta":{"mode":"plain-text","ops":{}},"pending":false}),
        ] {
            assert!(
                Content::decode_capture(&raw, "rich-blocks").is_err(),
                "{raw}"
            );
            assert!(
                Content::decode_capture(&raw, "plain-delta").is_err(),
                "{raw}"
            );
        }
        assert!(Content::decode_capture(&json!({"blocks":[]}), "unknown").is_err());
    }
}
