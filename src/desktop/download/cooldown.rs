//! Stable-owner process cooldown survives component/account/ShellState replacement.
use super::*;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
type Key = (Option<String>, String, String);
#[derive(Default)]
struct Registry {
    entries: HashMap<Key, Option<Instant>>,
    overflow: Option<Option<Instant>>,
}
static DEADLINES: OnceLock<Mutex<Registry>> = OnceLock::new();
pub(super) fn available(key: &Key) -> bool {
    let Ok(mut entries) = DEADLINES.get_or_init(Default::default).lock() else {
        return false;
    };
    let now = Instant::now();
    entries
        .entries
        .retain(|_, until| until.is_none_or(|until| until > now));
    if entries
        .overflow
        .is_some_and(|until| until.is_some_and(|until| until <= now))
    {
        entries.overflow = None;
    }
    entries.overflow.is_none() && !entries.entries.contains_key(key) && entries.entries.len() < 64
}
pub(super) fn record(key: &Key, result: &Result<(), DownloadError>) {
    let secs = match result {
        Err(DownloadError::Network(super_platinum_core::slack::Error::RateLimited {
            retry_after_secs,
        })) => Some(retry_after_secs.unwrap_or(30)),
        Err(DownloadError::Network(super_platinum_core::slack::Error::HttpStatus {
            retry_after_secs: Some(secs),
            ..
        })) => Some(*secs),
        _ => None,
    };
    let Some(secs) = secs else {
        return;
    };
    let Ok(mut entries) = DEADLINES.get_or_init(Default::default).lock() else {
        return;
    };
    let now = Instant::now();
    entries
        .entries
        .retain(|_, until| until.is_none_or(|until| until > now));
    let until = now.checked_add(Duration::from_secs(secs.max(1)));
    if let Some(old) = entries.entries.get_mut(key) {
        *old = match (*old, until) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
    } else if entries.entries.len() < 64 {
        entries.entries.insert(key.clone(), until);
    } else {
        entries.overflow = Some(match (entries.overflow, until) {
            (None, b) => b,
            (Some(Some(a)), Some(b)) => Some(a.max(b)),
            _ => None,
        });
    }
    // Overflow retains the strongest known fence globally, without growing memory.
}
