use super::Workspace;
use crate::slack::models::UserId;
use std::collections::HashSet;

impl Workspace {
    pub fn vip_revision(&self, user: &str) -> u64 {
        self.vip_revisions.get(user).copied().unwrap_or(0)
    }

    /// Record each person's membership changes, including remove/add ABA cycles.
    pub fn apply_vip_snapshot(&mut self, users: HashSet<UserId>) {
        for user in self.vip_users.symmetric_difference(&users) {
            let revision = self.vip_revisions.entry(user.clone()).or_default();
            *revision = revision.wrapping_add(1);
        }
        self.vip_users = users;
    }

    /// A delayed mutation response must not undo newer realtime preferences.
    pub fn apply_vip_change_since(&mut self, user: &str, desired: bool, since: u64) -> bool {
        if self.vip_revision(user) != since {
            return false;
        }
        let changed = if desired {
            self.vip_users.insert(user.into())
        } else {
            self.vip_users.remove(user)
        };
        if changed {
            let revision = self.vip_revisions.entry(user.into()).or_default();
            *revision = revision.wrapping_add(1);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> Workspace {
        Workspace::from_session(&crate::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "SELF".into(),
            name: "Test".into(),
            url: "https://example.test".into(),
            token: "fixture".into(),
        })
    }
    #[test]
    fn newer_realtime_membership_wins_even_after_an_aba_cycle() {
        let mut ws = workspace();
        let start = ws.vip_revision("U1");
        ws.apply_vip_snapshot(HashSet::from(["U1".into()]));
        ws.apply_vip_snapshot(HashSet::new());
        assert!(!ws.apply_vip_change_since("U1", true, start));
        assert!(!ws.vip_users.contains("U1"));
    }
    #[test]
    fn other_people_and_unchanged_snapshots_do_not_invalidate_the_change() {
        let mut ws = workspace();
        let start = ws.vip_revision("U1");
        ws.apply_vip_snapshot(HashSet::from(["U2".into()]));
        ws.apply_vip_snapshot(HashSet::from(["U2".into()]));
        assert!(ws.apply_vip_change_since("U1", true, start));
        assert!(ws.vip_users.contains("U1"));
        assert!(ws.vip_users.contains("U2"));
        let remove = ws.vip_revision("U1");
        assert!(ws.apply_vip_change_since("U1", false, remove));
        assert!(!ws.apply_vip_change_since("U1", true, start));
    }
}
