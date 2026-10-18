//! Read-only access to the shared account autocomplete observations. Consumers
//! supply their own query lease; this facade grants no Home action authority.
use super::*;
use serde_json::Value;
use super_platinum_core::{
    block_actions::home::workflow::directory_observations::Offline,
    composer_emoji::{model::Model, ranking::SimilarityCache},
};

/// Accepted account preference events retire immutable name/history inputs,
/// including consumers that have no Home or About surface open.
pub(crate) fn preferences_revision(shell: &ShellState) -> u64 {
    shell.app_home.workflow_names_revision
}

#[derive(Clone)]
pub(crate) struct Ranking(workflows::ranking_cache::Snapshot);
impl Ranking {
    pub(crate) fn capture(
        state: Signal<ShellState>,
        context: Arc<Context>,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        workflows::ranking_cache::capture_for(state, context, current).map(Self)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.0.current(shell)
    }
    pub(crate) fn model(&self) -> &Model {
        &self.0.model
    }
    pub(crate) fn offline(&self) -> &Offline {
        &self.0.offline
    }
    pub(crate) fn similarity(&self) -> &std::sync::Mutex<SimilarityCache> {
        &self.0.similarity
    }
}

#[derive(Clone)]
pub(crate) struct Navigation {
    value: Value,
    witness: workflows::navigation_history::Witness,
}
impl Navigation {
    /// Rejoins the existing account store and writer gate. No render-path IO.
    pub(crate) async fn capture(
        state: Signal<ShellState>,
        account: &str,
        team: &str,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Self, Error> {
        let (value, witness) =
            workflows::navigation_history::capture_for(state, account, team, current).await?;
        Ok(Self { value, witness })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.witness.current(shell)
    }
    pub(crate) fn value(&self) -> &Value {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_preferences_retire_inputs_without_home_and_refuse_foreign_owner() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::profile_edit::sections::fixture::install_interaction(&mut shell);
        shell.app_home = Default::default();
        let team = shell.core.active_team.clone().unwrap();
        let generation = shell.core.workspaces[&team].rt_generation;
        let event = super_platinum_core::slack::events::RtEvent::PreferencesChanged {
            name: "frecency".into(),
            value: serde_json::json!("{}"),
        };
        crate::app_home::realtime(&mut shell, &team, generation + 1, &event);
        crate::app_home::realtime(&mut shell, "OTHER", generation, &event);
        assert_eq!(preferences_revision(&shell), 0);
        crate::app_home::realtime(&mut shell, &team, generation, &event);
        assert_eq!(preferences_revision(&shell), 1);
        shell.core.active_account = Some("different-account".into());
        crate::app_home::realtime(&mut shell, &team, generation, &event);
        assert_eq!(preferences_revision(&shell), 1);
    }
}
