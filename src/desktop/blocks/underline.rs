//! Observed snippet underline decorates existing typed leaves, never HTML.
use super::{RichNode, Value};
#[cfg(test)]
mod tests;

pub(super) fn apply(raw: &Value, nodes: Vec<RichNode>) -> Vec<RichNode> {
    // The original snippet exporter decides outer versus anchor-inner styles
    // before this boundary. A typed mention with no underline stays unchanged.
    if matches!(
        raw.get("type").and_then(Value::as_str),
        Some("text" | "link" | "user" | "channel")
    ) && raw.get("style").and_then(|s| s.get("underline")) == Some(&Value::Bool(true))
    {
        nodes
            .into_iter()
            .map(|node| RichNode::Underlined(Box::new(node)))
            .collect()
    } else {
        nodes
    }
}

type LineParts = (String, Box<dyn Fn(String) -> RichNode>);

/// Preserve the existing section newline behavior while carrying decoration
/// to each rebuilt inline. Never wrap paragraphs or change token identities.
pub(super) fn line_parts(node: RichNode) -> Result<LineParts, RichNode> {
    match node {
        RichNode::Underlined(inner) => match line_parts(*inner) {
            Ok((text, rebuild)) => Ok((
                text,
                Box::new(move |piece| RichNode::Underlined(Box::new(rebuild(piece)))),
            )),
            Err(inner) => Err(RichNode::Underlined(Box::new(inner))),
        },
        RichNode::Text(text) if text.contains('\n') => Ok((text, Box::new(RichNode::Text))),
        RichNode::StyledText {
            text,
            bold,
            italic,
            strike,
            code,
        } if text.contains('\n') => Ok((
            text,
            Box::new(move |piece| RichNode::StyledText {
                text: piece,
                bold,
                italic,
                strike,
                code,
            }),
        )),
        other => Err(other),
    }
}
