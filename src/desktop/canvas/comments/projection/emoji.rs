//! Local cached-body refresh driven by actual shared observation transitions.
//! No requests, timers, native editor replacement, or reply authority.
use super::super::{ShellState, State};
use crate::app_home::channel_fetches::{EmojiStandard, emoji_standard};
use dioxus::prelude::*;

fn current_owner<'a>(shell: &ShellState, model: &'a State) -> Option<&'a super::super::Owner> {
    model.query.as_ref()?;
    model.owner.as_ref().filter(|owner| owner.current(shell))
}

/// A still-current retained handle identifies exactly the same accepted slot.
/// None stays None without writes; a stale handle must be removed even when no
/// successor observation exists. No version-based equality or default catalog.
fn transition(shell: &ShellState, model: &State) -> Option<Option<EmojiStandard>> {
    let owner = current_owner(shell, model)?;
    if model
        .emoji_standard
        .as_ref()
        .is_some_and(|old| old.current(shell))
    {
        return None;
    }
    let next = emoji_standard(shell, &owner.scope.team);
    (model.emoji_standard.is_some() || next.is_some()).then_some(next)
}

/// Tracked reads let the mounted Panel observe accepted assignments/socket
/// changes. The second pass shares one original serial shell/model borrow and
/// changes only cached presentation plus its exact map witness.
pub(in crate::canvas::comments) fn observe(state: Signal<ShellState>, mut model: Signal<State>) {
    let changed = {
        let shell = state.read();
        let value = model.read();
        transition(&shell, &value).is_some()
    };
    if !changed {
        return;
    }
    let shell = state.peek();
    refresh(&shell, &mut model.write());
}

fn refresh(shell: &ShellState, model: &mut State) -> bool {
    let Some(next) = transition(shell, model) else {
        return false;
    };
    let team = &model.owner.as_ref().unwrap().scope.team;
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return false;
    };
    reproject_rows(
        workspace,
        &shell.media,
        &mut model.rows,
        next.as_ref().map(|v| v.standard()),
    );
    model.emoji_standard = next;
    true
}

fn reproject_rows(
    workspace: &super_platinum_core::state::Workspace,
    media: &crate::media::MediaRegistry,
    rows: &mut [super::super::Row],
    standard: Option<
        &super_platinum_core::slack_lists::comments::edit_seed::prepare::StandardEmoji,
    >,
) {
    for row in rows.iter_mut().take(super::super::MAX_ROWS) {
        row.body = super::row_vm(workspace, &row.message, media, standard);
        row.preview_body = row.preview.as_ref().map(|message| {
            crate::message_vm::message_vm_with_standard(workspace, message, media, false, standard)
        });
    }
}

#[cfg(test)]
mod tests;
