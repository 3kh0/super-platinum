//! Fetch display policy alongside normal hydration, never on a render or timer.
use super::CountsOwner;
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{CoreAppState, slack::api, state::BadgeExperiments};

pub(super) struct RulesRead {
    owner: CountsOwner,
    ticket: u64,
}

impl CountsOwner {
    pub(super) fn begin_rules(&self, core: &mut CoreAppState) -> Option<RulesRead> {
        if !self.current(core) {
            return None;
        }
        let workspace = core.workspaces.get_mut(&self.session.team_id)?;
        Some(RulesRead {
            owner: self.clone(),
            ticket: workspace.activity_counts.begin_experiments(),
        })
    }

    pub(crate) fn refresh_badge_rules(&self, mut state: Signal<ShellState>) {
        if crate::fixture::is_fixture() {
            return;
        }
        let (read, client) = {
            let mut shell = state.write();
            let Some(read) = self.begin_rules(&mut shell.core) else {
                return;
            };
            (read, shell.core.client.clone())
        };
        dioxus::core::spawn_forever(async move {
            if let Ok(rules) =
                api::fetch_badge_experiments(&read.owner.transport, &client, &read.owner.session)
                    .await
            {
                read.apply(&mut state.write().core, rules);
            }
        });
    }
}

impl RulesRead {
    pub(super) fn apply(self, core: &mut CoreAppState, rules: BadgeExperiments) -> bool {
        if !self.owner.current(core) {
            return false;
        }
        core.workspaces
            .get_mut(&self.owner.session.team_id)
            .unwrap()
            .activity_counts
            .apply_experiments(self.ticket, rules)
    }
}
