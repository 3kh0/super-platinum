//! Explicit authored bot arrivals update existing native DM row data only.
use super::incoming::{ordered_ts, truthy};
use super::{CoreAppState, DeferredBody, DmsState, Message, Patch, Scope, same_preview};
use crate::file_actions::{tombstone, visit_message};

fn supported_bot(message: &Message, self_user: &str, channel: &str) -> bool {
    let Some(bot) = message.bot_id.as_deref().filter(|id| !id.is_empty()) else {
        return false;
    };
    // A separate conservative data subset of the captured latest predicate.
    // Native ownership remains at the caller; these fields are not SourceReady.
    !self_user.is_empty()
        && !channel.is_empty()
        && message.channel.as_deref() == Some(channel)
        && message.kind.as_deref() == Some("message")
        && message.subtype.as_deref() == Some("bot_message")
        && message
            .user
            .as_deref()
            .is_some_and(|u| !u.is_empty() && u != self_user)
        && message.bot_profile.as_ref().is_none_or(|p| {
            p.id.as_deref().is_none_or(|id| id == bot)
                && p.user_id
                    .as_deref()
                    .is_none_or(|user| Some(user) == message.user.as_deref())
        })
        && message.ts.as_deref().is_some_and(ordered_ts)
        && message
            .thread_ts
            .as_ref()
            .is_none_or(|root| Some(root) == message.ts.as_ref())
        && !crate::channel_threads::hidden(message)
        && !truthy(message.extra.get("_hidden_reply"))
        && !truthy(message.extra.get("dnd_suppressed"))
        && !truthy(message.extra.get("is_ephemeral"))
        && !["comment", "workflow_id", "metadata"]
            .iter()
            .any(|k| message.extra.contains_key(*k))
        && (message
            .text
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
            || !message.blocks.is_empty())
}

impl DmsState {
    pub fn update_bot_preview(
        &mut self,
        epoch: u64,
        team: &str,
        self_user: &str,
        channel: &str,
        message: &Message,
    ) -> bool {
        let scope = Scope {
            epoch,
            team: team.into(),
        };
        if self.preview_updates.published.as_ref() != Some(&scope)
            || !supported_bot(message, self_user, channel)
        {
            return false;
        }
        // Retain the existing ordinary path's row/replay/sort fences verbatim.
        let ts = message.ts.as_deref().unwrap();
        let mut rows = self.entries.iter_mut().filter(|e| e.id == channel);
        let Some(entry) = rows.next() else {
            return false;
        };
        if rows.next().is_some() {
            return false;
        }
        let Some(old) = entry.latest.as_deref() else {
            return false;
        };
        if !ordered_ts(old) || ts <= old || !same_preview(entry, channel, old) {
            return false;
        }
        entry.latest = Some(ts.into());
        entry.message = Some(message.clone());
        if let Some(read) = &mut self.preview_updates.read
            && read.scope == scope
        {
            read.updates.insert(
                channel.into(),
                Patch {
                    message: message.clone(),
                    body: DeferredBody::default(),
                    advanced: true,
                },
            );
        }
        self.sort();
        true
    }
}

impl CoreAppState {
    /// Only the existing admitted native bot frame calls this data-only path.
    pub fn update_dm_bot_preview(&mut self, team: &str, channel: &str, message: &Message) {
        if self.active_team.as_deref() != Some(team)
            || self.dms.preview_updates.published.as_ref()
                != Some(&Scope {
                    epoch: self.account_epoch,
                    team: team.into(),
                })
            || !self.dms.entries.iter().any(|e| e.id == channel)
        {
            return;
        }
        let Some(workspace) = self.workspaces.get(team) else {
            return;
        };
        if message.ts.as_deref().is_some_and(|ts| {
            workspace
                .messages
                .get(channel)
                .is_some_and(|b| b.was_deleted(ts))
        }) {
            return;
        }
        let mut message = message.clone();
        visit_message(&mut message, &mut |file| {
            if let Some(id) = file.id.as_deref()
                && workspace.deleted_files.contains(id)
            {
                *file = tombstone(id);
            }
        });
        self.dms.update_bot_preview(
            self.account_epoch,
            team,
            &workspace.self_user_id,
            channel,
            &message,
        );
    }
}
