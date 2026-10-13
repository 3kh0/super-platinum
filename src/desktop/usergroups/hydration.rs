use crate::state::ShellState;
use super_platinum_core::slack::{Error, models::UserGroup};

#[derive(Default)]
pub(super) struct Progress {
    applied: bool,
    stopped: bool,
}

impl Progress {
    /// Keep successful batches visible immediately. An error stops this run
    /// without undoing earlier batches or changing the next call's retry policy.
    pub(super) fn apply(
        &mut self,
        shell: &mut ShellState,
        team: &str,
        result: Result<Vec<UserGroup>, Error>,
    ) -> bool {
        if self.stopped {
            return false;
        }
        let Ok(groups) = result else {
            self.stopped = true;
            return false;
        };
        let Some(workspace) = shell.core.workspaces.get_mut(team) else {
            self.stopped = true;
            return false;
        };
        if groups.is_empty() {
            return true;
        }
        for mut group in groups {
            if let Some(existing) = workspace.usergroups.get(&group.id) {
                group.users = existing.users.clone();
                group.members_loaded = existing.members_loaded;
            }
            workspace.usergroups.insert(group.id.clone(), group);
        }
        shell.refresh_from_core();
        self.applied = true;
        true
    }

    pub(super) fn should_persist(&self) -> bool {
        self.applied && !self.stopped
    }
}

#[cfg(test)]
mod tests;
