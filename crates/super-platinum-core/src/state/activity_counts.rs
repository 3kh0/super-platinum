//! Complete category snapshots, kept independently of the paginated Activity feed.
use crate::slack::models::ActivityBadgeCounts;
mod dm_rail;
mod policy;
pub use policy::{BADGE_EXPERIMENT_NAMES, BadgeExperiments};

#[derive(Debug, Clone, Default)]
pub struct ActivityCountsState {
    pub snapshot: Option<ActivityBadgeCounts>,
    revision: u64,
    experiments: Option<BadgeExperiments>,
    experiment_request: u64,
}

impl ActivityCountsState {
    pub fn experiments(&self) -> Option<&BadgeExperiments> {
        self.experiments.as_ref()
    }

    pub fn begin_experiments(&mut self) -> u64 {
        self.experiment_request = self.experiment_request.wrapping_add(1);
        self.experiment_request
    }

    pub fn apply_experiments(&mut self, ticket: u64, policy: BadgeExperiments) -> bool {
        if ticket != self.experiment_request {
            return false;
        }
        self.experiments = Some(policy);
        true
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn apply_live(&mut self, counts: ActivityBadgeCounts) {
        self.snapshot = Some(counts);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn apply_snapshot(&mut self, counts: ActivityBadgeCounts, revision: u64) -> bool {
        if self.revision != revision {
            return false;
        }
        self.apply_live(counts);
        true
    }

    fn invalidate(&mut self) {
        self.snapshot = None;
        self.revision = self.revision.wrapping_add(1);
    }
}

impl super::Workspace {
    /// Source MdxY decrements once per previously unread root-list item, by its
    /// category, not by a bundled message count. Callers supply those exact deltas.
    pub fn mark_activity_categories_read(
        &mut self,
        deltas: &std::collections::BTreeMap<String, u32>,
    ) -> bool {
        if deltas.is_empty() {
            return self.activity_counts.snapshot.is_some();
        }
        let Some(mut counts) = self.activity_counts.snapshot.clone() else {
            return false;
        };
        counts.mark_read(deltas);
        self.apply_activity_counts(counts);
        true
    }

    pub fn apply_activity_counts(&mut self, counts: ActivityBadgeCounts) {
        self.activity_unread_count = Some(counts.total());
        self.activity_counts.apply_live(counts);
    }

    /// A local read knows the compatibility total, not a complete category map.
    /// Retire that map and any older in-flight HTTP snapshot until Slack supplies it.
    pub fn set_activity_unread_count(&mut self, total: u32) {
        self.activity_unread_count = Some(total);
        self.activity_counts.invalidate();
    }
}

#[cfg(test)]
mod tests;
