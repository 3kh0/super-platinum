//! Slack-backed saved items. UI state is scoped to one account and workspace.
use crate::slack::models::Message;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LaterFilter {
    #[default]
    Saved,
    Archived,
    Completed,
}
impl LaterFilter {
    pub const ALL: [Self; 3] = [Self::Saved, Self::Archived, Self::Completed];
    pub fn api(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Archived => "archived",
            Self::Completed => "completed",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Saved => "In progress",
            Self::Archived => "Archived",
            Self::Completed => "Completed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SavedKey {
    pub item_type: String,
    pub item_id: String,
    pub ts: String,
}
impl SavedKey {
    pub fn message(channel: impl Into<String>, ts: impl Into<String>) -> Self {
        Self {
            item_type: "message".into(),
            item_id: channel.into(),
            ts: ts.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SavedItem {
    pub item_id: String,
    pub item_type: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub date_created: i64,
    #[serde(default)]
    pub date_updated: i64,
    #[serde(default)]
    pub date_due: i64,
    #[serde(default)]
    pub date_completed: i64,
    #[serde(default)]
    pub date_snoozed_until: i64,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub todo_state: String,
    #[serde(default)]
    pub description: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
impl SavedItem {
    pub fn description_message(&self) -> Message {
        Message {
            blocks: self.description.clone(),
            ..Default::default()
        }
    }
    pub fn key(&self) -> SavedKey {
        SavedKey {
            item_type: self.item_type.clone(),
            item_id: self.item_id.clone(),
            ts: self.ts.clone(),
        }
    }
    pub fn filter(&self) -> LaterFilter {
        if self.is_archived || self.state == "archived" {
            LaterFilter::Archived
        } else if self.date_completed > 0 || self.state == "completed" {
            LaterFilter::Completed
        } else {
            LaterFilter::Saved
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SavedCounts {
    #[serde(default)]
    pub uncompleted_count: usize,
    #[serde(default)]
    pub archived_count: usize,
    #[serde(default)]
    pub completed_count: usize,
}
impl SavedCounts {
    pub fn get(&self, filter: LaterFilter) -> usize {
        match filter {
            LaterFilter::Saved => self.uncompleted_count,
            LaterFilter::Archived => self.archived_count,
            LaterFilter::Completed => self.completed_count,
        }
    }
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SavedPage {
    #[serde(default)]
    pub saved_items: Vec<SavedItem>,
    #[serde(default)]
    pub counts: SavedCounts,
    #[serde(default)]
    pub response_metadata: SavedMetadata,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SavedMetadata {
    #[serde(default)]
    pub next_cursor: String,
}

#[derive(Debug, Clone, Copy)]
pub enum SavedAction {
    Save,
    Remove,
    Complete,
    Restore,
    Archive,
    Unarchive,
    Reminder(i64),
}

#[derive(Debug, Default)]
pub struct LaterState {
    pub scope: Option<(u64, String)>,
    pub generation: u64,
    pub filter: LaterFilter,
    pub items: HashMap<SavedKey, SavedItem>,
    pub visible: Vec<SavedKey>,
    pub hydrated: HashMap<SavedKey, Message>,
    preview_updates: HashMap<SavedKey, Option<Message>>,
    pub busy: HashSet<SavedKey>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub next_cursor: Option<String>,
    pub counts: SavedCounts,
    pub last_event_ts: Option<String>,
}
impl LaterState {
    pub fn update_message(&mut self, channel: &str, message: &Message) {
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        let key = SavedKey::message(channel, ts);
        if !self.items.contains_key(&key) && !self.hydrated.contains_key(&key) {
            return;
        }
        if matches!(self.preview_updates.get(&key), Some(None)) {
            return;
        }
        if let Some(update) = self
            .preview_updates
            .entry(key.clone())
            .or_insert_with(|| Some(Message::default()))
        {
            crate::state::merge_message(update, message.clone());
        }
        if let Some(existing) = self.hydrated.get_mut(&key) {
            crate::state::merge_message(existing, message.clone());
        }
    }
    pub fn delete_message(&mut self, channel: &str, ts: &str) {
        let key = SavedKey::message(channel, ts);
        if !self.items.contains_key(&key) && !self.hydrated.contains_key(&key) {
            return;
        }
        self.preview_updates.insert(key.clone(), None);
        if let Some(existing) = self.hydrated.get_mut(&key) {
            *existing = Message {
                ts: existing.ts.clone(),
                user: existing.user.clone(),
                text: Some("This message was deleted.".into()),
                ..Default::default()
            };
        }
    }
    /// A history response started before an edit/delete must not restore the
    /// obsolete preview when it eventually arrives.
    pub fn hydrate_message(&mut self, key: SavedKey, mut message: Message) {
        match self.preview_updates.get(&key) {
            Some(Some(update)) => crate::state::merge_message(&mut message, update.clone()),
            Some(None) => {
                message = Message {
                    ts: Some(key.ts.clone()),
                    user: message.user,
                    text: Some("This message was deleted.".into()),
                    ..Default::default()
                }
            }
            None => {}
        }
        self.hydrated.insert(key, message);
    }
    /// Slack's live saved-item frames use the same item shape as saved.list.
    /// Ignore replayed frames and invalidate older pages before merging changes.
    pub fn apply_event(&mut self, event: &crate::slack::events::RawEvent) -> bool {
        if !matches!(
            event.kind.as_str(),
            "saved_added" | "saved_updated" | "saved_deleted"
        ) {
            return false;
        }
        let Some(item) = event
            .rest
            .get("saved")
            .and_then(|value| serde_json::from_value::<SavedItem>(value.clone()).ok())
        else {
            return false;
        };
        let timestamp = event.rest.get("event_ts").and_then(|value| value.as_str());
        if let (Some(previous), Some(current)) = (self.last_event_ts.as_deref(), timestamp)
            && crate::state::ts_key(current) <= crate::state::ts_key(previous)
        {
            return false;
        }
        if let Some(timestamp) = timestamp {
            self.last_event_ts = Some(timestamp.into());
        }
        let key = item.key();
        if event.kind == "saved_deleted" {
            self.updated(&key, None);
            self.hydrated.remove(&key);
            self.preview_updates.remove(&key);
        } else {
            self.updated(&key, Some(item));
        }
        if let Some(counts) = event
            .rest
            .get("counts")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
        {
            self.counts = counts;
        }
        true
    }
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t)| *e == epoch && t == team)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            let generation = self.generation.wrapping_add(1);
            *self = Self {
                scope: Some((epoch, team.into())),
                generation,
                ..Default::default()
            };
        }
    }
    pub fn begin(&mut self, filter: LaterFilter, append: bool) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        if self.filter != filter {
            self.visible.clear();
            self.loaded = false;
        }
        self.filter = filter;
        self.loading = true;
        self.error = None;
        if !append {
            self.next_cursor = None;
        }
        self.generation
    }
    pub fn apply_page(
        &mut self,
        generation: u64,
        page: SavedPage,
        append: bool,
        requested_cursor: Option<&str>,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        if !append {
            self.visible.clear();
            self.items.retain(|_, item| item.filter() != self.filter);
        }
        for item in page.saved_items {
            let key = item.key();
            if !self.visible.contains(&key) {
                self.visible.push(key.clone());
            }
            self.items.insert(key, item);
        }
        let cursor = page.response_metadata.next_cursor;
        self.next_cursor =
            (!cursor.is_empty() && Some(cursor.as_str()) != requested_cursor).then_some(cursor);
        self.counts = page.counts;
        self.loading = false;
        self.loaded = true;
        true
    }
    pub fn updated(&mut self, key: &SavedKey, item: Option<SavedItem>) {
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.visible.retain(|entry| entry != key);
        self.items.remove(key);
        if let Some(item) = item {
            if item.filter() == self.filter {
                self.visible.insert(0, key.clone());
            }
            self.items.insert(key.clone(), item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_hydration_cannot_restore_edited_or_deleted_saved_messages() {
        let mut state = LaterState::default();
        let generation = state.begin(LaterFilter::Saved, false);
        assert!(state.apply_page(generation, page(), false, None));
        let key = SavedKey::message("C_PRIVATE", "1.2");
        let original = Message {
            ts: Some("1.2".into()),
            user: Some("U1".into()),
            text: Some("old".into()),
            ..Default::default()
        };
        let edit = Message {
            ts: original.ts.clone(),
            text: Some("edited while history was loading".into()),
            ..Default::default()
        };
        state.update_message("C_PRIVATE", &edit);
        state.hydrate_message(key.clone(), original.clone());
        assert_eq!(state.hydrated[&key].text, edit.text);
        assert_eq!(state.hydrated[&key].user, original.user);
        state.delete_message("C_PRIVATE", "1.2");
        state.update_message("C_PRIVATE", &edit);
        state.hydrate_message(key.clone(), original);
        assert_eq!(
            state.hydrated[&key].text.as_deref(),
            Some("This message was deleted.")
        );
        assert!(state.hydrated[&key].blocks.is_empty());
    }
    #[test]
    fn edited_and_deleted_messages_refresh_saved_previews_without_losing_identity() {
        let key = SavedKey::message("C1", "1.2");
        let mut state = LaterState::default();
        state.hydrated.insert(
            key.clone(),
            Message {
                ts: Some("1.2".into()),
                user: Some("U1".into()),
                text: Some("old".into()),
                blocks: vec![
                    serde_json::json!({"type":"section","text":{"type":"plain_text","text":"old"}}),
                ],
                ..Default::default()
            },
        );
        state.update_message(
            "C1",
            &Message {
                ts: Some("1.2".into()),
                text: Some("new".into()),
                blocks: vec![
                    serde_json::json!({"type":"section","text":{"type":"plain_text","text":"new"}}),
                ],
                ..Default::default()
            },
        );
        assert_eq!(state.hydrated[&key].user.as_deref(), Some("U1"));
        assert_eq!(state.hydrated[&key].text.as_deref(), Some("new"));
        state.delete_message("C1", "1.2");
        assert!(state.hydrated[&key].blocks.is_empty());
        assert_eq!(
            state.hydrated[&key].text.as_deref(),
            Some("This message was deleted.")
        );
        assert_eq!(state.hydrated[&key].ts.as_deref(), Some("1.2"));
    }
    #[test]
    fn saved_events_merge_counts_reject_replays_and_invalidate_older_pages() {
        let mut state = LaterState::default();
        let generation = state.begin(LaterFilter::Saved, false);
        let event = |kind: &str, timestamp: &str, completed: bool| {
            serde_json::from_value::<crate::slack::events::RawEvent>(serde_json::json!({"type":kind,"event_ts":timestamp,"saved":{"item_type":"message","item_id":"C_PRIVATE","ts":"1.2","state":if completed {"completed"}else{"in_progress"}},"counts":{"uncompleted_count":if completed {0}else{1},"completed_count":if completed {1}else{0}}})).unwrap()
        };
        assert!(state.apply_event(&event("saved_added", "10.1", false)));
        assert_eq!(state.visible.len(), 1);
        assert!(!state.apply_page(generation, page(), false, None));
        assert!(state.apply_event(&event("saved_updated", "11.1", true)));
        assert!(state.visible.is_empty());
        assert_eq!(state.counts.completed_count, 1);
        assert!(!state.apply_event(&event("saved_added", "10.1", false)));
        assert!(state.apply_event(&event("saved_deleted", "12.1", true)));
        assert!(state.items.is_empty());
    }
    #[test]
    fn reminders_keep_rich_description_without_requiring_a_message_timestamp() {
        let description = serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review 🚀\nnotes","style":{"bold":true}},{"type":"user","user_id":"U1"}]}]}]);
        let item: SavedItem=serde_json::from_value(serde_json::json!({"item_id":"Sa1","item_type":"reminder","description":description,"date_due":123,"state":"in_progress"})).unwrap();
        assert_eq!(item.key().ts, "");
        assert_eq!(
            serde_json::to_value(item.description_message().blocks).unwrap(),
            description
        );
        assert_eq!(
            serde_json::to_value(item).unwrap()["description"],
            description
        );
    }
    fn page() -> SavedPage {
        serde_json::from_value(serde_json::json!({"saved_items":[{"item_type":"message","item_id":"C_PRIVATE","ts":"1.2","state":"in_progress","todo_state":"saved"}],"counts":{"uncompleted_count":1},"response_metadata":{"next_cursor":"next"}})).unwrap()
    }
    #[test]
    fn stale_pages_and_workspace_changes_cannot_restore_other_accounts_items() {
        let mut state = LaterState::default();
        state.ensure_scope(1, "T1");
        let old = state.begin(LaterFilter::Saved, false);
        state.ensure_scope(2, "T1");
        assert!(!state.apply_page(old, page(), false, None));
        assert!(state.visible.is_empty());
        let current = state.begin(LaterFilter::Saved, false);
        assert!(state.apply_page(current, page(), false, None));
        state.ensure_scope(2, "T2");
        assert!(state.items.is_empty());
    }
    #[test]
    fn pagination_deduplicates_and_stops_repeated_cursors() {
        let mut state = LaterState::default();
        let generation = state.begin(LaterFilter::Saved, false);
        state.apply_page(generation, page(), false, None);
        let generation = state.begin(LaterFilter::Saved, true);
        state.apply_page(generation, page(), true, Some("next"));
        assert_eq!(state.visible.len(), 1);
        assert!(state.next_cursor.is_none());
    }
    #[test]
    fn completed_and_archived_items_leave_the_active_list_and_invalidate_fetches() {
        let mut state = LaterState::default();
        let generation = state.begin(LaterFilter::Saved, false);
        state.apply_page(generation, page(), false, None);
        let key = state.visible[0].clone();
        let mut item = state.items[&key].clone();
        item.date_completed = 123;
        state.updated(&key, Some(item));
        assert!(state.visible.is_empty());
        assert!(!state.apply_page(generation, page(), false, None));
    }
}
