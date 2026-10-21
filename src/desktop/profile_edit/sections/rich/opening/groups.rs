use super::*;
use crate::directory_member_fetches::Context;
use std::collections::BTreeMap;

/// Actual group-label observations captured for this unopened description.
/// Mounted sessions retain their admitted labels independently of later rows.
pub(super) struct Groups {
    owner: Context,
    rows: BTreeMap<String, Value>,
    labels: BTreeMap<String, String>,
}
impl Groups {
    pub(super) fn hits(shell: &ShellState, token: &Token, ids: &BTreeSet<String>) -> Vec<Hit> {
        if !token.current(shell) {
            return vec![];
        }
        let Ok(owner) = Context::capture(shell, &token.owner.team) else {
            return vec![];
        };
        let Some(workspace) = shell.core.workspaces.get(&token.owner.team) else {
            return vec![];
        };
        let owner = Arc::new(owner);
        ids.iter()
            .filter(|id| {
                workspace
                    .usergroups
                    .get(*id)
                    .is_some_and(|g| &g.id == *id && g.profile_label().is_some())
            })
            .map(|id| Hit::Group {
                owner: owner.clone(),
                id: id.clone(),
            })
            .collect()
    }
    pub(super) fn capture(
        shell: &ShellState,
        token: &Token,
        ids: &BTreeSet<String>,
    ) -> Result<Self, Error> {
        if !token.current(shell) {
            return Err(unavailable());
        }
        let owner = Context::capture(shell, &token.owner.team)?;
        let workspace = shell
            .core
            .workspaces
            .get(&token.owner.team)
            .ok_or_else(unavailable)?;
        let mut rows = BTreeMap::new();
        let mut labels = BTreeMap::new();
        for id in ids {
            let group = workspace
                .usergroups
                .get(id)
                .filter(|g| &g.id == id)
                .ok_or_else(unavailable)?;
            let label = group.profile_label().ok_or_else(unavailable)?;
            rows.insert(
                id.clone(),
                serde_json::to_value(group).map_err(|_| unavailable())?,
            );
            labels.insert(id.clone(), label);
        }
        Ok(Self {
            owner,
            rows,
            labels,
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.rows.iter().all(|(id, before)| {
                        w.usergroups.get(id).is_some_and(|group| {
                            serde_json::to_value(group).is_ok_and(|now| &now == before)
                        })
                    })
                })
    }
    pub(super) fn label(&self, raw: &Value) -> Result<String, Error> {
        self.labels
            .get(raw["usergroup_id"].as_str().ok_or_else(unavailable)?)
            .cloned()
            .ok_or_else(unavailable)
    }
}
