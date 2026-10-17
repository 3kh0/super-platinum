//! Ordinary Vote configuration and own-user intent. The wire protocol replaces
//! the complete user array; a preflight is not an atomic server vote operation.
use super::{Column, ListError, RatingOptions, Record};
use serde_json::Value;
use std::collections::HashSet;

/// Same owned emoji/team representation as Rating, without a quantity option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoteOptions(RatingOptions);
impl Default for VoteOptions {
    fn default() -> Self {
        Self(RatingOptions::capture(":sparkling_heart:", 1, None).expect("static Vote emoji"))
    }
}
impl VoteOptions {
    pub fn capture(emoji: &str, team: Option<&str>) -> Result<Self, ListError> {
        RatingOptions::capture(emoji, 1, team)
            .map(Self)
            .map_err(|_| ListError("Choose a current emoji for this Vote field."))
    }
    pub fn emoji(&self) -> &str {
        self.0.emoji()
    }
    pub fn team_id(&self) -> Option<&str> {
        self.0.team_id()
    }
    pub fn wire(&self) -> Value {
        let mut value = self.0.wire();
        value
            .as_object_mut()
            .expect("typed emoji options")
            .remove("max");
        value
    }
    /// Exact authored shape only. An output emoji_url remains unverified, not
    /// silently treated as equivalent to the requested configuration.
    pub fn from_wire(value: &Value) -> Result<Self, ListError> {
        let err = || ListError("This Vote configuration is unsupported.");
        let map = value.as_object().ok_or_else(err)?;
        if map.len() != 1 + usize::from(map.contains_key("emoji_team_id"))
            || map
                .keys()
                .any(|k| !matches!(k.as_str(), "emoji" | "emoji_team_id"))
        {
            return Err(err());
        }
        let emoji = map.get("emoji").and_then(Value::as_str).ok_or_else(err)?;
        let team = match map.get("emoji_team_id") {
            None => None,
            Some(Value::String(s)) => Some(s.as_str()),
            _ => return Err(err()),
        };
        Self::capture(emoji, team)
    }
    pub fn from_column(column: &Column) -> Result<Self, ListError> {
        if column.kind != "vote" {
            return Err(ListError("This field is not an ordinary Vote."));
        }
        Self::from_wire(
            column
                .raw
                .get("options")
                .ok_or(ListError("This Vote configuration is unavailable."))?,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteIntent {
    AddMyVote,
    RemoveMyVote,
}

/// Explicit local bound on loaded/editable voters, not a Slack service limit.
pub const MAX_VOTERS: usize = 4096;
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '|')
}

pub(super) fn read(column: &Column, record: &Record) -> Result<Vec<String>, ListError> {
    VoteOptions::from_column(column)?;
    let err = || ListError("This Vote value is malformed or ambiguous.");
    if record.fields.iter().any(|(key, field)| {
        key != &column.key && field.get("column_id").and_then(Value::as_str) == Some(&column.id)
    }) {
        return Err(err());
    }
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(Vec::new());
    };
    let map = field.as_object().ok_or_else(err)?;
    if map
        .get("key")
        .is_some_and(|v| v.as_str() != Some(&column.key))
        || map
            .get("column_id")
            .is_some_and(|v| v.as_str() != Some(&column.id))
        || map
            .get("row_id")
            .is_some_and(|v| v.as_str() != Some(&record.id))
        || map.iter().any(|(key, value)| {
            !matches!(
                key.as_str(),
                "key" | "column_id" | "row_id" | "text" | "user"
            ) && !value.is_null()
        })
    {
        return Err(err());
    }
    // A genuinely absent field and an explicit empty array are typed empty.
    // Present null/missing user cannot impersonate either raw representation.
    let values = map.get("user").and_then(Value::as_array).ok_or_else(err)?;
    if values.len() > MAX_VOTERS {
        return Err(err());
    }
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| {
            let id = value
                .as_str()
                .filter(|id| identity(id) && seen.insert(*id))
                .ok_or_else(err)?;
            Ok(id.to_owned())
        })
        .collect()
}

pub(super) fn desired(
    mut voters: Vec<String>,
    user: &str,
    intent: VoteIntent,
) -> Result<Vec<String>, ListError> {
    if !identity(user) {
        return Err(ListError("The authenticated voter is unavailable."));
    }
    let present = voters.iter().any(|id| id == user);
    match (intent, present) {
        (VoteIntent::AddMyVote, false) if voters.len() < MAX_VOTERS => voters.push(user.into()),
        (VoteIntent::RemoveMyVote, true) => voters.retain(|id| id != user),
        (VoteIntent::AddMyVote, false) => {
            return Err(ListError(
                "This Vote exceeds the local editable voter bound.",
            ));
        }
        _ => return Err(ListError("Your vote is already set to that value.")),
    }
    Ok(voters)
}

#[cfg(test)]
mod tests;
