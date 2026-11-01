//! Ordinary accepted native arrivals update existing DM row data, never membership.
use super::{CoreAppState, DeferredBody, DmEntry, DmsState, Message, Patch, Scope, same_preview};
use crate::file_actions::{tombstone, visit_message};
use serde_json::Value;

// Comparable lexical shape only, not origin, namespace or permission evidence.
pub(super) fn ordered_ts(ts: &str) -> bool {
    ts.len() == 17
        && ts.as_bytes()[10] == b'.'
        && ts[..10].bytes().all(|b| b.is_ascii_digit())
        && ts[11..].bytes().all(|b| b.is_ascii_digit())
        && ts[..10].bytes().any(|b| b != b'0')
}

pub(super) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}

fn ordinary(message: &Message, self_user: &str, channel: &str) -> bool {
    // A conservative data subset of the captured latest predicate. Own authors
    // satisfy its first arm; ordinary peers satisfy its alternate unread arm
    // independently of the unknown system-messages experiment. This is not a
    // normalized Source receipt. Native event/account ownership remains outside.
    !self_user.is_empty()
        && !channel.is_empty()
        && message.channel.as_deref() == Some(channel)
        && message.kind.as_deref() == Some("message")
        && message.subtype.is_none()
        && message.user.as_deref().is_some_and(|u| !u.is_empty())
        && message.bot_id.is_none()
        && message.bot_profile.is_none()
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

impl Patch {
    pub(super) fn apply(&self, entry: &mut DmEntry) {
        let Some(ts) = self.message.ts.as_deref() else {
            return;
        };
        if self.advanced {
            let Some(old) = entry.latest.as_deref() else {
                return;
            };
            if !ordered_ts(old) || !same_preview(entry, &entry.id, old) {
                return;
            }
            if ts > old {
                // A different message identity cannot inherit the old body's
                // blocks/reactions. Keep the response's membership/metadata.
                entry.latest = Some(ts.into());
                entry.message = Some(self.message.clone());
                return;
            }
        }
        if same_preview(entry, &entry.id, ts) {
            self.body.merge_into(
                entry.message.as_mut().unwrap(),
                &entry.id,
                self.message.clone(),
                0,
            );
        }
    }
}

impl DmsState {
    pub fn update_ordinary_preview(
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
            || !ordinary(message, self_user, channel)
        {
            return false;
        }
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
    /// Called only after the existing native socket/account/generation admission.
    /// Row data only: no new row, read, navigation, action or readiness grant.
    pub fn update_dm_ordinary_preview(&mut self, team: &str, channel: &str, message: &Message) {
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
        let mut message = message.clone();
        visit_message(&mut message, &mut |file| {
            if let Some(id) = file.id.as_deref()
                && workspace.deleted_files.contains(id)
            {
                *file = tombstone(id);
            }
        });
        self.dms.update_ordinary_preview(
            self.account_epoch,
            team,
            &workspace.self_user_id,
            channel,
            &message,
        );
    }
}

#[cfg(test)]
mod tests;
