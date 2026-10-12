//! Scoped Activity pagination and reconciliation of changes received during a fetch.
use crate::slack::models::{ActivityFeedPage, ActivityItem, ChannelId, Message, MessageTs};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct ActivityState {
    pub items: Vec<ActivityItem>,
    pub hydrated: HashMap<(ChannelId, MessageTs), Message>,
    pub next_cursor: Option<String>,
    pub load_seq: u64,
    pub loading: bool,
    pub loaded: bool,
    pub failed: bool,
    pub selected: Option<String>,
    pub unread_only: bool,
    pub sync: ActivitySync,
}

#[derive(Debug, Clone, Default)]
pub struct ActivitySync {
    scope: Option<(u64, String)>,
    changes: HashMap<String, ActivityItem>,
    cursors: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct ActivityRequest {
    epoch: u64,
    team: String,
    sequence: u64,
    pub cursor: Option<String>,
}

impl ActivityState {
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.sync
            .scope
            .as_ref()
            .is_some_and(|s| s.0 == epoch && s.1 == team)
    }

    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            *self = Self {
                load_seq: self.load_seq.wrapping_add(1),
                unread_only: self.unread_only,
                sync: ActivitySync {
                    scope: Some((epoch, team.to_owned())),
                    ..Default::default()
                },
                ..Default::default()
            };
        }
    }

    pub fn begin(&mut self, append: bool) -> Option<ActivityRequest> {
        let (epoch, team) = self.sync.scope.clone()?;
        if self.loading || (append && self.next_cursor.is_none()) {
            return None;
        }
        self.loading = true;
        self.failed = false;
        self.load_seq = self.load_seq.wrapping_add(1);
        self.sync.changes.clear();
        if !append {
            self.sync.cursors.clear();
        }
        Some(ActivityRequest {
            epoch,
            team,
            sequence: self.load_seq,
            cursor: if append {
                self.next_cursor.clone()
            } else {
                None
            },
        })
    }

    pub fn accepts(&self, request: &ActivityRequest) -> bool {
        self.loading
            && self.matches_scope(request.epoch, &request.team)
            && self.load_seq == request.sequence
    }

    pub fn fail(&mut self, request: &ActivityRequest) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.loading = false;
        self.failed = true;
        self.sync.changes.clear();
        true
    }

    pub fn apply_page(&mut self, request: &ActivityRequest, page: ActivityFeedPage) -> bool {
        if !self.accepts(request) {
            return false;
        }
        let selected_identity = self
            .items
            .iter()
            .find(|i| self.selected.as_deref() == Some(&i.key))
            .map(ActivityItem::identity);
        if request.cursor.is_none() {
            self.items.clear();
        }
        for item in page.items {
            self.upsert_inner(item);
        }
        // Socket events and local reads observed after the request began win
        // against an equally old HTTP item, including when its key changed.
        for (_, item) in std::mem::take(&mut self.sync.changes) {
            self.upsert_inner(item);
        }
        self.sort();
        if let Some(selected) =
            selected_identity.and_then(|id| self.items.iter().find(|i| i.identity() == id))
        {
            self.selected = Some(selected.key.clone());
        }
        let mut next = page
            .response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|c| !c.is_empty());
        if let Some(cursor) = &request.cursor {
            self.sync.cursors.insert(cursor.clone());
        }
        if next.as_ref().is_some_and(|c| self.sync.cursors.contains(c)) {
            next = None;
            self.failed = true;
        }
        self.next_cursor = next;
        self.loading = false;
        self.loaded = true;
        let referenced: HashSet<_> = self
            .items
            .iter()
            .flat_map(|item| {
                item.channel().into_iter().flat_map(|channel| {
                    item.request_ts()
                        .into_iter()
                        .map(move |ts| (channel.to_owned(), ts))
                })
            })
            .collect();
        self.hydrated.retain(|key, _| referenced.contains(key));
        true
    }

    /// Number of feed items still unread — the bell badge.
    pub fn unread_count(&self) -> u32 {
        self.items.iter().filter(|item| item.is_pending()).count() as u32
    }

    /// A loaded page is only part of the feed. Subtract the rows actually read
    /// from the server total instead of replacing it with this page's count.
    pub fn total_after_read(&self, total: Option<u32>, previous_unread: u32) -> u32 {
        total
            .unwrap_or(previous_unread)
            .saturating_sub(previous_unread.saturating_sub(self.unread_count()))
    }

    /// Clears the item with this key. Returns its `(type, feed_ts)` when the
    /// item was unread, so the caller can tell Slack about it.
    pub fn mark_item_read(&mut self, key: &str) -> Option<(String, String)> {
        let item = self.items.iter_mut().find(|item| item.key == key)?;
        if !item.is_pending() {
            return None;
        }
        let target = (item.item.kind.clone(), item.feed_ts.clone());
        item.mark_read();
        if self.loading {
            self.sync.changes.insert(item.identity(), item.clone());
        }
        Some(target)
    }

    /// Clears every item that a channel read covers. Thread items keep their own
    /// unread state — Slack tracks those against the thread, not the channel.
    pub fn mark_channel_read(&mut self, channel: &str) -> bool {
        let mut changed = false;
        for item in &mut self.items {
            if item.is_thread() || item.channel() != Some(channel) {
                continue;
            }
            if item.mark_read() {
                changed = true;
                if self.loading {
                    self.sync.changes.insert(item.identity(), item.clone());
                }
            }
        }
        changed
    }

    pub fn upsert(&mut self, item: ActivityItem) {
        let change = item.clone();
        if self.upsert_inner(item) && self.loading {
            self.sync.changes.insert(change.identity(), change);
        }
        self.sort();
    }

    fn upsert_inner(&mut self, item: ActivityItem) -> bool {
        let identity = item.identity();
        if let Some(existing) = self.items.iter_mut().find(|i| i.identity() == identity) {
            if crate::state::cmp_ts(Some(&item.feed_ts), Some(&existing.feed_ts)).is_lt() {
                return false;
            }
            if self.selected.as_deref() == Some(existing.key.as_str()) {
                self.selected = Some(item.key.clone());
            }
            *existing = item;
        } else {
            self.items.push(item);
        }
        true
    }

    fn sort(&mut self) {
        self.items
            .sort_by(|a, b| crate::state::cmp_ts(Some(&b.feed_ts), Some(&a.feed_ts)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(ts: &str) -> ActivityItem {
        serde_json::from_value(serde_json::json!({
            "key": format!("mention-{ts}"), "feed_ts": ts, "is_unread": true,
            "item": {"type":"at_user", "message":{"channel":"C1", "ts":ts}}
        }))
        .unwrap()
    }

    fn page(items: Vec<ActivityItem>, cursor: Option<&str>) -> ActivityFeedPage {
        serde_json::from_value(serde_json::json!({
            "items":items, "response_metadata":{"next_cursor":cursor}
        }))
        .unwrap()
    }

    fn feed() -> ActivityState {
        let mut state = ActivityState::default();
        state.ensure_scope(1, "T1");
        state
    }

    #[test]
    fn switching_workspaces_and_back_rejects_old_page_and_error() {
        let mut state = feed();
        state.upsert(item("1.0"));
        state
            .hydrated
            .insert(("C1".into(), "1.0".into()), Message::default());
        let old = state.begin(false).unwrap();
        state.ensure_scope(1, "T2");
        assert!(state.items.is_empty());
        assert!(state.hydrated.is_empty());
        state.ensure_scope(1, "T1");
        let new = state.begin(false).unwrap();
        assert!(!state.apply_page(&old, page(vec![item("9.0")], None)));
        assert!(!state.fail(&old));
        assert!(state.accepts(&new));
        assert!(state.apply_page(&new, page(vec![item("2.0")], None)));
        assert_eq!(state.items[0].feed_ts, "2.0");
    }

    #[test]
    fn replacing_account_and_duplicate_fetches_cannot_cross_scopes() {
        let mut state = feed();
        let old = state.begin(false).unwrap();
        assert!(state.begin(false).is_none());
        state.ensure_scope(2, "T1");
        let new = state.begin(false).unwrap();
        assert!(!state.apply_page(&old, page(vec![item("1.0")], None)));
        assert!(state.accepts(&new));
    }

    #[test]
    fn refresh_preserves_socket_insert_and_local_read_during_fetch() {
        let mut state = feed();
        let old = item("1.0");
        state.upsert(old.clone());
        let request = state.begin(false).unwrap();
        state.mark_item_read(&old.key).unwrap();
        state.upsert(item("2.0"));
        state.apply_page(&request, page(vec![old], Some("next")));
        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].feed_ts, "2.0");
        assert!(state.items[0].is_pending());
        assert!(!state.items[1].is_pending());
    }

    #[test]
    fn channel_reads_survive_paging_and_do_not_erase_other_channels() {
        let mut state = feed();
        let old = item("1.0");
        state.upsert(old.clone());
        let request = state.begin(false).unwrap();
        assert!(state.mark_channel_read("C1"));
        let mut other = item("2.0");
        other.item.message.as_mut().unwrap().channel = Some("C2".into());
        state.apply_page(&request, page(vec![old, other], None));
        assert_eq!(state.unread_count(), 1);
        assert!(state.items[0].is_pending());
        assert!(!state.items[1].is_pending());
    }

    #[test]
    fn overlapping_pages_deduplicate_and_cursor_cycles_stop() {
        let mut state = feed();
        let request = state.begin(false).unwrap();
        state.apply_page(&request, page(vec![item("3.0"), item("2.0")], Some("a")));
        let request = state.begin(true).unwrap();
        assert_eq!(request.cursor.as_deref(), Some("a"));
        state.apply_page(&request, page(vec![item("2.0"), item("1.0")], Some("b")));
        assert_eq!(state.items.len(), 3);
        let request = state.begin(true).unwrap();
        state.apply_page(&request, page(vec![], Some("a")));
        assert!(state.failed);
        assert!(state.next_cursor.is_none());
        assert!(state.begin(true).is_none());
        // Explicit refresh recovers from a server's repeating cursor.
        let request = state.begin(false).unwrap();
        state.apply_page(&request, page(vec![item("4.0")], Some("a")));
        assert!(!state.failed);
        assert!(state.next_cursor.is_some());
    }

    #[test]
    fn failure_preserves_rows_and_cursor_for_retry() {
        let mut state = feed();
        let request = state.begin(false).unwrap();
        state.apply_page(&request, page(vec![item("2.0")], Some("a")));
        let failed = state.begin(true).unwrap();
        state.upsert(item("3.0"));
        assert!(state.fail(&failed));
        assert_eq!(state.items.len(), 2);
        let retry = state.begin(true).unwrap();
        assert_eq!(retry.cursor, failed.cursor);
        assert!(!state.fail(&failed));
        state.apply_page(&retry, page(vec![item("1.0")], None));
        assert_eq!(state.items.len(), 3);
    }

    #[test]
    fn refresh_tracks_selected_bundle_key_and_prunes_unused_previews() {
        let mut state = feed();
        let mut old = item("1.0");
        old.item.kind = "dm".into();
        state.selected = Some(old.key.clone());
        state.upsert(old);
        state
            .hydrated
            .insert(("C1".into(), "1.0".into()), Message::default());
        let request = state.begin(false).unwrap();
        let mut new = item("2.0");
        new.item.kind = "dm".into();
        let key = new.key.clone();
        state.apply_page(&request, page(vec![new], None));
        assert_eq!(state.selected.as_deref(), Some(key.as_str()));
        assert!(state.hydrated.is_empty());
    }

    #[test]
    fn reading_one_loaded_notification_preserves_unloaded_unread_total() {
        let mut state = feed();
        state.upsert(item("1.0"));
        state.upsert(item("2.0"));
        let previous = state.unread_count();
        state.mark_item_read("mention-1.0").unwrap();
        assert_eq!(state.total_after_read(Some(40), previous), 39);
        assert_eq!(state.total_after_read(None, previous), 1);
        assert_eq!(state.total_after_read(Some(0), previous), 0);
    }

    #[test]
    fn newer_server_bundle_wins_over_an_older_local_read() {
        let mut state = feed();
        let mut old = item("1.0");
        old.item.kind = "dm".into();
        state.upsert(old);
        let request = state.begin(false).unwrap();
        state.mark_item_read("mention-1.0").unwrap();
        let mut new = item("2.0");
        new.item.kind = "dm".into();
        state.apply_page(&request, page(vec![new], None));
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].feed_ts, "2.0");
        assert!(state.items[0].is_pending());
    }
}
