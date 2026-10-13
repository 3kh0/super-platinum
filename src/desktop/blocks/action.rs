//! Preserve action metadata without interpreting it as an executable action.

pub(crate) mod fixture;

use serde_json::Value;
use super_platinum_core::block_actions::{
    BlockConfirmation, BlockConfirmationState, BlockElementMetadata, BlockElementSource, BlockText,
    BlockTextFormat,
};

pub(super) fn visible_to(value: &Value, self_user_id: &str) -> bool {
    let Some(restriction) = value.get("visible_to_user_ids") else {
        return true;
    };
    let Some(users) = restriction.as_array() else {
        return false;
    };
    !self_user_id.trim().is_empty()
        && users
            .iter()
            .all(|user| user.as_str().is_some_and(|id| !id.trim().is_empty()))
        && users.iter().any(|user| user.as_str() == Some(self_user_id))
}

pub(crate) fn metadata(
    value: &Value,
    parent_block_id: Option<&str>,
    source: BlockElementSource,
) -> BlockElementMetadata {
    BlockElementMetadata {
        source,
        block_id: parent_block_id.map(str::to_owned),
        action_id: string(value, "action_id"),
        value: string(value, "value"),
        confirm: match value.get("confirm") {
            None => BlockConfirmationState::Missing,
            Some(confirm) if confirm.is_object() => {
                BlockConfirmationState::Parsed(BlockConfirmation {
                    title: text(confirm.get("title")),
                    text: text(confirm.get("text")),
                    confirm: text(confirm.get("confirm")),
                    deny: text(confirm.get("deny")),
                    style: string(confirm, "style"),
                })
            }
            Some(_) => BlockConfirmationState::Invalid,
        },
        accessibility_label: string(value, "accessibility_label"),
    }
}

fn string(value: &Value, name: &str) -> Option<String> {
    value.get(name)?.as_str().map(str::to_owned)
}

fn text(value: Option<&Value>) -> Option<BlockText> {
    let value = value?;
    Some(BlockText {
        format: match value.get("type")?.as_str()? {
            "plain_text" => BlockTextFormat::PlainText,
            "mrkdwn" => BlockTextFormat::Mrkdwn,
            other => BlockTextFormat::Unknown(other.to_owned()),
        },
        text: string(value, "text")?,
        emoji: value.get("emoji").and_then(Value::as_bool),
        verbatim: value.get("verbatim").and_then(Value::as_bool),
    })
}

#[cfg(test)]
mod tests;
