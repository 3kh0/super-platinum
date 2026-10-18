//! Decisions shared by the actual chooser callbacks; no dispatch or authority.
use super::NetworkError;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use super_platinum_core::slash_commands::catalog::{Catalog, Choice};

pub(super) fn choice(catalog: &Catalog, input: &str, index: usize) -> Option<Choice> {
    let name = input.split_whitespace().next()?;
    let choice = catalog.choice(index)?;
    // Core's frozen request has this same case-insensitive display comparison.
    if !name.eq_ignore_ascii_case(&choice.command().name)
        || choice.admit(catalog, catalog.owner()).is_err()
    {
        return None;
    }
    Some(choice)
}
pub(super) fn choices(catalog: &Catalog, input: &str) -> Vec<(usize, String)> {
    (0..catalog.commands().len())
        .filter_map(|index| {
            let choice = choice(catalog, input, index)?;
            let command = choice.command();
            let app = command
                .raw
                .get("app_name")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(&command.app);
            Some((index, format!("{} \u{b7} {}", command.name, app)))
        })
        .collect()
}

// A canceled discovery can still return a real rate limit. Sharing this map
// across the two composer slots prevents closing one from erasing that result.
// None means Instant overflow, conservatively blocked until process restart.
pub(super) fn remember_cooldown(
    values: &mut BTreeMap<(String, String), Option<Instant>>,
    key: (String, String),
    error: &NetworkError,
    now: Instant,
) -> Option<Instant> {
    let seconds = match error {
        NetworkError::RateLimited {
            retry_after_secs: Some(seconds),
        }
        | NetworkError::HttpStatus {
            status: 429,
            retry_after_secs: Some(seconds),
        } => *seconds,
        _ => return None,
    };
    let candidate = now.checked_add(Duration::from_secs(seconds));
    let retained = match values.get(&key) {
        Some(None) => None,
        Some(Some(previous)) => candidate.map(|current| current.max(*previous)),
        None => candidate,
    };
    values.insert(key, retained);
    retained
}
#[cfg(test)]
mod tests;
