//! Account-scoped Slack drafts queue. Unscheduled, deleted and sent drafts never
//! appear as scheduled messages; all original rich content and destinations survive edits.
use crate::slack::models::Message;
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeMap, HashSet};
mod realtime;
use realtime::RealtimeDrafts;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DraftFilter {
    Drafts,
    #[default]
    Scheduled,
}
impl DraftFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Drafts => "Drafts",
            Self::Scheduled => "Scheduled",
        }
    }
    pub fn accepts(self, message: &ScheduledMessage) -> bool {
        message.active()
            && match self {
                Self::Drafts => !message.scheduled(),
                Self::Scheduled => message.scheduled(),
            }
    }
}

pub const MAX_SCHEDULE_SECONDS: i64 = 120 * 24 * 60 * 60;
pub fn validate_time(post_at: i64, now: i64) -> Result<(), &'static str> {
    if post_at <= now {
        Err("Choose a time in the future.")
    } else if post_at > now.saturating_add(MAX_SCHEDULE_SECONDS) {
        Err("Choose a time within the next 120 days.")
    } else {
        Ok(())
    }
}
pub(crate) fn epoch<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Epoch {
        Number(i64),
        Text(String),
    }
    match Epoch::deserialize(d)? {
        Epoch::Number(n) => Ok(n),
        Epoch::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScheduledMessage {
    pub id: String,
    #[serde(default, rename = "date_scheduled", deserialize_with = "epoch")]
    pub post_at: i64,
    #[serde(default, deserialize_with = "epoch")]
    pub date_created: i64,
    #[serde(default)]
    pub client_msg_id: String,
    #[serde(default)]
    pub blocks: Vec<serde_json::Value>,
    #[serde(default)]
    pub file_ids: Vec<String>,
    #[serde(default)]
    pub destinations: Vec<serde_json::Value>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub is_sent: bool,
    #[serde(default)]
    pub last_updated_ts: String,
}
impl ScheduledMessage {
    pub fn active(&self) -> bool {
        !self.is_deleted && !self.is_sent
    }
    pub fn scheduled(&self) -> bool {
        self.post_at > 0 && self.active()
    }
    pub fn same_revision(&self, other: &Self) -> bool {
        self.id == other.id
            && self.active() == other.active()
            && if !self.last_updated_ts.is_empty() && !other.last_updated_ts.is_empty() {
                self.last_updated_ts == other.last_updated_ts
            } else {
                self.blocks == other.blocks
                    && self.destinations == other.destinations
                    && self.file_ids == other.file_ids
                    && self.post_at == other.post_at
            }
    }
    pub fn can_send(&self) -> bool {
        self.active()
            && self.destinations.len() == 1
            && self.channel().is_some_and(|c| !c.is_empty())
    }

    pub fn channel(&self) -> Option<&str> {
        self.destinations.first()?.get("channel_id")?.as_str()
    }
    pub fn thread_ts(&self) -> Option<&str> {
        self.destinations.first()?.get("thread_ts")?.as_str()
    }
    pub fn preview(&self) -> Message {
        Message {
            blocks: self.blocks.clone(),
            thread_ts: self.thread_ts().map(str::to_owned),
            ..Default::default()
        }
    }
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScheduledPage {
    #[serde(default)]
    pub drafts: Vec<ScheduledMessage>,
    #[serde(default)]
    pub has_more: bool,
}
#[derive(Debug, Default)]
pub struct ScheduledState {
    scope: Option<(u64, String)>,
    pub items: BTreeMap<String, ScheduledMessage>,
    pub has_more: bool,
    pub filter: DraftFilter,
    pub limit: u32,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub generation: u64,
    pub busy: HashSet<String>,
    realtime: RealtimeDrafts,
}
impl ScheduledState {
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|s| s.0 == epoch && s.1 == team)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            let generation = self.generation.wrapping_add(1);
            *self = Self {
                scope: Some((epoch, team.into())),
                generation,
                limit: 100,
                ..Default::default()
            };
        }
    }
    pub fn begin(&mut self, more: bool) -> Option<(u64, u32)> {
        if self.loading || !self.busy.is_empty() || (more && !self.has_more) {
            return None;
        }
        self.loading = true;
        self.realtime.pending.clear();
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
        Some((
            self.generation,
            if more {
                self.limit.saturating_add(100)
            } else {
                self.limit.max(100)
            },
        ))
    }
    pub fn apply(&mut self, generation: u64, page: ScheduledPage, limit: u32) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        self.loaded = true;
        self.limit = limit;
        self.has_more = page.has_more;
        self.items = self.merge_page(page.drafts);
        true
    }
    pub fn begin_mutation(&mut self, id: &str) -> bool {
        if !self.busy.insert(id.into()) {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        true
    }
    /// Keep the revision discovered during a save/send conflict available for
    /// the next editor, without allowing an older list response to replace it.
    pub fn accept_remote(&mut self, draft: ScheduledMessage) {
        if self.remote_is_obsolete(&draft) {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        if draft.active() {
            self.items.insert(draft.id.clone(), draft);
        } else {
            self.realtime.retired.insert(draft.id.clone());
            self.items.remove(&draft.id);
        }
    }
    /// Gate a mutation against the revision the user reviewed. A rejected
    /// mutation keeps the fresh content and invalidates any older list request.
    /// This is a preflight check, not a server-side compare-and-swap guarantee.
    pub fn verify_revision(
        &mut self,
        expected: &ScheduledMessage,
        remote: ScheduledMessage,
    ) -> bool {
        if self.remote_is_obsolete(&remote) {
            return false;
        }
        if expected.same_revision(&remote) {
            true
        } else {
            self.accept_remote(remote);
            false
        }
    }
    pub fn sorted(&self) -> Vec<&ScheduledMessage> {
        let mut items: Vec<_> = self
            .items
            .values()
            .filter(|m| self.filter.accepts(m))
            .collect();
        match self.filter {
            DraftFilter::Scheduled => items.sort_by_key(|m| (m.post_at, &m.id)),
            DraftFilter::Drafts => items.sort_by_key(|m| {
                (
                    std::cmp::Reverse(if m.last_updated_ts.is_empty() {
                        (m.date_created.max(0) as u64, 0)
                    } else {
                        crate::state::ts_key(&m.last_updated_ts)
                    }),
                    &m.id,
                )
            }),
        }
        items
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn epochs_rich_content_files_and_thread_destinations_survive() {
        let item:ScheduledMessage=serde_json::from_value(json!({"id":"Dr1","date_scheduled":"1800000000","date_created":1799990000,"blocks":[{"type":"rich_text"}],"file_ids":["F1"],"destinations":[{"channel_id":"C1","thread_ts":"1.2","broadcast":true}]})).unwrap();
        let request = crate::slack::api::ScheduleMessage::from_draft(&item);
        assert_eq!(item.post_at, 1800000000);
        assert_eq!(item.preview().thread_ts.as_deref(), Some("1.2"));
        assert_eq!(request.destinations[0]["broadcast"], true);
        assert_eq!(request.file_ids, vec!["F1"]);
        assert_eq!(request.blocks, item.blocks);
    }
    #[test]
    fn time_bounds_are_inclusive_and_future_only() {
        assert!(validate_time(10, 10).is_err());
        assert!(validate_time(11, 10).is_ok());
        assert!(validate_time(10 + MAX_SCHEDULE_SECONDS, 10).is_ok());
        assert!(validate_time(11 + MAX_SCHEDULE_SECONDS, 10).is_err());
    }
    #[test]
    fn mutation_and_account_change_invalidate_inflight_pages() {
        let mut s = ScheduledState::default();
        s.ensure_scope(1, "T1");
        let (g, _) = s.begin(false).unwrap();
        assert!(s.begin_mutation("Dr1"));
        assert!(!s.apply(g, ScheduledPage::default(), 100));
        assert!(!s.begin_mutation("Dr1"));
        s.ensure_scope(2, "T1");
        assert!(s.busy.is_empty());
        assert!(!s.apply(g, ScheduledPage::default(), 100));
    }
    #[test]
    fn queue_excludes_unscheduled_deleted_and_sent_and_orders_by_time() {
        let mut s = ScheduledState::default();
        s.ensure_scope(1, "T1");
        let (g, n) = s.begin(false).unwrap();
        let page=serde_json::from_value(json!({"drafts":[{"id":"b","date_scheduled":20},{"id":"a","date_scheduled":10},{"id":"draft"},{"id":"sent","date_scheduled":10,"is_sent":true},{"id":"deleted","date_scheduled":10,"is_deleted":true}],"has_more":true})).unwrap();
        assert!(s.apply(g, page, n));
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.sorted().len(), 2);
        assert_eq!(s.sorted()[0].id, "a");
        assert_eq!(s.begin(true).unwrap().1, 200);
    }
    #[test]
    fn draft_filter_keeps_recoverable_content_and_orders_recent_edits_first() {
        let mut state = ScheduledState::default();
        state.ensure_scope(1, "T1");
        let (g, n) = state.begin(false).unwrap();
        let page=serde_json::from_value(json!({"drafts":[{"id":"new","date_scheduled":0,"last_updated_ts":"20.000001","file_ids":["F1"],"blocks":[{"type":"rich_text"}]},{"id":"old","date_scheduled":0,"last_updated_ts":"10.000001"},{"id":"future","date_scheduled":30}]})).unwrap();
        state.apply(g, page, n);
        assert_eq!(state.sorted()[0].id, "future");
        state.filter = DraftFilter::Drafts;
        assert_eq!(
            state
                .sorted()
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "old"]
        );
        assert_eq!(state.sorted()[0].file_ids, vec!["F1"]);
    }
    #[test]
    fn revision_check_rejects_remote_edits_and_retired_messages() {
        let draft: ScheduledMessage = serde_json::from_value(
            json!({"id":"Dr1","date_scheduled":10,"last_updated_ts":"1.000001"}),
        )
        .unwrap();
        let mut remote = draft.clone();
        assert!(draft.same_revision(&remote));
        remote.last_updated_ts = "2.000001".into();
        assert!(!draft.same_revision(&remote));
        remote = draft.clone();
        remote.is_sent = true;
        assert!(!draft.same_revision(&remote));
        let mut without_stamp = draft.clone();
        without_stamp.last_updated_ts.clear();
        remote = without_stamp.clone();
        remote.file_ids.push("F2".into());
        assert!(!without_stamp.same_revision(&remote));
    }
    #[test]
    fn conflict_revision_survives_an_older_list_response() {
        let mut state = ScheduledState::default();
        state.ensure_scope(1, "T1");
        let (generation, limit) = state.begin(false).unwrap();
        let draft: ScheduledMessage = serde_json::from_value(json!({
            "id":"Dr1", "last_updated_ts":"2.000001", "file_ids":["F1"],
            "blocks":[{"type":"rich_text","elements":[]}]
        }))
        .unwrap();
        state.accept_remote(draft.clone());
        assert!(!state.loading);
        assert!(!state.apply(generation, ScheduledPage::default(), limit));
        assert!(state.items["Dr1"].same_revision(&draft));
        assert_eq!(state.items["Dr1"].file_ids, vec!["F1"]);
        let mut sent = draft;
        sent.is_sent = true;
        state.accept_remote(sent);
        assert!(!state.items.contains_key("Dr1"));
    }
    #[test]
    fn confirmation_keeps_the_reviewed_revision_across_a_queue_refresh() {
        let mut state = ScheduledState::default();
        state.ensure_scope(1, "T1");
        let reviewed: ScheduledMessage = serde_json::from_value(json!({
            "id":"Dr1", "last_updated_ts":"1.000001", "destinations":[{"channel_id":"C1"}],
            "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original"}]}]}]
        })).unwrap();
        state.accept_remote(reviewed.clone());
        let mut changed = reviewed.clone();
        changed.last_updated_ts = "2.000001".into();
        changed.blocks[0]["elements"][0]["elements"][0]["text"] = "Newer edit".into();
        changed.file_ids.push("F1".into());
        state.accept_remote(changed.clone());
        let (generation, limit) = state.begin(false).unwrap();
        assert!(!state.verify_revision(&reviewed, changed.clone()));
        assert_eq!(state.items["Dr1"].blocks, changed.blocks);
        assert_eq!(state.items["Dr1"].file_ids, vec!["F1"]);
        assert!(!state.apply(generation, ScheduledPage::default(), limit));
        // An eventually consistent preflight response must not authorize an
        // obsolete confirmation when a newer socket revision is already known.
        assert!(!state.verify_revision(&reviewed, reviewed.clone()));
        // A new confirmation explicitly reviewing the fresh version may proceed.
        assert!(state.verify_revision(&changed, changed.clone()));
    }
    #[test]
    fn revision_preflight_rejects_remote_deletion_and_payload_changes_without_stamps() {
        let mut state = ScheduledState::default();
        let original: ScheduledMessage = serde_json::from_value(json!({
            "id":"Dr1", "destinations":[{"channel_id":"C1","thread_ts":"1.2"}], "file_ids":["F1"]
        }))
        .unwrap();
        let mut moved = original.clone();
        moved.destinations[0]["thread_ts"] = "2.3".into();
        assert!(!state.verify_revision(&original, moved));
        let mut retired = original.clone();
        retired.is_deleted = true;
        assert!(!state.verify_revision(&original, retired));
        assert!(!state.items.contains_key("Dr1"));
    }
}
