//! Observation of the native runtime's initially empty Quip-title map.
//!
//! HTEe initializes a nonpersistent empty map; cf9a updates it only through the
//! editor delegate. This host has no such producer yet. File metadata, exports,
//! rename responses and Profile visits are not editor-title observations.
use crate::state::ShellState;
use std::sync::Arc;
use super_platinum_core::slack::api::custom::texty::file_titles::QuipTitle;

#[derive(Default)]
pub(crate) struct State {
    runtime: Option<Arc<()>>,
}

impl State {
    /// Call only when constructing a new shell runtime, never on Profile open.
    pub(crate) fn fresh() -> Self {
        Self {
            runtime: Some(Arc::new(())),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Owner {
    account: String,
    epoch: u64,
    team: String,
    user: String,
}

impl Owner {
    fn capture(shell: &ShellState) -> Option<Self> {
        let account = shell
            .core
            .active_account
            .as_ref()
            .filter(|s| !s.is_empty())?;
        let team = shell.core.active_team.as_ref().filter(|s| !s.is_empty())?;
        let workspace = shell.core.workspaces.get(team)?;
        let user = &workspace.self_user_id;
        (!user.is_empty() && shell.self_account.user_id == *user).then(|| Self {
            account: account.clone(),
            epoch: shell.core.account_epoch,
            team: team.clone(),
            user: user.clone(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct Observation {
    runtime: Arc<()>,
    owner: Owner,
    file: String,
}

impl Observation {
    pub(crate) fn capture(shell: &ShellState, id: &str) -> Option<Self> {
        if id.is_empty() {
            return None;
        }
        Some(Self {
            runtime: shell.canvas_titles.runtime.clone()?,
            owner: Owner::capture(shell)?,
            file: id.to_owned(),
        })
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        !self.file.is_empty()
            && shell
                .canvas_titles
                .runtime
                .as_ref()
                .is_some_and(|runtime| Arc::ptr_eq(runtime, &self.runtime))
            && Owner::capture(shell).as_ref() == Some(&self.owner)
    }

    /// Only the independently witnessed empty runtime map grants this value.
    /// The consumer must recheck `current` along with its own read/Token fences.
    pub(crate) fn value(&self) -> QuipTitle<'_> {
        QuipTitle::Nullish
    }
}

#[cfg(test)]
mod tests;
