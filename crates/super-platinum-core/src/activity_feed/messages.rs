//! Message copies for existing Activity references; no feed/action authority.
use super::{ActivityState, ChannelId, HashMap, HashSet, Message, MessageTs};
use crate::{
    CoreAppState,
    file_actions::{FileDisplayRead, tombstone, visit_message},
    slack::{
        events::{MessageUpdate, MessageUpdateKind},
        models::{File, MessagesListPage},
    },
    state::{PreviewUpdate, merge_message_update, original_edit_matches},
};
use std::sync::Arc;

type Key = (ChannelId, MessageTs);

#[derive(Debug, Clone, Default)]
pub(super) struct MessageSync {
    scope: Arc<()>,
    revision: u64,
    updates: HashMap<Key, PreviewUpdate>,
    accepted: HashMap<Key, u64>,
    edited_body: HashSet<Key>,
}

/// Existing read coordinates and local field order only. The native adapter
/// retains credential/account/transport/load guards before adoption.
pub struct ActivityMessageRead {
    scope: Arc<()>,
    epoch: u64,
    team: String,
    sequence: u64,
    revision: u64,
    keys: HashSet<Key>,
}

impl ActivityState {
    fn message_references(&self) -> HashSet<Key> {
        self.items
            .iter()
            .flat_map(|i| {
                i.channel()
                    .into_iter()
                    .flat_map(move |c| i.request_ts().into_iter().map(move |ts| (c.to_owned(), ts)))
            })
            .collect()
    }
    pub(super) fn prune_message_updates(&mut self) {
        let keys = self.message_references();
        self.sync.messages.updates.retain(|k, _| keys.contains(k));
        self.sync.messages.accepted.retain(|k, _| keys.contains(k));
        self.sync.messages.edited_body.retain(|k| keys.contains(k));
    }
    fn next_message_revision(&mut self) -> Option<u64> {
        self.sync.messages.revision = self.sync.messages.revision.checked_add(1)?;
        Some(self.sync.messages.revision)
    }
    pub fn begin_message_hydration(
        &mut self,
        ids: &[(ChannelId, Vec<MessageTs>)],
    ) -> Option<ActivityMessageRead> {
        let (epoch, team) = self.sync.scope.clone()?;
        let references = self.message_references();
        let keys = ids
            .iter()
            .flat_map(|(c, ts)| ts.iter().map(move |t| (c.clone(), t.clone())))
            .collect::<HashSet<_>>();
        if keys.is_empty()
            || keys
                .iter()
                .any(|k| k.0.is_empty() || k.1.is_empty() || !references.contains(k))
        {
            return None;
        }
        Some(ActivityMessageRead {
            scope: self.sync.messages.scope.clone(),
            epoch,
            team,
            sequence: self.load_seq,
            revision: self.next_message_revision()?,
            keys,
        })
    }
    fn accepts_message_read(&self, read: &ActivityMessageRead) -> bool {
        self.matches_scope(read.epoch, &read.team)
            && self.load_seq == read.sequence
            && Arc::ptr_eq(&self.sync.messages.scope, &read.scope)
    }
    pub fn update_original_message(
        &mut self,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
    ) {
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        let key = (channel.to_owned(), ts.clone());
        let Some(original) = original else {
            return;
        };
        if original.kind() != MessageUpdateKind::Changed {
            return;
        }
        if !self.message_references().contains(&key) {
            return;
        }
        let seed = Message {
            ts: Some(ts.clone()),
            ..Default::default()
        };
        let stored = self.hydrated.get(&key).unwrap_or(&seed);
        if !original_edit_matches(stored, channel, message, original) {
            return;
        }
        let Some(revision) = self.next_message_revision() else {
            return;
        };
        let pending = self
            .sync
            .messages
            .updates
            .entry(key.clone())
            .or_insert_with(|| PreviewUpdate {
                message: Some(seed),
                ..Default::default()
            });
        pending.revision = revision;
        pending.record_fields(message, revision);
        pending
            .body
            .record(channel, message, Some(original), revision);
        pending.body.merge_into(
            pending.message.as_mut().unwrap(),
            channel,
            message.clone(),
            0,
        );
        if let Some(stored) = self.hydrated.get_mut(&key) {
            merge_message_update(stored, channel, message.clone(), Some(original));
        }
        // Counts alone must not suppress a legitimate special-card fallback.
        if message.text.is_some()
            || !message.blocks.is_empty()
            || !message.attachments.is_empty()
            || !message.files.is_empty()
            || ["blocks", "attachments"].iter().any(|f| {
                original
                    .raw()
                    .get(*f)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(Vec::is_empty)
            })
        {
            self.sync.messages.edited_body.insert(key);
        }
    }
    pub fn has_original_body_edit(&self, channel: &str, ts: &str) -> bool {
        self.sync
            .messages
            .edited_body
            .contains(&(channel.into(), ts.into()))
    }
    pub(crate) fn visit_message_files(&mut self, update: &mut impl FnMut(&mut File)) {
        for m in self.hydrated.values_mut() {
            visit_message(m, update);
        }
        for p in self.sync.messages.updates.values_mut() {
            if let Some(m) = &mut p.message {
                visit_message(m, update);
            }
        }
    }
}

