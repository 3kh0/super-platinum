//! Shared per-field ordering for already-owned preview copies. Data only.
use crate::slack::models::Message;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct PreviewUpdate {
    pub(crate) revision: u64,
    pub(crate) message: Option<Message>,
    pub(crate) fields: HashMap<&'static str, u64>,
    pub(crate) extras: HashMap<String, u64>,
    pub(crate) body: crate::state::DeferredBody,
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
    pub(crate) fn record_fields(&mut self, message: &Message, revision: u64) {
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
    pub(crate) fn since(&self, revision: u64) -> Option<Message> {
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
