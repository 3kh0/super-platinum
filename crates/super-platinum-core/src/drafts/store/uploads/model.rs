use super::*;
use serde::{Deserialize, Serialize};
/// Display metadata only: no local path, signed URL, token, cookie, or bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub account_id: String,
    pub team_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub thread_ts: Option<String>,
    pub reply_broadcast: bool,
    pub client_msg_id: String,
    pub optimistic_ts: String,
    pub caption: String,
    pub attachments: Vec<Attachment>,
}
/// Issued by the store, never synthesized from a response or current UI state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub(super) account_id: String,
    pub(super) journal_id: String,
    pub(super) sequence: u64,
    pub(super) client_msg_id: String,
}
impl Ticket {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub ticket: Ticket,
    pub draft: Draft,
    pub file_ids: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub created: bool,
    pub stored: Stored,
}
fn id(value: &str, prefixes: &[u8]) -> bool {
    value.len() >= 2
        && value.len() <= 128
        && prefixes.contains(&value.as_bytes()[0])
        && value.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(seconds, fraction)| {
        !seconds.is_empty()
            && seconds.bytes().all(|b| b.is_ascii_digit())
            && seconds.parse::<u64>().is_ok_and(|s| s > 0)
            && !fraction.is_empty()
            && fraction.len() <= 6
            && fraction.bytes().all(|b| b.is_ascii_digit())
    })
}
impl Draft {
    pub fn validate(&self) -> Result<(), AppError> {
        if uuid::Uuid::parse_str(&self.account_id).is_err()
            || !id(&self.team_id, b"TE")
            || !id(&self.user_id, b"UW")
            || !id(&self.channel_id, b"CDG")
            || self.thread_ts.as_deref().is_some_and(|ts| !timestamp(ts))
            || (self.reply_broadcast && self.thread_ts.is_none())
            || !timestamp(&self.optimistic_ts)
            || self.thread_ts.as_ref() == Some(&self.optimistic_ts)
            || self.client_msg_id.is_empty()
            || self.client_msg_id.len() > 256
            || !self
                .client_msg_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || self.caption.len() > 65536
            || self.caption.contains('\0')
            || self.attachments.is_empty()
            || self.attachments.len() > 100
            || self.attachments.iter().any(|a| {
                a.name.is_empty()
                    || a.name.len() > 1024
                    || a.name.chars().any(|c| matches!(c, '\0' | '/'))
            })
        {
            return Err(sync::invalid("invalid upload recovery draft"));
        }
        Ok(())
    }
}
pub(super) fn validate_files(ids: &[String], count: usize) -> Result<(), AppError> {
    if ids.len() != count
        || ids.iter().any(|s| !id(s, b"F"))
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
    {
        return Err(sync::invalid("invalid finalized upload identity"));
    }
    Ok(())
}
