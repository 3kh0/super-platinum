//! These calls follow the scheduled operation's existing original-owner check.
//! Queue pages enrich positive metadata; absence never clears a draft key.
use super::*;
pub(super) fn observe(shell: &mut ShellState, team: &str, draft: &ScheduledMessage) {
    let changed = shell
        .core
        .workspaces
        .get_mut(team)
        .is_some_and(|w| w.draft_presence.observe_scheduled(draft).unwrap_or(false));
    if changed {
        crate::app_home::about::management::activity::drafts_changed(shell, team);
    }
}
pub(super) fn retire(shell: &mut ShellState, team: &str, id: &str) {
    let changed = shell
        .core
        .workspaces
        .get_mut(team)
        .is_some_and(|w| w.draft_presence.retire_server_id(id));
    if changed {
        crate::app_home::about::management::activity::drafts_changed(shell, team);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_metadata_and_definite_retirement_preserve_unrelated_and_local_drafts() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        let team = s.core.active_team.clone().unwrap();
        let raw = serde_json::json!({"id":"DRAFT_A","client_msg_id":"UNIQUE_A","destinations":[{"channel_id":"C_A"}],"last_updated_ts":"1.000001"});
        let draft: ScheduledMessage = serde_json::from_value(raw).unwrap();
        observe(&mut s, &team, &draft);
        assert!(s.core.workspaces[&team].draft_presence.has_channel("C_A"));
        let mut other = draft.clone();
        other.id = "DRAFT_B".into();
        other.client_msg_id = "UNIQUE_B".into();
        other.destinations =
            serde_json::from_value(serde_json::json!([{"channel_id":"C_B"}])).unwrap();
        observe(&mut s, &team, &other);
        let before = s.core.drafts.items.len();
        retire(&mut s, &team, &draft.id);
        assert!(!s.core.workspaces[&team].draft_presence.has_channel("C_A"));
        assert!(s.core.workspaces[&team].draft_presence.has_channel("C_B"));
        assert_eq!(s.core.drafts.items.len(), before);
        observe(&mut s, &team, &draft);
        assert!(!s.core.workspaces[&team].draft_presence.has_channel("C_A"));
    }
}
