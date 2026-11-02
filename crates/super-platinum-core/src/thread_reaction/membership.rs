use super::*;
use crate::slack::models::Reaction;
use std::collections::{BTreeMap, BTreeSet};

/// Explicit complete original reaction rows; no typed/default reconstruction.
pub struct RootMembership {
    pub(super) original: OriginalRoot,
    pub(super) message: Value,
    pub(super) reactions: Vec<Reaction>,
}
impl RootMembership {
    pub fn decode(original: &OriginalRoot, raw: &Value) -> Result<Self> {
        envelope(raw)?;
        if raw.get("type").and_then(Value::as_str) != Some("message")
            || raw.get("channel").and_then(Value::as_str) != Some(original.channel())
        {
            return Err(UNAVAILABLE);
        }
        let message = raw.get("message").ok_or(UNAVAILABLE)?;
        if !original.matches(message) {
            return Err(UNAVAILABLE);
        }
        let rows = message
            .get("reactions")
            .and_then(Value::as_array)
            .ok_or(UNAVAILABLE)?;
        // Defensive native allocation bounds, not Source quotas/schema. Add's
        // actual raw-row quotas are applied later; Remove does not inherit them.
        if rows.len() > 1024 {
            return Err(UNAVAILABLE);
        }
        let mut names = BTreeSet::new();
        let mut reactions = Vec::with_capacity(rows.len());
        for row in rows {
            let fields = row.as_object().ok_or(UNAVAILABLE)?;
            let name = row
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| reaction_name(s))
                .ok_or(UNAVAILABLE)?;
            let count = row
                .get("count")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or(UNAVAILABLE)?;
            let users = row
                .get("users")
                .and_then(Value::as_array)
                .ok_or(UNAVAILABLE)?;
            if !names.insert(name) || users.len() > count as usize || users.len() > 4096 {
                return Err(UNAVAILABLE);
            }
            let mut unique = BTreeSet::new();
            let mut ids = Vec::with_capacity(users.len());
            for user in users {
                let id = user.as_str().filter(|id| user_id(id)).ok_or(UNAVAILABLE)?;
                if !unique.insert(id) {
                    return Err(UNAVAILABLE);
                }
                ids.push(id.into());
            }
            reactions.push(Reaction {
                name: name.into(),
                count,
                users: ids,
                extra: fields
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "name" | "count" | "users"))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<BTreeMap<_, _>>(),
            });
        }
        Ok(Self {
            original: original.clone(),
            message: message.clone(),
            reactions,
        })
    }
    pub fn original(&self) -> &OriginalRoot {
        &self.original
    }
    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }
}

pub(super) fn reaction_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'+' | b':')
        })
}
