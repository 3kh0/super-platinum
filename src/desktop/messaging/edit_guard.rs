//! Denial-only facts. Absence of a table is never edit permission or readiness.
use crate::{model::RichNode, state::ShellState};
use super_platinum_core::slack::models::Message;

pub(crate) fn known_table(shell: &ShellState, team: &str, channel: &str, ts: &str) -> bool {
    if shell.core.active_team.as_deref() != Some(team) {
        return false;
    }
    let marker = |m: &Message| {
        // The enclosing bag supplies the namespace for this negative fact.
        // Contradictory embedded coordinates cannot turn a known table into
        // a plain-text edit seed. No positive authority is acquired here.
        m.ts.as_deref() == Some(ts) && crate::blocks::has_table_block(&m.blocks)
    };
    if shell
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel))
        .is_some_and(|bag| bag.messages.iter().any(&marker))
    {
        return true;
    }
    if shell
        .core
        .threads
        .iter()
        .any(|((t, c, _), bag)| t == team && c == channel && bag.messages.iter().any(&marker))
    {
        return true;
    }
    let feed = &shell.core.threads_view;
    feed.matches_scope(shell.core.account_epoch, team)
        && feed
            .items
            .iter()
            .filter(|i| i.channel().map(String::as_str) == Some(channel))
            .any(|i| {
                marker(&i.root_msg)
                    || i.unread_replies.iter().any(&marker)
                    || i.latest_replies.iter().any(&marker)
            })
}

/// Renderer-local fallback for supplied nodes. Table data and an uninspectably
/// large/deep edit seed are refused; no node can supply positive edit authority.
pub(crate) fn rendered_unsupported(nodes: &[RichNode]) -> bool {
    fn children(nodes: &[RichNode], depth: usize, budget: &mut usize) -> bool {
        nodes.iter().any(|n| visit(n, depth + 1, budget))
    }
    fn visit(node: &RichNode, depth: usize, budget: &mut usize) -> bool {
        if depth > 64 || *budget == 0 {
            return true;
        }
        *budget -= 1;
        match node {
            RichNode::Table(_) => true,
            RichNode::Underlined(node) => visit(node, depth + 1, budget),
            RichNode::Paragraph(c)
            | RichNode::Quote(c)
            | RichNode::Header(c)
            | RichNode::Context(c)
            | RichNode::Actions(c) => children(c, depth, budget),
            RichNode::Section {
                text,
                fields,
                accessory,
            } => {
                children(text, depth, budget)
                    || fields.iter().any(|c| children(c, depth, budget))
                    || accessory
                        .as_deref()
                        .is_some_and(|c| visit(c, depth + 1, budget))
            }
            RichNode::List { items, .. } => items.iter().any(|c| children(c, depth, budget)),
            _ => false,
        }
    }
    let mut budget = 4096;
    nodes.iter().any(|n| visit(n, 0, &mut budget))
}
