//! Passive metadata on the same native feed body, never edit/action authority.
use crate::{model::RichNode, state::ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::slack::models::{Message, ThreadViewItem};

// Defensive native work bounds, not Slack schema or Source renderer limits.
const MAX_BLOCKS: usize = 64;
const MAX_SECTIONS: usize = 128;
const MAX_LEAVES: usize = 1024;
const MAX_PARAGRAPHS: usize = 512;
const MAX_TEXT_BYTES: usize = 64 * 1024;

fn timestamp(value: &str) -> bool {
    if value.len() > 32 {
        return false;
    }
    let Some((seconds, fraction)) = value.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && seconds
            .parse::<u64>()
            .is_ok_and(|n| n > 0 && n <= i64::MAX as u64)
        && !fraction.is_empty()
        && fraction.len() <= 6
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

fn ordinary(message: &Message) -> bool {
    message.kind.as_deref() == Some("message")
        && message.user.as_deref().is_some_and(|user| !user.is_empty())
        && message.subtype.is_none()
        && message.bot_id.as_deref().is_none_or(str::is_empty)
        && message.bot_profile.is_none()
        && message.message.is_none()
        && message.files.is_empty()
        && message.attachments.is_empty()
        && ["document_comment", "workflow_id", "ai_context"]
            .iter()
            .all(|key| message.extra.get(*key).is_none_or(Value::is_null))
        && !super_platinum_core::channel_threads::hidden(message)
}

fn owned(snapshot: &ShellState, item: &ThreadViewItem, message: &Message) -> bool {
    let Some(team) = snapshot.core.active_team.as_deref() else {
        return false;
    };
    let Some(feed) = crate::thread_feed::visible_state(snapshot) else {
        return false;
    };
    // Only references from this render's current feed qualify, not an equal DTO.
    if !feed.items.iter().any(|row| std::ptr::eq(row, item)) {
        return false;
    }
    let root = &item.root_msg;
    let Some(channel) = root.channel.as_deref().filter(|id| !id.is_empty()) else {
        return false;
    };
    let Some(root_ts) = root.ts.as_deref().filter(|ts| timestamp(ts)) else {
        return false;
    };
    let Some(workspace) = snapshot.core.workspaces.get(team) else {
        return false;
    };
    if root.thread_ts.as_deref().is_some_and(|ts| ts != root_ts)
        || root.team.as_deref().is_some_and(|id| id != team)
        || message.channel.as_deref().is_some_and(|id| id != channel)
        || message.thread_ts.as_deref().is_some_and(|ts| ts != root_ts)
        || message.team.as_deref().is_some_and(|id| id != team)
        || super_platinum_core::channel_threads::hidden(root)
        || workspace
            .messages
            .get(channel)
            .is_some_and(|bag| bag.was_deleted(root_ts))
    {
        return false;
    }
    let Some(ts) = message.ts.as_deref().filter(|ts| timestamp(ts)) else {
        return false;
    };
    if workspace
        .messages
        .get(channel)
        .is_some_and(|bag| bag.was_deleted(ts))
    {
        return false;
    }
    std::ptr::eq(root, message)
        || super::replies::selected(snapshot, item)
            .into_iter()
            .any(|reply| std::ptr::eq(reply, message))
}

fn supported_style(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Object(style)) => style.iter().all(|(key, value)| {
            matches!(key.as_str(), "bold" | "italic" | "strike" | "code") && value.is_boolean()
        }),
        _ => false,
    }
}

fn expected_paragraphs(message: &Message) -> Option<Vec<String>> {
    if message.blocks.is_empty() || message.blocks.len() > MAX_BLOCKS {
        return None;
    }
    let mut result = Vec::new();
    let (mut sections, mut leaves, mut bytes) = (0usize, 0usize, 0usize);
    let mut final_newline = false;
    for block in &message.blocks {
        if block.get("type").and_then(Value::as_str) != Some("rich_text") {
            return None;
        }
        let elements = block.get("elements")?.as_array()?;
        if elements.is_empty() {
            return None;
        }
        for section in elements {
            sections += 1;
            if sections > MAX_SECTIONS
                || section.get("type").and_then(Value::as_str) != Some("rich_text_section")
            {
                return None;
            }
            let elements = section.get("elements")?.as_array()?;
            if elements.is_empty() {
                return None;
            }
            let mut text = String::new();
            for leaf in elements {
                leaves += 1;
                if leaves > MAX_LEAVES
                    || leaf.get("type").and_then(Value::as_str) != Some("text")
                    || !supported_style(leaf.get("style"))
                {
                    return None;
                }
                let value = leaf.get("text")?.as_str()?;
                bytes = bytes.checked_add(value.len())?;
                if bytes > MAX_TEXT_BYTES || value.contains('\r') {
                    return None;
                }
                text.push_str(value);
            }
            if text.is_empty() {
                return None;
            }
            final_newline = text.ends_with('\n');
            // Match the existing section splitter; no decoding or parsing here.
            for line in text.split('\n').filter(|line| !line.is_empty()) {
                if result.len() == MAX_PARAGRAPHS {
                    return None;
                }
                result.push(line.to_owned());
            }
        }
    }
    // Source would place its label on the trailing empty line that native
    // paragraph splitting drops. Do not guess that placement in this subset.
    (!final_newline && !result.is_empty()).then_some(result)
}

fn corresponds(message: &Message, nodes: &[RichNode]) -> bool {
    let Some(expected) = expected_paragraphs(message) else {
        return false;
    };
    if expected.len() != nodes.len() {
        return false;
    }
    expected.iter().zip(nodes).all(|(expected, node)| {
        let RichNode::Paragraph(children) = node else {
            return false;
        };
        if children.is_empty() {
            return false;
        }
        let mut text = String::new();
        for child in children {
            let (RichNode::Text(value) | RichNode::StyledText { text: value, .. }) = child else {
                return false;
            };
            if text.len().saturating_add(value.len()) > MAX_TEXT_BYTES {
                return false;
            }
            text.push_str(value);
        }
        text == *expected
    })
}

fn qualifies(
    snapshot: &ShellState,
    item: &ThreadViewItem,
    message: &Message,
    nodes: &[RichNode],
) -> bool {
    let edited = message
        .edited
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|edited| edited.get("ts"))
        .and_then(Value::as_str)
        .is_some_and(timestamp);
    edited && ordinary(message) && owned(snapshot, item, message) && corresponds(message, nodes)
}

pub(super) fn render(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    item: &ThreadViewItem,
    message: &Message,
    nodes: &[RichNode],
) -> Element {
    if qualifies(snapshot, item, message, nodes)
        && let Some((RichNode::Paragraph(children), prefix)) = nodes.split_last()
    {
        return rsx! {
            for node in prefix {{super::super::rich::rich_node(node, snapshot.media_epoch, state)}}
            p {
                for node in children {{super::super::rich::rich_node(node, snapshot.media_epoch, state)}}
                span {class:"message-edited",dir:"ltr","\u{a0}(edited)\u{a0}"}
            }
        };
    }
    rsx! {for node in nodes {{super::super::rich::rich_node(node, snapshot.media_epoch, state)}}}
}

#[cfg(test)]
mod tests;
