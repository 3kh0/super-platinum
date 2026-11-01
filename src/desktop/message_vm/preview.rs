//! Feed-owned reading only. No canonical/history, action or media authority.
use super::*;

/// Reuse the inert reading whitelist for an already projected supplied body.
/// No additional parsing, media registration, or action ownership is acquired.
pub(crate) fn nodes(nodes: Vec<RichNode>) -> Vec<RichNode> {
    nodes.into_iter().map(|node| text_only(node, 0)).collect()
}

pub(crate) fn body(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
) -> Vec<RichNode> {
    // Block translation may register media. This detached, credentialless
    // registry has no store or loader and is dropped here; none of its handles
    // may escape into the shell's registry, periodic sweep or rendered DOM.
    let detached = MediaRegistry::default();
    message_body(workspace, message, &detached, None)
        .into_iter()
        .map(|node| text_only(node, 0))
        .collect()
}

fn text_only(node: RichNode, depth: usize) -> RichNode {
    if depth > 64 {
        return RichNode::Text("More content is available in the thread.".into());
    }
    let children =
        |nodes: Vec<RichNode>| nodes.into_iter().map(|n| text_only(n, depth + 1)).collect();
    match node {
        RichNode::Canvas(c) => RichNode::Text(c.title),
        RichNode::FileMention { fallback, .. } => RichNode::Text(if fallback.is_empty() {
            "File".into()
        } else {
            fallback
        }),
        RichNode::Link { label, url } => RichNode::Text(if label.is_empty() { url } else { label }),
        RichNode::UserMention { label, .. }
        | RichNode::GroupMention { label, .. }
        | RichNode::ChannelMention { label, .. }
        | RichNode::Button { label, .. } => RichNode::Text(label),
        RichNode::Media { name, .. } => RichNode::Text(name),
        RichNode::EmojiImage { name, .. } => RichNode::Text(format!(":{name}:")),
        RichNode::InlineImage { alt, .. } => RichNode::Text(alt),
        RichNode::ImageBlock { alt, title, .. } => RichNode::Paragraph(vec![
            RichNode::Text(title.unwrap_or_default()),
            RichNode::Text(alt),
        ]),
        RichNode::Underlined(c) => RichNode::Underlined(Box::new(text_only(*c, depth + 1))),
        RichNode::Paragraph(c) => RichNode::Paragraph(children(c)),
        RichNode::Quote(c) => RichNode::Quote(children(c)),
        RichNode::Header(c) => RichNode::Header(children(c)),
        RichNode::Context(c) => RichNode::Context(children(c)),
        // The action container itself has no interaction in the shared renderer.
        RichNode::Actions(c) => RichNode::Actions(children(c)),
        RichNode::Section {
            text,
            fields,
            accessory,
        } => RichNode::Section {
            text: children(text),
            fields: fields.into_iter().map(children).collect(),
            accessory: accessory.map(|a| Box::new(text_only(*a, depth + 1))),
        },
        RichNode::List {
            ordered,
            indent,
            offset,
            items,
        } => RichNode::List {
            ordered,
            indent,
            offset,
            items: items.into_iter().map(children).collect(),
        },
        // Exhaustive whitelist: a new RichNode variant requires an explicit
        // decision here before it can be displayed in this inert feed body.
        RichNode::Text(_)
        | RichNode::StyledText { .. }
        | RichNode::SearchText { .. }
        | RichNode::Code(_)
        | RichNode::Emoji { .. }
        | RichNode::Divider => node,
    }
}

#[cfg(test)]
mod tests;
