//! Original request ownership for typed usergroup hydration, not a new cache.
use super::GroupPanel;
use crate::{directory_member_fetches::Context, state::ShellState};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::slack::models::{User, UserGroup};

pub(super) struct GroupRead {
    pub(super) owner: Arc<Context>,
    before: BTreeMap<String, Option<Value>>,
}
impl GroupRead {
    pub(super) fn capture(shell: &ShellState, owner: Arc<Context>, ids: &[String]) -> Option<Self> {
        if !owner.current(shell) {
            return None;
        }
        let workspace = shell.core.workspaces.get(&owner.session.team_id)?;
        let before = ids
            .iter()
            .map(|id| {
                Some((
                    id.clone(),
                    workspace
                        .usergroups
                        .get(id)
                        .map(serde_json::to_value)
                        .transpose()
                        .ok()?,
                ))
            })
            .collect::<Option<_>>()?;
        Some(Self { owner, before })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.before.iter().all(|(id, before)| {
                        w.usergroups
                            .get(id)
                            .map(serde_json::to_value)
                            .transpose()
                            .is_ok_and(|now| &now == before)
                    })
                })
    }
    pub(super) fn accepts(&self, groups: &[UserGroup]) -> bool {
        groups
            .iter()
            .all(|group| self.before.contains_key(&group.id))
    }
}

pub(super) struct UsersRead {
    group: GroupRead,
    before: BTreeMap<String, Option<Value>>,
}
impl UsersRead {
    pub(super) fn capture(shell: &ShellState, group: GroupRead, ids: &[String]) -> Option<Self> {
        if !group.current(shell) {
            return None;
        }
        let workspace = shell.core.workspaces.get(&group.owner.session.team_id)?;
        let before = ids
            .iter()
            .map(|id| {
                Some((
                    id.clone(),
                    workspace
                        .users
                        .get(id)
                        .map(serde_json::to_value)
                        .transpose()
                        .ok()?,
                ))
            })
            .collect::<Option<_>>()?;
        Some(Self { group, before })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.group.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.group.owner.session.team_id)
                .is_some_and(|w| {
                    self.before.iter().all(|(id, before)| {
                        w.users
                            .get(id)
                            .map(serde_json::to_value)
                            .transpose()
                            .is_ok_and(|now| &now == before)
                    })
                })
    }
    pub(super) fn accepts(&self, users: &[User]) -> bool {
        users.iter().all(|user| self.before.contains_key(&user.id))
    }
}

pub(super) fn panel_current(
    shell: &ShellState,
    owner: &Context,
    id: &str,
    attempt: &Arc<()>,
) -> bool {
    owner.current(shell)
        && shell.core.active_team.as_ref() == Some(&owner.session.team_id)
        && shell
            .group_panel
            .as_ref()
            .is_some_and(|panel| same_panel(panel, id, attempt))
}
pub(super) fn same_panel(panel: &GroupPanel, id: &str, attempt: &Arc<()>) -> bool {
    panel.id == id && Arc::ptr_eq(&panel.attempt, attempt)
}

pub(super) fn finish(
    shell: &mut ShellState,
    owner: &Context,
    id: &str,
    attempt: &Arc<()>,
    success: bool,
) -> bool {
    if !panel_current(shell, owner, id, attempt) {
        return false;
    }
    let panel = shell.group_panel.as_mut().expect("checked current panel");
    panel.loading = false;
    panel.error = !success;
    true
}

#[cfg(test)]
mod tests;
