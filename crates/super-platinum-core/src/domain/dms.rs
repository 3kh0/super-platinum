//! Selected DM preview body copies. No latest selection, membership or navigation.
use super::{DmEntry, DmsState, HashMap, Message};
use crate::{
    CoreAppState,
    file_actions::{tombstone, visit_message},
    slack::{events::MessageUpdate, models::File},
    state::{DeferredBody, merge_message_update, original_edit_matches},
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope {
    epoch: u64,
    team: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PreviewUpdates {
    // Data provenance for the existing row snapshots, never read/action authority.
    published: Option<Scope>,
    read: Option<Read>,
}

#[derive(Debug, Clone)]
struct Read {
    scope: Scope,
    sequence: u64,
    updates: HashMap<String, Patch>,
}

#[derive(Debug, Clone)]
struct Patch {
    message: Message,
    body: DeferredBody,
    advanced: bool,
}

fn same_preview(entry: &DmEntry, channel: &str, ts: &str) -> bool {
    !channel.is_empty()
        && !ts.is_empty()
        && entry.id == channel
        && entry.latest.as_deref() == Some(ts)
        && entry.channel.as_ref().is_none_or(|c| c.id == channel)
        && entry.message.as_ref().is_some_and(|m| {
            m.ts.as_deref() == Some(ts) && m.channel.as_deref().is_none_or(|c| c == channel)
        })
}

impl DmsState {
    /// The native reader retains all original account/transport/visit fences.
    /// Starting a read retires only its prior compact replay interval.
    pub fn begin_preview_read(&mut self, epoch: u64, team: &str, sequence: u64) {
        self.preview_updates.read = Some(Read {
            scope: Scope {
                epoch,
                team: team.into(),
            },
            sequence,
            updates: HashMap::new(),
        });
    }

    pub fn finish_preview_read(&mut self, sequence: u64) {
        if self
            .preview_updates
            .read
            .as_ref()
            .is_some_and(|r| r.sequence == sequence)
        {
            self.preview_updates.read = None;
        }
    }

    /// Publish through the original page's clear/upsert behavior. Only edits
    /// observed after this exact read began may overlay its same selected body.
    pub fn publish_preview_page(
        &mut self,
        epoch: u64,
        team: &str,
        sequence: u64,
        append: bool,
        mut entries: Vec<DmEntry>,
    ) {
        let scope = Scope {
            epoch,
            team: team.into(),
        };
        let read = self
            .preview_updates
            .read
            .as_ref()
            .is_some_and(|r| r.sequence == sequence && r.scope == scope)
            .then(|| self.preview_updates.read.take().unwrap());
        if let Some(read) = read {
            let mut counts = HashMap::<String, usize>::new();
            for entry in &entries {
                *counts.entry(entry.id.clone()).or_default() += 1;
            }
            for entry in &mut entries {
                if counts[&entry.id] != 1 {
                    continue;
                }
                if let Some(patch) = read.updates.get(&entry.id) {
                    patch.apply(entry);
                }
            }
        }
        if !append {
            self.entries.clear();
            self.preview_updates.published = Some(scope);
        }
        for entry in entries {
            self.upsert(entry);
        }
    }

    pub fn update_original_preview(
        &mut self,
        epoch: u64,
        team: &str,
        channel: &str,
        message: &Message,
        original: &MessageUpdate,
    ) -> bool {
        let scope = Scope {
            epoch,
            team: team.into(),
        };
        if self.preview_updates.published.as_ref() != Some(&scope) {
            return false;
        }
        let Some(ts) = message.ts.as_deref() else {
            return false;
        };
        // Duplicate row identities cannot choose a body owner, even when only
        // one happens to contain this timestamp. No insertion or sorting here.
        let mut matching = self.entries.iter_mut().filter(|e| e.id == channel);
        let Some(entry) = matching.next() else {
            return false;
        };
        if matching.next().is_some() || !same_preview(entry, channel, ts) {
            return false;
        }
        let existing = entry.message.as_mut().unwrap();
        if !original_edit_matches(existing, channel, message, original) {
            return false;
        }
        merge_message_update(existing, channel, message.clone(), Some(original));
        if let Some(read) = &mut self.preview_updates.read
            && read.scope == scope
        {
            let patch = read.updates.entry(channel.into()).or_insert_with(|| Patch {
                // The existing merge never assigns ts; bind the delta at birth.
                message: Message {
                    ts: Some(ts.into()),
                    ..Default::default()
                },
                body: DeferredBody::default(),
                advanced: false,
            });
            if patch.message.ts.as_deref() == Some(ts) {
                patch.body.record(channel, message, Some(original), 1);
                patch
                    .body
                    .merge_into(&mut patch.message, channel, message.clone(), 0);
            }
        }
        true
    }

    pub(crate) fn visit_preview_files(&mut self, update: &mut impl FnMut(&mut File)) {
        for entry in &mut self.entries {
            if let Some(message) = &mut entry.message {
                visit_message(message, update);
            }
        }
        if let Some(read) = &mut self.preview_updates.read {
            for patch in read.updates.values_mut() {
                visit_message(&mut patch.message, update);
            }
        }
    }
}

impl CoreAppState {
    /// Read-only native display evidence for the current owned selected body.
    /// This does not create a Source tombstone, membership or action authority.
    pub fn dm_preview_was_deleted(&self, entry: &DmEntry) -> bool {
        let Some(team) = self.active_team.as_deref() else {
            return false;
        };
        if self.dms.preview_updates.published.as_ref()
            != Some(&Scope {
                epoch: self.account_epoch,
                team: team.into(),
            })
        {
            return false;
        }
        let Some(ts) = entry.latest.as_deref() else {
            return false;
        };
        if !same_preview(entry, &entry.id, ts) {
            return false;
        }
        let mut rows = self.dms.entries.iter().filter(|row| row.id == entry.id);
        let Some(current) = rows.next() else {
            return false;
        };
        if rows.next().is_some() || !same_preview(current, &entry.id, ts) {
            return false;
        }
        self.workspaces
            .get(team)
            .and_then(|ws| ws.messages.get(&entry.id))
            .is_some_and(|messages| messages.was_deleted(ts))
    }

    pub fn update_dm_original_preview(
        &mut self,
        team: &str,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
    ) {
        if self.active_team.as_deref() != Some(team) {
            return;
        }
        let Some(original) = original else {
            return;
        };
        let Some(workspace) = self.workspaces.get(team) else {
            return;
        };
        let mut message = message.clone();
        visit_message(&mut message, &mut |file| {
            if let Some(id) = file.id.as_deref()
                && workspace.deleted_files.contains(id)
            {
                *file = tombstone(id);
            }
        });
        self.dms
            .update_original_preview(self.account_epoch, team, channel, &message, original);
    }
}

#[cfg(test)]
mod tests;

mod bot_incoming;
mod incoming;
