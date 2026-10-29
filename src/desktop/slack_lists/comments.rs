//! Read-only record comments own a file route, never the selected conversation.
use super::*;
use std::sync::Arc;
use super_platinum_core::slack_lists::comments::{Page, Target, projection::Projection};
mod accepted;
pub(crate) mod completion;
mod controller;
mod deletions;
pub(crate) mod delta_editor;
pub(crate) mod draft;
mod edits;
mod emoji_standard;
pub(crate) mod file_acquisition;
pub(crate) mod file_navigation;
pub(crate) mod fixture;
mod incoming;
pub(crate) mod member_root;
mod network;
pub(crate) mod own_delete;
mod own_delete_policy;
pub(crate) mod own_edit;
pub(crate) mod own_edit_policy;
mod owner;
pub(crate) mod reaction_actions;
mod reactions;
pub(crate) mod reply;
mod storage;
#[cfg(test)]
mod tests;
pub(crate) mod title_users;
pub(crate) use controller::{close, next_page, open, refresh};
pub(super) use deletions::apply as apply_deletion;
pub(super) use edits::apply as apply_edit;
pub(crate) use emoji_standard::changed as emoji_standard_changed;
pub(super) use incoming::apply as apply_incoming;
use owner::Owner;
pub(super) use reactions::apply as apply_reaction;

#[derive(Clone, Default)]
pub(crate) struct State {
    entry: Option<Entry>,
    open: bool,
    // Credential-free account/workspace debt survives switching List records.
    cooldown: Option<(Scope, i64)>,
    drafts: draft::Root,
    title_users: title_users::State,
    completion: completion::State,
    member_root: member_root::State,
    file_acquisition: file_acquisition::State,
    pub(crate) fixture: Option<fixture::Model>,
    reaction_actions: reaction_actions::State,
    own_edit: own_edit::State,
    own_delete: own_delete::State,
}
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    own_edit::carry_pending(previous, loaded);
    own_delete::carry_pending(previous, loaded);
    loaded.slack_lists.comments.reaction_actions =
        previous.slack_lists.comments.reaction_actions.clone();
    loaded
        .slack_lists
        .comments
        .drafts
        .entries
        .append(&mut previous.slack_lists.comments.drafts.entries);
}
#[derive(Clone)]
struct Entry {
    owner: Owner,
    title: String,
    target: Target,
    read: u64,
    // The transcript's acquisition generation, captured before its first await.
    read_socket: Option<u64>,
    // Send preflight uses the same read authority but never publishes its Page.
    read_replaces_page: bool,
    loading: bool,
    page: Option<Page>,
    projection: Option<Projection>,
    accepted: Vec<super_platinum_core::slack::models::Message>,
    // Socket arrivals are visible evidence, never local send acknowledgements.
    live: Vec<super_platinum_core::slack::models::Message>,
    // A late local ACK must not erase reactions received after its socket echo.
    live_reactions: std::collections::BTreeSet<String>,
    // Only live rows can await an older local ACK. Retired with that row.
    live_edits:
        std::collections::BTreeMap<String, super_platinum_core::slack_lists::comments::ReplyEdit>,
    // Socket deletions survive older server pages for this exact visible visit.
    deleted: std::collections::BTreeSet<String>,
    reveal: Option<(u64, String)>,
    rows: Arc<Vec<crate::model::MessageVm>>,
    error: Option<String>,
    retry_at: i64,
    cursors: Vec<String>,
    reaction_picker: Option<reaction_actions::selection::Picker>,
    reaction_addition: Option<reaction_actions::addition::Pending>,
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub generation: u64,
    pub title: String,
    pub channel: String,
    pub rows: Arc<Vec<crate::model::MessageVm>>,
    pub loading: bool,
    pub error: Option<String>,
    pub has_more: bool,
    pub retry_after: u64,
    pub reveal: Option<(u64, String)>,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let entry = shell.slack_lists.comments.entry.as_ref()?;
    if !entry.owner.active(shell) {
        return None;
    }
    Some(Presentation {
        generation: entry.owner.serial,
        title: entry.title.clone(),
        channel: entry.target.channel_id().into(),
        rows: entry.rows.clone(),
        loading: entry.loading,
        error: entry.error.clone(),
        has_more: entry.page.as_ref().is_some_and(|p| {
            p.has_more()
                && p.cursor()
                    .is_some_and(|c| !entry.cursors.iter().any(|seen| seen == c))
        }),
        retry_after: entry.retry_at.saturating_sub(now()).max(0) as u64,
        reveal: entry.reveal.clone(),
    })
}
fn unavailable() -> Error {
    Error::Api("list_comment_owner_changed".into())
}
fn cooldown(shell: &ShellState) -> i64 {
    shell
        .slack_lists
        .comments
        .cooldown
        .as_ref()
        .filter(|(owner, _)| owner.stable_current(shell))
        .map_or(0, |(_, deadline)| *deadline)
}
