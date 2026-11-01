//! Passive inline decoration preserving the complete existing snippet text.
use crate::model::RichNode;

pub(super) struct Preview {
    pub(super) text: String,
    pub(super) rich: Option<Vec<RichNode>>,
}
impl Preview {
    pub(super) fn plain(text: String) -> Self {
        Self { text, rich: None }
    }
}

pub(super) fn first(nodes: Vec<RichNode>, expected: &str) -> Option<Vec<RichNode>> {
    let first = nodes
        .into_iter()
        .find(|n| !n.plain_text().trim().is_empty())?;
    let mut budget = 64;
    let mut styled = false;
    let nodes = match first {
        RichNode::Paragraph(nodes) => nodes,
        node => vec![node],
    };
    let result = nodes
        .into_iter()
        .map(|n| inline(n, &mut budget, &mut styled))
        .collect::<Option<Vec<_>>>()?;
    // plain_text trims a node's trailing whitespace. Flatten the paragraph
    // once so spaces between styled leaves remain part of the comparison.
    let text = RichNode::Paragraph(result.clone()).plain_text();
    // Native first-line/64-node policy, not Source truncation/normalization.
    // Any unsupported/mismatched shape keeps the entire existing string path.
    (styled && !text.contains('\n') && text == expected).then_some(result)
}
pub(super) fn inline(node: RichNode, budget: &mut usize, styled: &mut bool) -> Option<RichNode> {
    *budget = budget.checked_sub(1)?;
    match node {
        RichNode::Text(text) => Some(RichNode::Text(text)),
        RichNode::StyledText {
            text,
            bold,
            italic,
            strike,
            ..
        } => {
            *styled |= bold || italic || strike;
            Some(RichNode::StyledText {
                text,
                bold,
                italic,
                strike,
                code: false,
            })
        }
        RichNode::Underlined(child) => {
            *styled = true;
            Some(RichNode::Underlined(Box::new(inline(
                *child, budget, styled,
            )?)))
        }
        RichNode::Emoji { name, glyph } => Some(RichNode::Emoji { name, glyph }),
        _ => None,
    }
}
