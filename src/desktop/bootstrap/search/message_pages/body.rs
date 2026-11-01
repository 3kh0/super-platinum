//! Supplied search-page reading, never a shared canonical body or action source.
use super::*;
use crate::state::RichNode;

#[derive(Clone, Default)]
pub(in crate::bootstrap::search) struct Supplied {
    pub coherent: bool,
    pub nodes: Vec<RichNode>,
    pub reactions: Vec<crate::state::ReactionVm>,
    pub ordinary_root: bool,
    pub reply_count: Option<u32>,
}
pub(in crate::bootstrap::search) type Bodies = BTreeMap<(String, String), Supplied>;

pub(crate) fn body(shell: &ShellState, index: usize) -> &[RichNode] {
    let Some(book) = shell
        .search_message_pages
        .as_ref()
        .filter(|b| b.current(shell))
    else {
        return &[];
    };
    if known_deleted(shell, book, index) {
        return &[];
    }
    let Some(row) = book.pages.get(&book.shown).and_then(|rows| rows.get(index)) else {
        return &[];
    };
    book.bodies
        .get(&book.shown)
        .and_then(|bodies| bodies.get(&(row.channel_id.clone(), row.ts.clone())))
        .map(|supplied| &supplied.nodes)
        .filter(|nodes| {
            nodes
                .iter()
                .any(|node| !node.plain_text().trim().is_empty())
        })
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The original search projection already removes these highlight delimiters
/// from plain text. Balanced within-leaf pairs may decorate the same clean text;
/// unsupported/malformed input retains original cleanup without emphasis.
pub(in crate::bootstrap::search) fn clean_nodes(nodes: Vec<RichNode>) -> Vec<RichNode> {
    // Presentation-only native cap across all leaves in this accepted result.
    // Overflow keeps clean text and styles, never truncates message content.
    let mut budget = 256;
    children(nodes, &mut budget)
}
fn children(nodes: Vec<RichNode>, budget: &mut usize) -> Vec<RichNode> {
    nodes.into_iter().map(|node| clean(node, budget)).collect()
}
fn clean(node: RichNode, budget: &mut usize) -> RichNode {
    let text = |s: String| s.replace(['\u{e000}', '\u{e001}'], "");
    match node {
        RichNode::Text(s) => super::emphasis::leaf_with_budget(s, None, budget),
        RichNode::Code(s) => RichNode::Code(text(s)),
        RichNode::StyledText {
            text: s,
            bold,
            italic,
            strike,
            code,
        } => super::emphasis::leaf_with_budget(s, Some([bold, italic, strike, code]), budget),
        RichNode::SearchText {
            spans,
            bold,
            italic,
            strike,
            code,
        } => {
            if spans.len() <= *budget {
                *budget -= spans.len();
                RichNode::SearchText {
                    spans,
                    bold,
                    italic,
                    strike,
                    code,
                }
            } else {
                RichNode::StyledText {
                    text: spans.into_iter().map(|(text, _)| text).collect(),
                    bold,
                    italic,
                    strike,
                    code,
                }
            }
        }
        RichNode::Underlined(child) => RichNode::Underlined(Box::new(clean(*child, budget))),
        RichNode::Paragraph(c) => RichNode::Paragraph(children(c, budget)),
        RichNode::Quote(c) => RichNode::Quote(children(c, budget)),
        RichNode::Header(c) => RichNode::Header(children(c, budget)),
        RichNode::Context(c) => RichNode::Context(children(c, budget)),
        RichNode::Actions(c) => RichNode::Actions(children(c, budget)),
        RichNode::Section {
            text: c,
            fields,
            accessory,
        } => RichNode::Section {
            text: children(c, budget),
            fields: fields
                .into_iter()
                .map(|nodes| children(nodes, budget))
                .collect(),
            accessory: accessory.map(|a| Box::new(clean(*a, budget))),
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
            items: items
                .into_iter()
                .map(|nodes| children(nodes, budget))
                .collect(),
        },
        RichNode::Emoji { name, glyph } => RichNode::Emoji {
            name: text(name),
            glyph: text(glyph),
        },
        RichNode::Divider => RichNode::Divider,
        // The existing exhaustive inert whitelist runs first. Any later safe
        // addition still falls back to text here until explicitly supported.
        other => RichNode::Text(text(other.plain_text())),
    }
}

pub(crate) fn fixture_body() -> Vec<RichNode> {
    vec![
        RichNode::Paragraph(vec![
            RichNode::StyledText {
                text: "Release ready".into(),
                bold: true,
                italic: false,
                strike: false,
                code: false,
            },
            RichNode::Text(" for review.".into()),
        ]),
        RichNode::Quote(vec![RichNode::Text(
            "Keep the indexed search context.".into(),
        )]),
    ]
}
