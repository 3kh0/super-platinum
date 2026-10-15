//! Inert candidate. No production admission constructor or runtime hooks.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod api;
mod arguments;
pub mod catalog;
pub mod codec;
pub mod controller;
pub mod journal;
mod paired;
mod rich_pair;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    #[error("invalid slash protocol input")]
    Invalid,
    #[error("unsupported slash command scope or content")]
    Unsupported,
    #[error("slash ownership or catalog changed")]
    Stale,
    #[error("slash receipt persistence failed")]
    Persistence,
    #[error("slash operation cannot be dispatched again")]
    NoReplay,
    #[error("protected slash admission is unavailable")]
    AdmissionUnavailable,
    #[error("slash command cooldown has not elapsed")]
    RateLimited,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub account: String,
    pub team: String,
    pub user: String,
    pub channel: String,
    pub thread: Option<String>,
}
impl Owner {
    pub fn validate(&self) -> Result<(), Failure> {
        if !uuid::Uuid::parse_str(&self.account).is_ok_and(|id| !id.is_nil())
            || !id(&self.team, 'T')
            || !id(&self.user, 'U')
            || !id(&self.channel, 'C')
            || self.thread.as_deref().is_some_and(|ts| !timestamp(ts))
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
fn id(value: &str, prefix: char) -> bool {
    value.starts_with(prefix)
        && value.len() > 1
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(a, b)| {
        !a.is_empty()
            && !b.is_empty()
            && a.bytes().all(|v| v.is_ascii_digit())
            && b.bytes().all(|v| v.is_ascii_digit())
    })
}
fn bounded(value: &Value, limit: usize) -> bool {
    value.to_string().len() <= limit
}

#[cfg(test)]
mod tests;
