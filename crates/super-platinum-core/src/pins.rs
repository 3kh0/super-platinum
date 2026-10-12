//! Pinned channel resources, scoped to the current authenticated account.
use crate::slack::{
    events::RawEvent,
    models::{File, Message},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PinnedItem {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub file: Option<File>,
    #[serde(default)]
    pub comment: Option<Value>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub created_by: Option<String>,
}
impl PinnedItem {
    pub fn key(&self) -> Option<String> {
        if let Some(ts) = self.message.as_ref().and_then(|m| m.ts.as_ref()) {
            return Some(format!("message:{ts}"));
        }
        if let Some(id) = self
            .comment
            .as_ref()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
        {
            return Some(format!("comment:{id}"));
        }
        self.file
            .as_ref()
            .and_then(|f| f.id.as_ref())
            .map(|id| format!("file:{id}"))
    }
}
#[derive(Debug, Default, Deserialize)]
pub struct PinsPage {
    #[serde(default)]
    pub items: Vec<PinnedItem>,
}
#[derive(Debug, Clone)]
pub struct PinUpdate {
    pub channel: String,
    pub item: PinnedItem,
    pub pinned: bool,
    pub event_ts: String,
}
impl PinUpdate {
    pub fn from_event(raw: &RawEvent) -> Option<Self> {
        let pinned = match raw.kind.as_str() {
            "pin_added" => true,
            "pin_removed" => false,
            _ => return None,
        };
        let value = raw.rest.get("item")?;
        let mut item: PinnedItem = serde_json::from_value(value.clone()).ok()?;
        let channel = raw
            .rest
            .get("channel_id")
            .or_else(|| raw.rest.get("channel"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| item.channel.clone())?;
        if item.kind == "message" && item.message.is_none() {
            item.message = value.get("ts").and_then(Value::as_str).map(|ts| Message {
                ts: Some(ts.into()),
                ..Default::default()
            });
        }
        item.key()?;
        let event_ts = raw
            .rest
            .get("event_ts")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        item.channel = Some(channel.clone());
        if item.created_by.is_none() {
            item.created_by = raw
                .rest
                .get("user")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if item.created.is_none() {
            item.created = event_ts.split('.').next().and_then(|s| s.parse().ok());
        }
        Some(Self {
            channel,
            item,
            pinned,
            event_ts,
        })
    }
}
#[derive(Debug, Default)]
pub struct PinsState {
    pub scope: Option<(u64, String)>,
    pub channel: Option<String>,
    pub generation: u64,
    pub items: BTreeMap<String, PinnedItem>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub busy: HashSet<(String, String)>,
    overrides: HashMap<String, HashMap<String, bool>>,
    versions: HashMap<String, HashMap<String, String>>,
    message_updates: HashMap<String, Option<Message>>,
    reaction_updates: HashMap<(String, String, String), bool>,
}
impl PinsState {
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t)| *e == epoch && t == team)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            *self = Self {
                scope: Some((epoch, team.into())),
                generation: self.generation.wrapping_add(1),
                ..Default::default()
            };
        }
    }
    pub fn begin(&mut self, channel: &str) -> u64 {
        // Only changes racing this fetch overlay its server snapshot.
        self.message_updates.clear();
        self.reaction_updates.clear();
        if self.channel.as_deref() != Some(channel) {
            self.channel = Some(channel.into());
            self.items.clear();
            self.message_updates.clear();
            self.loaded = false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.generation
    }
    pub fn apply(&mut self, generation: u64, page: PinsPage) -> bool {
        if self.generation != generation {
            return false;
        }
        self.items = page
            .items
            .into_iter()
            .filter_map(|mut item| {
                if let Some(message) = item.message.as_mut() {
                    match message
                        .ts
                        .as_ref()
                        .and_then(|ts| self.message_updates.get(ts))
                    {
                        Some(None) => return None,
                        Some(Some(update)) => crate::state::merge_message(message, update.clone()),
                        None => {}
                    }
                    for ((ts, user, name), added) in &self.reaction_updates {
                        if message.ts.as_ref() == Some(ts) {
                            crate::state::apply_message_reaction(message, user, name, *added);
                        }
                    }
                }
                item.key().map(|key| (key, item))
            })
            .collect();
        if let Some(channel) = &self.channel {
            self.overrides.remove(channel);
        }
        self.loading = false;
        self.loaded = true;
        self.error = None;
        true
    }
    pub fn is_pinned(&self, channel: &str, ts: &str, fallback: bool) -> bool {
        if let Some(value) = self
            .overrides
            .get(channel)
            .and_then(|values| values.get(ts))
        {
            return *value;
        }
        if self.loaded && self.channel.as_deref() == Some(channel) {
            return self.items.contains_key(&format!("message:{ts}"));
        }
        fallback
    }
    pub fn is_busy(&self, channel: &str, ts: &str) -> bool {
        self.busy.iter().any(|(c, t)| c == channel && t == ts)
    }
    pub fn version(&self, channel: &str, key: &str) -> Option<&str> {
        self.versions
            .get(channel)
            .and_then(|v| v.get(key))
            .map(String::as_str)
    }
    pub fn apply_event(&mut self, update: &PinUpdate) -> bool {
        let Some(key) = update.item.key() else {
            return false;
        };
        if !update.event_ts.is_empty() {
            if self
                .version(&update.channel, &key)
                .is_some_and(|old| !crate::state::cmp_ts(Some(&update.event_ts), Some(old)).is_gt())
            {
                return false;
            }
            self.versions
                .entry(update.channel.clone())
                .or_default()
                .insert(key, update.event_ts.clone());
        }
        self.set(&update.channel, update.item.clone(), update.pinned);
        true
    }
    pub fn set(&mut self, channel: &str, mut item: PinnedItem, pinned: bool) {
        if let Some(ts) = item.message.as_ref().and_then(|m| m.ts.as_ref()) {
            self.overrides
                .entry(channel.into())
                .or_default()
                .insert(ts.clone(), pinned);
        }
        if self.channel.as_deref() != Some(channel) {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        let Some(key) = item.key() else {
            return;
        };
        if pinned {
            if let Some(existing) = self.items.get(&key).and_then(|i| i.message.as_ref()) {
                if let Some(update) = item.message.take() {
                    let mut message = existing.clone();
                    crate::state::merge_message(&mut message, update);
                    item.message = Some(message);
                }
            }
            self.items.insert(key, item);
        } else {
            self.items.remove(&key);
        }
    }
    pub fn update_message(&mut self, channel: &str, message: &Message) {
        if self.channel.as_deref() != Some(channel) {
            return;
        }
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        if matches!(self.message_updates.get(ts), Some(None)) {
            return;
        }
        if self.loading || self.items.contains_key(&format!("message:{ts}")) {
            let update = self
                .message_updates
                .entry(ts.clone())
                .or_insert_with(|| Some(Message::default()));
            if let Some(existing) = update {
                crate::state::merge_message(existing, message.clone());
            }
        }
        if let Some(existing) = self
            .items
            .get_mut(&format!("message:{ts}"))
            .and_then(|i| i.message.as_mut())
        {
            crate::state::merge_message(existing, message.clone());
        }
    }
    pub fn delete_message(&mut self, channel: &str, ts: &str) {
        self.overrides
            .entry(channel.into())
            .or_default()
            .insert(ts.into(), false);
        if self.channel.as_deref() != Some(channel) {
            return;
        }
        self.message_updates.insert(ts.into(), None);
        self.items.remove(&format!("message:{ts}"));
    }
    pub fn apply_reaction(&mut self, channel: &str, ts: &str, user: &str, name: &str, added: bool) {
        if self.channel.as_deref() != Some(channel) {
            return;
        }
        if self.loading {
            self.reaction_updates
                .insert((ts.into(), user.into(), name.into()), added);
        }
        if let Some(message) = self
            .items
            .get_mut(&format!("message:{ts}"))
            .and_then(|i| i.message.as_mut())
        {
            crate::state::apply_message_reaction(message, user, name, added);
        }
    }
    pub fn ordered_items(&self) -> Vec<&PinnedItem> {
        let mut items = self.items.values().collect::<Vec<_>>();
        items.sort_by(|a, b| {
            b.created
                .cmp(&a.created)
                .then_with(|| a.key().cmp(&b.key()))
        });
        items
    }
}

pub fn set_message_pin(message: &mut Message, channel: &str, pinned: bool) {
    let mut channels = message
        .extra
        .get("pinned_to")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    channels.retain(|c| c.as_str() != Some(channel));
    if pinned {
        channels.push(Value::String(channel.into()));
    }
    message
        .extra
        .insert("pinned_to".into(), Value::Array(channels));
}

impl crate::CoreAppState {
    pub fn reconcile_pin_copies(&mut self, team: &str, channel: &str) {
        if !self.pins.matches_scope(self.account_epoch, team)
            || !self.pins.loaded
            || self.pins.channel.as_deref() != Some(channel)
        {
            return;
        }
        let pinned: HashSet<&str> = self
            .pins
            .items
            .values()
            .filter_map(|item| item.message.as_ref()?.ts.as_deref())
            .collect();
        let reconcile = |message: &mut Message| {
            let is_pinned = message.ts.as_deref().is_some_and(|ts| pinned.contains(ts));
            set_message_pin(message, channel, is_pinned);
        };
        if let Some(messages) = self
            .workspaces
            .get_mut(team)
            .and_then(|w| w.messages.get_mut(channel))
        {
            messages.messages.iter_mut().for_each(reconcile);
        }
        for ((t, c, _), messages) in &mut self.threads {
            if t == team && c == channel {
                messages.messages.iter_mut().for_each(reconcile);
            }
        }
    }
    pub fn update_pin_copies(&mut self, team: &str, channel: &str, ts: &str, pinned: bool) {
        if let Some(messages) = self
            .workspaces
            .get_mut(team)
            .and_then(|w| w.messages.get_mut(channel))
        {
            if let Some(message) = messages
                .messages
                .iter_mut()
                .find(|m| m.ts.as_deref() == Some(ts))
            {
                set_message_pin(message, channel, pinned);
            }
        }
        for ((t, c, _), messages) in &mut self.threads {
            if t == team && c == channel {
                if let Some(message) = messages
                    .messages
                    .iter_mut()
                    .find(|m| m.ts.as_deref() == Some(ts))
                {
                    set_message_pin(message, channel, pinned);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn item(ts: &str) -> PinnedItem {
        serde_json::from_value(json!({"type":"message","channel":"C1","message":{"ts":ts,"text":"original","user":"U1"},"created":1,"created_by":"U2"})).unwrap()
    }
    fn page() -> PinsPage {
        PinsPage {
            items: vec![item("1.0")],
        }
    }
    fn state() -> PinsState {
        let mut state = PinsState::default();
        state.ensure_scope(1, "T1");
        let generation = state.begin("C1");
        state.apply(generation, page());
        state
    }
    fn event(kind: &str, ts: &str) -> PinUpdate {
        let raw: RawEvent = serde_json::from_value(json!({"type":kind,"channel_id":"C1","user":"U2","item":{"type":"message","ts":"1.0"},"event_ts":ts})).unwrap();
        PinUpdate::from_event(&raw).unwrap()
    }
    #[test]
    fn account_and_channel_changes_reject_late_lists() {
        let mut state = state();
        let old = state.begin("C1");
        state.begin("C2");
        assert!(!state.apply(old, page()));
        assert!(!state.is_pinned("C2", "1.0", false));
        let old = state.begin("C2");
        state.busy.insert(("C2".into(), "1.0".into()));
        state.ensure_scope(2, "T1");
        assert!(!state.apply(old, page()));
        assert!(state.busy.is_empty());
        assert!(state.channel.is_none());
    }
    #[test]
    fn realtime_pin_change_invalidates_list_and_rejects_old_events() {
        let mut state = state();
        let pending = state.begin("C1");
        assert!(state.apply_event(&event("pin_removed", "20.0")));
        assert!(!state.apply(pending, page()));
        assert!(!state.apply_event(&event("pin_added", "19.0")));
        assert!(!state.apply_event(&event("pin_added", "20.0")));
        assert!(!state.is_pinned("C1", "1.0", true));
        assert!(state.apply_event(&event("pin_added", "21.0")));
        assert!(state.is_pinned("C1", "1.0", false));
        assert_eq!(state.version("C1", "message:1.0"), Some("21.0"));
    }
    #[test]
    fn edits_and_deletion_during_load_survive_the_response() {
        let mut state = state();
        let generation = state.begin("C1");
        let update: Message = serde_json::from_value(json!({"ts":"1.0","text":"edited"})).unwrap();
        state.update_message("C1", &update);
        assert!(state.apply(generation, page()));
        let message = state.items["message:1.0"].message.as_ref().unwrap();
        assert_eq!(message.text.as_deref(), Some("edited"));
        assert_eq!(message.user.as_deref(), Some("U1"));
        // An older overlay must not overwrite a subsequent authoritative fetch.
        let generation = state.begin("C1");
        state.apply(generation, page());
        assert_eq!(
            state.items["message:1.0"]
                .message
                .as_ref()
                .unwrap()
                .text
                .as_deref(),
            Some("original")
        );
        let generation = state.begin("C1");
        state.delete_message("C1", "1.0");
        state.update_message("C1", &update);
        state.apply(generation, page());
        assert!(state.items.is_empty());
    }
    #[test]
    fn reactions_during_load_overlay_unknown_pinned_messages_idempotently() {
        let mut state = state();
        let generation = state.begin("C1");
        state.apply_reaction("C1", "1.0", "U1", "eyes", true);
        state.apply_reaction("C1", "1.0", "U1", "eyes", true);
        state.apply(generation, page());
        let message = state.items["message:1.0"].message.as_ref().unwrap();
        assert_eq!(message.reactions[0].count, 1);
        let generation = state.begin("C1");
        state.apply_reaction("C1", "1.0", "U1", "eyes", false);
        let mut stale = page();
        crate::state::apply_message_reaction(
            stale.items[0].message.as_mut().unwrap(),
            "U1",
            "eyes",
            true,
        );
        state.apply(generation, stale);
        assert!(
            state.items["message:1.0"]
                .message
                .as_ref()
                .unwrap()
                .reactions
                .is_empty()
        );
    }
    #[test]
    fn legacy_pins_and_nullable_fields_remain_readable() {
        let page: PinsPage = serde_json::from_value(json!({"items":[
            {"type":"file","file":{"id":"F1","title":"Guide"},"created_by":null},
            {"type":"file_comment","file":{"id":"F1"},"comment":{"id":"FC1","comment":"Review"},"created":null},
            {"type":"future"}
        ]})).unwrap();
        let mut state = state();
        let generation = state.begin("C1");
        state.apply(generation, page);
        assert_eq!(state.items.len(), 2);
        assert!(state.items.contains_key("file:F1"));
        assert!(state.items.contains_key("comment:FC1"));
    }
    #[test]
    fn badge_updates_preserve_other_channel_pins() {
        let mut message = Message::default();
        set_message_pin(&mut message, "C2", true);
        set_message_pin(&mut message, "C1", true);
        set_message_pin(&mut message, "C1", true);
        assert_eq!(message.extra["pinned_to"], json!(["C2", "C1"]));
        set_message_pin(&mut message, "C1", false);
        assert_eq!(message.extra["pinned_to"], json!(["C2"]));
    }
}
