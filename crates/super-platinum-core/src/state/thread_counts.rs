//! Workspace thread totals and stale HTTP protection. Socket arrival order is
//! authoritative: Slack's `timestamp` can be an old, manually moved read cursor.
use crate::slack::models::ThreadCounts;

#[derive(Debug, Clone, Default)]
pub struct ThreadCountsState {
    pub snapshot: Option<ThreadCounts>,
    revision: u64,
}

impl ThreadCountsState {
    pub fn from_cache(snapshot: Option<ThreadCounts>) -> Self {
        Self {
            snapshot,
            revision: 0,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn apply_live(&mut self, counts: ThreadCounts) {
        self.snapshot = Some(counts);
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn apply_snapshot(&mut self, counts: ThreadCounts, revision: u64) -> bool {
        if self.revision != revision {
            return false;
        }
        self.apply_live(counts);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn counts() -> ThreadCounts {
        ThreadCounts {
            has_unreads: true,
            mention_count: 7,
            vip_count: 15,
            unread_count_by_channel: [("C1".into(), 2)].into(),
            mention_count_by_channel: [("C1".into(), 7)].into(),
        }
    }
    #[test]
    fn late_snapshot_cannot_restore_counts_cleared_by_the_socket() {
        let mut state = ThreadCountsState::default();
        state.apply_live(counts());
        let revision = state.revision();
        state.apply_live(ThreadCounts::default());
        assert!(!state.apply_snapshot(counts(), revision));
        let value = state.snapshot.unwrap();
        assert!(!value.has_unreads);
        assert_eq!((value.mention_count, value.vip_count), (0, 0));
        assert!(value.unread_count_by_channel.is_empty());
    }
    #[test]
    fn first_snapshot_replaces_cached_state_and_missing_snapshot_preserves_unknown() {
        let mut state = ThreadCountsState::from_cache(Some(counts()));
        assert!(state.apply_snapshot(ThreadCounts::default(), 0));
        assert_eq!(state.snapshot, Some(ThreadCounts::default()));
        assert!(ThreadCountsState::default().snapshot.is_none());
    }

    #[test]
    fn counts_page_absence_preserves_state_but_explicit_clear_replaces_maps() {
        let session = crate::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://test.slack.com".into(),
            token: String::new(),
        };
        let mut workspace = crate::state::Workspace::from_session(&session);
        workspace.thread_counts.apply_live(counts());
        workspace.apply_counts(serde_json::from_str(r#"{"ok":true}"#).unwrap());
        assert_eq!(workspace.thread_counts.snapshot, Some(counts()));
        workspace.apply_counts(
            serde_json::from_str(r#"{"ok":true,"threads":{"has_unreads":false}}"#).unwrap(),
        );
        assert_eq!(
            workspace.thread_counts.snapshot,
            Some(ThreadCounts::default())
        );
    }
}