impl CoreAppState {
    /// Positive deletion evidence from the existing native workspace reducer.
    /// This neither creates a tombstone nor grants Source/canonical authority.
    pub fn activity_message_was_deleted(&self, channel: &str, ts: &str) -> bool {
        if channel.is_empty() || ts.is_empty() {
            return false;
        }
        self.active_team
            .as_deref()
            .filter(|team| self.activity.matches_scope(self.account_epoch, team))
            .and_then(|team| self.workspaces.get(team))
            .and_then(|ws| ws.messages.get(channel))
            .is_some_and(|messages| messages.was_deleted(ts))
    }

    pub fn activity_item_message_deleted(&self, item: &crate::slack::models::ActivityItem) -> bool {
        item.channel()
            .zip(item.preview_ts())
            .is_some_and(|(channel, ts)| self.activity_message_was_deleted(channel, ts))
    }

    pub fn delete_activity_message(&mut self, team: &str, channel: &str, ts: &str) {
        if self.active_team.as_deref() != Some(team)
            || !self.activity_message_was_deleted(channel, ts)
        {
            return;
        }
        let key = (channel.to_owned(), ts.to_owned());
        self.activity.hydrated.remove(&key);
        self.activity.sync.messages.updates.remove(&key);
        self.activity.sync.messages.accepted.remove(&key);
        self.activity.sync.messages.edited_body.remove(&key);
    }

    pub fn update_activity_original_message(
        &mut self,
        team: &str,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
    ) {
        if self.active_team.as_deref() != Some(team)
            || !self.activity.matches_scope(self.account_epoch, team)
            || self.activity_message_was_deleted(channel, message.ts.as_deref().unwrap_or(""))
        {
            return;
        }
        let Some(ws) = self.workspaces.get(team) else {
            return;
        };
        let mut message = message.clone();
        visit_message(&mut message, &mut |f| {
            if let Some(id) = f.id.as_deref()
                && ws.deleted_files.contains(id)
            {
                *f = tombstone(id);
            }
        });
        self.activity
            .update_original_message(channel, &message, original);
    }
    pub fn apply_activity_message_page(
        &mut self,
        team: &str,
        read: ActivityMessageRead,
        mut page: MessagesListPage,
        files: FileDisplayRead,
    ) -> Result<bool, &'static str> {
        if self.active_team.as_deref() != Some(team)
            || read.epoch != self.account_epoch
            || read.team != team
            || !self.activity.accepts_message_read(&read)
        {
            return Ok(false);
        }
        let mut seen = HashSet::new();
        // Refuse malformed/foreign/duplicate response coordinates before any
        // adoption. Public defaults or an unrequested body never seed a cache.
        for (channel, data) in &page.messages_data {
            if !read.keys.iter().any(|k| &k.0 == channel) {
                return Err("activity_message_response_unrequested");
            }
            for m in &data.messages {
                let Some(ts) = m.ts.as_ref() else {
                    return Err("activity_message_response_identity");
                };
                let key = (channel.clone(), ts.clone());
                if !read.keys.contains(&key)
                    || !seen.insert(key)
                    || m.channel.as_deref().is_some_and(|c| c != channel)
                {
                    return Err("activity_message_response_identity");
                }
            }
        }
        let overlay = self.finish_file_display_read(team, files)?;
        let references = self.activity.message_references();
        for (channel, data) in &mut page.messages_data {
            for message in &mut data.messages {
                let key = (channel.clone(), message.ts.clone().unwrap());
                if !references.contains(&key)
                    || self.activity_message_was_deleted(channel, &key.1)
                    || self
                        .activity
                        .sync
                        .messages
                        .accepted
                        .get(&key)
                        .is_some_and(|r| *r > read.revision)
                {
                    continue;
                }
                visit_message(message, &mut |f| overlay.apply_file(f));
                if let Some(p) = self.activity.sync.messages.updates.get(&key)
                    && let Some(update) = p.since(read.revision)
                    && p.revision > read.revision
                {
                    p.body.merge_into(message, channel, update, read.revision);
                }
                if let Some(ws) = self.workspaces.get(team) {
                    visit_message(message, &mut |f| {
                        if let Some(id) = f.id.as_deref()
                            && ws.deleted_files.contains(id)
                        {
                            *f = tombstone(id);
                        }
                    });
                }
                self.activity.hydrated.insert(key.clone(), message.clone());
                self.activity
                    .sync
                    .messages
                    .accepted
                    .insert(key, read.revision);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
