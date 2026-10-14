//! Mutation-boundary Dock cache: no render subscriptions or periodic work.
use super::ShellState;
use std::collections::BTreeMap;
use super_platinum_core::{
    CoreAppState,
    state::{DockBadge, DockBadgeContribution},
};

#[derive(Default)]
pub(crate) struct Cache {
    owner: Option<(String, u64)>,
    workspaces: BTreeMap<String, Entry>,
    value: DockBadge,
}
struct Entry {
    user: String,
    contribution: DockBadgeContribution,
    /// Independently authoritative ordinary-unread state. Missing context must
    /// not keep a stale numeric badge after canonical highlights reach zero.
    has_unreads: Option<bool>,
}

impl Cache {
    fn prepare(&mut self, core: &CoreAppState) -> bool {
        let owner = core
            .active_account
            .as_ref()
            .map(|a| (a.clone(), core.account_epoch));
        if self.owner != owner {
            self.workspaces.clear();
            self.value = DockBadge::Empty;
            self.owner = owner;
        }
        self.workspaces.retain(|team, entry| {
            core.workspaces
                .get(team)
                .is_some_and(|w| w.self_user_id == entry.user)
        });
        self.owner.is_some()
    }
    fn refresh(&mut self, core: &CoreAppState, team: Option<&str>) {
        if self.prepare(core) {
            if let Some(team) = team {
                if let Some(workspace) = core.workspaces.get(team) {
                    self.project(team, workspace);
                }
            } else {
                for (team, workspace) in &core.workspaces {
                    self.project(team, workspace);
                }
            }
        }
        self.value = DockBadge::aggregate(self.workspaces.values().map(|e| e.contribution));
    }
    fn project(&mut self, team: &str, workspace: &super_platinum_core::state::Workspace) {
        if let Some(projection) = workspace.dock_badge_projection() {
            let has_unreads = projection.has_unreads.or_else(|| {
                self.workspaces
                    .get(team)
                    .and_then(|entry| entry.has_unreads)
            });
            self.workspaces.insert(
                team.into(),
                Entry {
                    user: workspace.self_user_id.clone(),
                    contribution: DockBadgeContribution {
                        highlights: projection.highlights,
                        has_unreads: has_unreads == Some(true),
                        show_bullet: projection.show_bullet,
                    },
                    has_unreads,
                },
            );
        }
        // Missing input retains a prior value only after prepare validated the
        // account, epoch, workspace presence and self user. Never infer zero.
    }
}

impl ShellState {
    pub(crate) fn dock_badge_value(&self) -> DockBadge {
        // Owner retirement is visible immediately, even before an asynchronous
        // account loader reaches a projection refresh. This check is O(1).
        if self
            .dock_badge_cache
            .owner
            .as_ref()
            .is_some_and(|(account, epoch)| {
                self.core.active_account.as_ref() == Some(account)
                    && self.core.account_epoch == *epoch
            })
        {
            self.dock_badge_cache.value
        } else {
            DockBadge::Empty
        }
    }
    pub(crate) fn refresh_dock_badge(&mut self) {
        self.dock_badge_cache.refresh(&self.core, None);
    }
    pub(crate) fn refresh_dock_badge_for(&mut self, team: &str) {
        self.dock_badge_cache.refresh(&self.core, Some(team));
    }
}

#[cfg(test)]
mod tests;
