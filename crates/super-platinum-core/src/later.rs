//! Slack-backed saved items. UI state is scoped to one account and workspace.
use crate::slack::models::{File, Message};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
mod files;
mod membership;
pub use files::FileHydrationFailure;
pub use membership::{SavedMembership, SavedRequest};

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
    pub fn file(id: impl Into<String>) -> Self {
        Self {
            item_type: "file".into(),
            item_id: id.into(),
            ts: String::new(),
        }
    }
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
        } else if self.date_completed > 0
            || self.state == "completed"
            || self.todo_state == "completed"
        {
            LaterFilter::Completed
        } else {
            LaterFilter::Saved
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SavedCounts {
    /// Canonical overdue count; omission is not an authoritative zero.
    #[serde(default)]
    pub uncompleted_overdue_count: Option<u32>,
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
struct PreviewUpdate {
    revision: u64,
    message: Option<Message>,
    fields: HashMap<&'static str, u64>,
    extras: HashMap<String, u64>,
}

// Keep partial-event fields independent: a count-only event must not renew an
// earlier text, reaction or file update over a fresher history response.
macro_rules! preview_fields {
    ($action:ident) => {
        $action!(user, bot_id, username, bot_profile, icons, kind, subtype,
            client_msg_id, text, team, channel, thread_ts, parent_user_id,
            reply_count, reply_users_count, latest_reply, edited, message;
            reply_users, reactions, blocks, files, attachments);
    };
}
impl PreviewUpdate {
    fn record_fields(&mut self, message: &Message, revision: u64) {
        macro_rules! record {
            ($($optional:ident),*; $($vector:ident),*) => {
                $(if message.$optional.is_some() {
                    self.fields.insert(stringify!($optional), revision);
                })*
                $(if !message.$vector.is_empty() {
                    self.fields.insert(stringify!($vector), revision);
                })*
            };
        }
        preview_fields!(record);
        for key in message.extra.keys() {
            self.extras.insert(key.clone(), revision);
        }
    }
    fn since(&self, revision: u64) -> Option<Message> {
        let mut update = self.message.clone()?;
        macro_rules! retain {
            ($($optional:ident),*; $($vector:ident),*) => {
                $(if self.fields.get(stringify!($optional)).is_none_or(|at| *at <= revision) {
                    update.$optional = None;
                })*
                $(if self.fields.get(stringify!($vector)).is_none_or(|at| *at <= revision) {
                    update.$vector.clear();
                })*
            };
        }
        preview_fields!(retain);
        update
            .extra
            .retain(|key, _| self.extras.get(key).is_some_and(|at| *at > revision));
        Some(update)
    }
}

#[derive(Debug)]
struct FileUpdate {
    revision: u64,
    title: String,
    name: Option<(u64, String)>,
}

#[derive(Debug, Default)]
pub struct LaterState {
    pub scope: Option<(u64, String)>,
    pub generation: u64,
    pub filter: LaterFilter,
    pub items: HashMap<SavedKey, SavedItem>,
    pub visible: Vec<SavedKey>,
    pub hydrated: HashMap<SavedKey, Message>,
    /// Standalone saved-file metadata, never synthetic message identities.
    pub files: HashMap<String, File>,
    pub file_errors: HashMap<String, FileHydrationFailure>,
    file_hydrated_revisions: HashMap<String, u64>,
    membership: HashMap<SavedKey, Option<SavedItem>>,
    item_revisions: HashMap<SavedKey, u64>,
    item_revision: u64,
    scope_revision: u64,
    hydrated_revisions: HashMap<SavedKey, u64>,
    preview_updates: HashMap<SavedKey, PreviewUpdate>,
    hydration_revision: u64,
    // One latest value per changed file, not an event log. Retained until scope
    // reset because multiple hydration requests may still be in flight.
    file_updates: HashMap<String, FileUpdate>,
    deleted_files: HashSet<String>,
    /// Invalidates mounted saved-file recovery inventories after controller outcomes.
    pub file_actions_revision: u64,
    pub busy: HashSet<SavedKey>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub next_cursor: Option<String>,
    pub counts: SavedCounts,
    pub last_event_ts: Option<String>,
}
impl LaterState {
    /// Current revision for inspection. Use begin_hydration for a new request.
    pub fn hydration_revision(&self) -> u64 {
        self.hydration_revision
    }
    /// Capture immediately before requesting history. Each request gets a
    /// distinct token; only later changes may override that response.
    pub fn begin_hydration(&mut self) -> u64 {
        self.next_hydration_revision()
    }
    fn next_hydration_revision(&mut self) -> u64 {
        self.hydration_revision = self.hydration_revision.wrapping_add(1);
        self.hydration_revision
    }
    pub(crate) fn visit_files(&mut self, update: &mut impl FnMut(&mut File)) {
        for file in self.files.values_mut() {
            update(file);
        }
        for message in self.hydrated.values_mut().chain(
            self.preview_updates
                .values_mut()
                .filter_map(|update| update.message.as_mut()),
        ) {
            crate::file_actions::visit_message(message, update);
        }
    }
    pub(crate) fn record_file_rename(&mut self, id: &str, title: &str, name: Option<&str>) {
        let revision = self.next_hydration_revision();
        let name = name.map(|name| (revision, name.into())).or_else(|| {
            self.file_updates
                .get(id)
                .and_then(|update| update.name.clone())
        });
        self.file_updates.insert(
            id.into(),
            FileUpdate {
                revision,
                title: title.into(),
                name,
            },
        );
    }
    pub(crate) fn record_file_deletion(&mut self, id: &str) {
        self.deleted_files.insert(id.into());
        self.file_updates.remove(id);
        if let Some(file) = self.files.get_mut(id) {
            *file = crate::file_actions::tombstone(id);
        }
        self.file_errors
            .insert(id.into(), FileHydrationFailure::Deleted);
    }
    pub fn update_message(&mut self, channel: &str, message: &Message) {
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        let key = SavedKey::message(channel, ts);
        if !self.items.contains_key(&key) && !self.hydrated.contains_key(&key) {
            return;
        }
        if self
            .preview_updates
            .get(&key)
            .is_some_and(|update| update.message.is_none())
        {
            return;
        }
        let mut message = message.clone();
        crate::file_actions::visit_message(&mut message, &mut |file| {
            if let Some(id) = file.id.as_deref()
                && self.deleted_files.contains(id)
            {
                *file = crate::file_actions::tombstone(id);
            }
        });
        let revision = self.next_hydration_revision();
        let pending = self
            .preview_updates
            .entry(key.clone())
            .or_insert_with(|| PreviewUpdate {
                message: Some(Message::default()),
                ..Default::default()
            });
        pending.revision = revision;
        pending.record_fields(&message, revision);
        if let Some(update) = &mut pending.message {
            crate::state::merge_message(update, message.clone());
        }
        if let Some(existing) = self.hydrated.get_mut(&key) {
            crate::state::merge_message(existing, message);
        }
    }
    pub fn delete_message(&mut self, channel: &str, ts: &str) {
        let key = SavedKey::message(channel, ts);
        if !self.items.contains_key(&key) && !self.hydrated.contains_key(&key) {
            return;
        }
        let revision = self.next_hydration_revision();
        self.preview_updates.insert(
            key.clone(),
            PreviewUpdate {
                revision,
                ..Default::default()
            },
        );
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
    pub fn hydrate_message(&mut self, key: SavedKey, message: Message) {
        self.hydrate_message_since(key, message, 0);
    }
    pub fn hydrate_message_since(
        &mut self,
        key: SavedKey,
        mut message: Message,
        requested_revision: u64,
    ) {
        if self
            .hydrated_revisions
            .get(&key)
            .is_some_and(|accepted| *accepted > requested_revision)
        {
            return;
        }
        crate::file_actions::visit_message(&mut message, &mut |file| {
            let Some(id) = file.id.as_deref() else { return };
            if self.deleted_files.contains(id) {
                *file = crate::file_actions::tombstone(id);
            } else if !crate::file_actions::deleted(file)
                && let Some(update) = self.file_updates.get(id)
            {
                if update.revision > requested_revision {
                    file.title = Some(update.title.clone());
                }
                if let Some((revision, name)) = &update.name
                    && *revision > requested_revision
                {
                    file.name = Some(name.clone());
                }
            }
        });
        if let Some(pending) = self.preview_updates.get(&key) {
            if let Some(update) = pending.since(requested_revision) {
                if pending.revision > requested_revision {
                    crate::state::merge_message(&mut message, update);
                }
            } else {
                message = Message {
                    ts: Some(key.ts.clone()),
                    user: message.user,
                    text: Some("This message was deleted.".into()),
                    ..Default::default()
                }
            }
        }
        self.hydrated_revisions
            .insert(key.clone(), requested_revision);
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
            self.hydrated_revisions.remove(&key);
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
            let scope_revision = self.scope_revision.wrapping_add(1);
            *self = Self {
                scope: Some((epoch, team.into())),
                scope_revision,
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
            self.record_membership(&key, Some(item.clone()));
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
        if item.as_ref().is_some_and(|item| item.key() != *key) {
            return;
        }
        self.record_membership(key, item.clone());
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
mod tests;
