//! One-way local reading. No request, navigation, or shared-body authority.
use super::*;
use crate::state::RichNode;
use std::collections::BTreeSet;

pub(super) type Expanded = BTreeSet<(u32, String, String)>;

pub(crate) fn is_expanded(shell: &ShellState, index: usize) -> bool {
    let Some(book) = shell
        .search_message_pages
        .as_ref()
        .filter(|b| b.current(shell))
    else {
        return false;
    };
    let Some(row) = rows(shell).get(index) else {
        return false;
    };
    !body(shell, index).is_empty()
        && book
            .expanded
            .contains(&(book.shown, row.channel_id.clone(), row.ts.clone()))
}

// Native positive disclosure policy for the existing three-line clamp. This
// is not Source canBeTruncated, normalized extracts, or a geometry measurement.
pub(crate) fn can_expand(shell: &ShellState, index: usize) -> bool {
    let nodes = body(shell, index);
    !nodes.is_empty()
        && !is_expanded(shell, index)
        && (nodes.len() > 3
            || rows(shell)
                .get(index)
                .is_some_and(|row| row.text.chars().take(201).count() > 200))
}

pub(crate) fn expand(shell: &mut ShellState, source: &RowSource) -> bool {
    let Some(row) = source.resolve(shell) else {
        return false;
    };
    if !can_expand(shell, source.index) {
        return false;
    }
    let book = shell.search_message_pages.as_mut().unwrap();
    // Explicit local intent persists for this page/message in this Book only.
    // A new Book clears it; Source remount/reset lifetime is not inferred.
    book.expanded.insert((book.shown, row.channel_id, row.ts))
}

pub(crate) fn disclosure_fixture_body() -> Vec<RichNode> {
    let mut nodes = fixture_body();
    nodes.extend([
        RichNode::Paragraph(vec![RichNode::Text(
            "Review the complete accepted result without leaving the search query.".into(),
        )]),
        RichNode::Paragraph(vec![RichNode::Text(
            "This additional context is supplied by the result and remains passive reading.".into(),
        )]),
    ]);
    nodes
}
