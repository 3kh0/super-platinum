//! Search snippets use the visible typed attachment content when the body is blank.
use crate::model::{MessageVm, RichNode};

pub(super) fn text(message: &MessageVm) -> String {
    let body = crate::view::rich::message_plain_text(message);
    if !without_highlights(&body).trim().is_empty() {
        return body;
    }
    let mut parts = Vec::new();
    for attachment in &message.attachments {
        let start = parts.len();
        push_nodes(&mut parts, &attachment.pretext);
        if let Some(title) = &attachment.title {
            push(&mut parts, title.clone());
        }
        push_nodes(&mut parts, &attachment.body);
        for (title, nodes, _) in &attachment.fields {
            let value = nodes_text(nodes);
            let title = without_highlights(title);
            let value = without_highlights(&value);
            push(
                &mut parts,
                match (title.trim(), value.trim()) {
                    ("", value) => value.into(),
                    (title, "") => title.into(),
                    (title, value) => format!("{title}: {value}"),
                },
            );
        }
        push_nodes(&mut parts, &attachment.files);
        if parts.len() == start && (attachment.image.is_some() || attachment.thumb.is_some()) {
            parts.push("Image attachment".into());
        }
    }
    parts.join("\n")
}

fn without_highlights(value: &str) -> String {
    value.replace(['\u{e000}', '\u{e001}'], "")
}
fn nodes_text(nodes: &[RichNode]) -> String {
    nodes
        .iter()
        .map(RichNode::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}
fn push_nodes(parts: &mut Vec<String>, nodes: &[RichNode]) {
    push(parts, nodes_text(nodes));
}
fn push(parts: &mut Vec<String>, value: String) {
    let value = without_highlights(&value);
    if !value.trim().is_empty() {
        parts.push(value.trim().into());
    }
}

mod fixture;
pub(super) use fixture::install;

#[cfg(test)]
mod tests;
