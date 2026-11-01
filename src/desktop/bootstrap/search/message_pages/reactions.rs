//! Passive native overlay. Source's separate keyed reaction selector is not
//! equated with the native transcript/thread bags; neither grants actions.
use super::*;
use crate::state::ReactionVm;

pub(crate) fn reactions(shell: &ShellState, index: usize) -> Vec<ReactionVm> {
    let Some(book) = shell
        .search_message_pages
        .as_ref()
        .filter(|b| b.current(shell))
    else {
        return Vec::new();
    };
    if known_deleted(shell, book, index) {
        return Vec::new();
    }
    let Some(row) = book.pages.get(&book.shown).and_then(|rows| rows.get(index)) else {
        return Vec::new();
    };
    if row.channel_id.is_empty() || row.ts.is_empty() {
        return Vec::new();
    }
    let Some(supplied) = book
        .bodies
        .get(&book.shown)
        .and_then(|bodies| bodies.get(&(row.channel_id.clone(), row.ts.clone())))
        .filter(|supplied| supplied.coherent)
    else {
        return Vec::new();
    };
    let Some(workspace) = shell.core.workspaces.get(&book.frame.team) else {
        return Vec::new();
    };
    let thread_key = (
        book.frame.team.clone(),
        row.channel_id.clone(),
        row.thread_ts.as_ref().unwrap_or(&row.ts).clone(),
    );
    let thread = shell.core.threads.get(&thread_key);
    if thread.is_some_and(|bag| bag.was_deleted(&row.ts)) {
        return Vec::new();
    }
    // A present shared copy, including an empty reaction list, wins. No search
    // result is inserted into canonical state or used to fetch a missing copy.
    let shared = workspace
        .messages
        .get(&row.channel_id)
        .and_then(|bag| {
            bag.messages
                .iter()
                .find(|m| m.ts.as_deref() == Some(&row.ts))
        })
        .or_else(|| {
            thread.and_then(|bag| {
                bag.messages
                    .iter()
                    .find(|m| m.ts.as_deref() == Some(&row.ts))
            })
        });
    if let Some(message) = shared {
        return crate::message_vm::reaction_vms_for_reading(workspace, message, &shell.media);
    }
    supplied.reactions.clone()
}

pub(crate) fn fixture_reactions() -> Vec<ReactionVm> {
    vec![
        ReactionVm {
            name: "thumbsup".into(),
            count: 3,
            own: true,
            glyph: Some(super_platinum_core::state::emoji_glyph("thumbsup")),
            ..Default::default()
        },
        ReactionVm {
            name: "shipit".into(),
            count: 1,
            ..Default::default()
        },
    ]
}

pub(crate) fn fixture_reaction_groups() -> Vec<ReactionVm> {
    [
        ("thumbsup::skin-tone-2", 2, true),
        ("shipit", 1, false),
        ("thumbsup::skin-tone-5", 3, false),
        ("thumbsup", 1, false),
    ]
    .into_iter()
    .map(|(name, count, own)| ReactionVm {
        name: name.into(),
        count,
        own,
        glyph: Some(super_platinum_core::state::emoji_glyph(name)),
        ..Default::default()
    })
    .collect()
}
