use crate::state::ShellState;
use super_platinum_core::slack::{Error, models::UserGroup};

#[derive(Default)]
pub(super) struct Progress {
    applied: bool,
    stopped: bool,
}

impl Progress {
    /// Preserve ordered source history when this workspace has a genuine
    /// bootstrap lineage. Legacy caches retain the existing typed recovery.
    pub(super) async fn receive_observed(
        &mut self,
        mut state: dioxus::prelude::Signal<ShellState>,
        request: &super::ownership::GroupRead,
        result: Result<Vec<UserGroup>, Error>,
        ordered: Option<Vec<serde_json::Value>>,
        current: impl Fn(&ShellState) -> bool,
    ) -> bool {
        use dioxus::prelude::{ReadableExt, WritableExt};
        use super_platinum_core::state::directory_usergroups::{NativeMembers, Operation, Origin};
        if self.stopped || !current(&state.peek()) || !request.current(&state.peek()) {
            self.stopped = true;
            return false;
        }
        let Ok(groups) = result else {
            self.stopped = true;
            return false;
        };
        if !request.accepts(&groups) {
            self.stopped = true;
            return false;
        }
        if groups.is_empty() && ordered.as_ref().is_none_or(Vec::is_empty) {
            return true;
        }
        let rows = ordered.or_else(|| {
            groups
                .iter()
                .map(|group| group.label_source.clone())
                .collect()
        });
        if let Some(rows) = rows {
            let admission = super::source::enqueue(
                &mut state.write(),
                request.owner.clone(),
                Operation::Metadata {
                    origin: Origin::Fetch,
                    rows,
                    native_members: NativeMembers::Preserve,
                },
            );
            if let super::source::Admission::Tracked(work) = admission {
                if work
                    .run(state, |shell| request.current(shell) && current(shell))
                    .await
                    .is_err()
                {
                    self.stopped = true;
                    return false;
                }
                self.applied = true;
                return true;
            }
        }
        self.receive(&mut state.write(), request, Ok(groups))
    }

    pub(super) fn receive(
        &mut self,
        shell: &mut ShellState,
        request: &super::ownership::GroupRead,
        result: Result<Vec<UserGroup>, Error>,
    ) -> bool {
        if !request.current(shell) || result.as_ref().is_ok_and(|groups| !request.accepts(groups)) {
            self.stopped = true;
            return false;
        }
        self.apply(shell, &request.owner.session.team_id, result)
    }
    /// Keep successful batches visible immediately. An error stops this run
    /// without undoing earlier batches or changing the next call's retry policy.
    fn apply(
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
