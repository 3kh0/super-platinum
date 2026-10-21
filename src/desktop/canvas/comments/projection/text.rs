use crate::model::RichNode;
use super_platinum_core::canvas::comments::display::decode_entities;

/// Decode source text after the Slack token parser has assigned identities.
/// In particular, `&lt;@U1&gt;` stays inert text, never a new member action.
pub(super) fn decode(nodes: &mut [RichNode]) {
    for node in nodes {
        match node {
            RichNode::Text(text) | RichNode::StyledText { text, .. } | RichNode::Code(text) => {
                *text = decode_entities(text);
            }
            RichNode::Link { label, .. } => *label = decode_entities(label),
            RichNode::Paragraph(children) | RichNode::Quote(children) => decode(children),
            // Mention labels come from workspace metadata, not the source text;
            // URLs and member/channel identities retain their original bytes.
            _ => (),
        }
    }
}
