use serde_json::Value;
use std::collections::BTreeMap;

/// Canonical counts independent of loaded Later items. `channel_badges` patches
/// merge in 2Zmw; saved counts replace when the supplied response contains them.
#[derive(Debug, Clone, Default)]
pub struct DockCountsState {
    pub(crate) channel_badges: Option<BTreeMap<String, u32>>,
    pub(crate) overdue: Option<u32>,
    revision: u64,
    pub(crate) context: super::DockWorkspaceContext,
    pub(crate) counts_loaded: bool,
}

impl DockCountsState {
    pub fn set_context(&mut self, context: super::DockWorkspaceContext) {
        if self.context != context {
            self.context = context;
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn overdue_count(&self) -> Option<u32> {
        self.overdue
    }
    pub fn channel_badges(&self) -> Option<&BTreeMap<String, u32>> {
        self.channel_badges.as_ref()
    }

    /// Complete client.counts response. Missing/null objects preserve existing
    /// authority; malformed objects cannot turn an existing count into zero.
    pub fn apply_snapshot(&mut self, extra: &BTreeMap<String, Value>, revision: u64) -> bool {
        if revision != self.revision {
            return false;
        }
        self.counts_loaded = true;
        if let Some(value) = extra.get("channel_badges") {
            self.merge_channel_badges(value);
        }
        if let Some(value) = extra.get("saved") {
            self.apply_saved_counts(value);
        }
        true
    }

    /// Legacy `badge_counts_updated.counts` uses the same partial-map reducer.
    /// The caller must select this branch only when Inbox is known disabled.
    pub fn merge_channel_badges(&mut self, value: &Value) -> bool {
        let Ok(patch) = serde_json::from_value::<BTreeMap<String, u32>>(value.clone()) else {
            return false;
        };
        self.channel_badges
            .get_or_insert_with(Default::default)
            .extend(patch);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Exact `saved.counts` / client.counts.saved field; not page row counts.
    pub fn apply_saved_counts(&mut self, value: &Value) -> bool {
        let Some(snapshot) = value.as_object() else {
            return false;
        };
        let Some(raw_count) = snapshot.get("uncompleted_overdue_count") else {
            // WKbT replaces a supplied saved-count snapshot as a whole. An
            // omitted field retires its previous authority; it is not zero.
            self.overdue = None;
            self.revision = self.revision.wrapping_add(1);
            return true;
        };
        let Some(count) = raw_count
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
        else {
            return false;
        };
        self.apply_overdue_count(count);
        true
    }
    pub fn apply_overdue_count(&mut self, count: u32) {
        self.overdue = Some(count);
        self.revision = self.revision.wrapping_add(1);
    }
}
