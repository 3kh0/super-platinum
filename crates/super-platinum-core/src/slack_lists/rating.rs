//! Ordinary Rating configuration and stored values. Display fallback is not
//! mutation authority; unknown options and legacy stored zero remain read-only.
use super::{Column, ListError, Record};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatingOptions {
    emoji: String,
    max: u8,
    emoji_team_id: Option<String>,
}
impl Default for RatingOptions {
    fn default() -> Self {
        Self {
            emoji: ":star:".into(),
            max: 5,
            emoji_team_id: None,
        }
    }
}
impl RatingOptions {
    /// Local bounded representation, not a claimed Slack shortcode length limit.
    /// The desktop additionally admits choices only from its current owned picker.
    pub fn capture(emoji: &str, max: u8, team: Option<&str>) -> Result<Self, ListError> {
        if !(1..=5).contains(&max)
            || !(3..=256).contains(&emoji.len())
            || !emoji.starts_with(':')
            || !emoji.ends_with(':')
            || emoji.trim_matches(':').is_empty()
            || emoji.chars().any(|c| c.is_control() || c.is_whitespace())
            || team.is_some_and(|t| {
                t.is_empty() || t.len() > 1024 || t.trim() != t || t.chars().any(char::is_control)
            })
        {
            return Err(ListError(
                "Choose a current emoji and a rating maximum from 1 to 5.",
            ));
        }
        Ok(Self {
            emoji: emoji.into(),
            max,
            emoji_team_id: team.map(str::to_owned),
        })
    }
    pub fn emoji(&self) -> &str {
        &self.emoji
    }
    pub fn max(&self) -> u8 {
        self.max
    }
    pub fn team_id(&self) -> Option<&str> {
        self.emoji_team_id.as_deref()
    }
    pub fn wire(&self) -> Value {
        let mut value = json!({"emoji":self.emoji,"max":self.max});
        if let Some(team) = &self.emoji_team_id {
            value["emoji_team_id"] = json!(team);
        }
        value
    }
    /// Exact two/three-member authored shape; output emoji_url is not inferred
    /// equivalent to its omission or accepted as a writable input argument.
    pub fn from_wire(value: &Value) -> Result<Self, ListError> {
        let err = || ListError("This Rating configuration is unsupported.");
        let map = value.as_object().ok_or_else(err)?;
        if map.len() != 2 + usize::from(map.contains_key("emoji_team_id"))
            || map
                .keys()
                .any(|key| !matches!(key.as_str(), "emoji" | "max" | "emoji_team_id"))
        {
            return Err(err());
        }
        let max = map
            .get("max")
            .and_then(Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(err)?;
        let emoji = map.get("emoji").and_then(Value::as_str).ok_or_else(err)?;
        let team = match map.get("emoji_team_id") {
            None => None,
            Some(Value::String(s)) => Some(s.as_str()),
            _ => return Err(err()),
        };
        Self::capture(emoji, max, team)
    }
    pub fn from_column(column: &Column) -> Result<Self, ListError> {
        if column.kind != "rating" {
            return Err(ListError("This field is not an ordinary Rating."));
        }
        Self::from_wire(
            column
                .raw
                .get("options")
                .ok_or(ListError("This Rating configuration is unavailable."))?,
        )
    }
}

pub(super) fn read(column: &Column, record: &Record) -> Result<Option<u8>, ListError> {
    let options = RatingOptions::from_column(column)?;
    let err = || ListError("This Rating value is malformed or outside its configured maximum.");
    // A different-key cell carrying this column identity is not a missing value.
    if record.fields.iter().any(|(key, field)| {
        key != &column.key && field.get("column_id").and_then(Value::as_str) == Some(&column.id)
    }) {
        return Err(err());
    }
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(None);
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
                "key" | "column_id" | "row_id" | "text" | "rating"
            ) && !value.is_null()
        })
    {
        return Err(err());
    }
    // Present null/missing member cannot impersonate the genuinely absent cell.
    let values = map
        .get("rating")
        .and_then(Value::as_array)
        .ok_or_else(err)?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => value
            .as_u64()
            .filter(|n| (1..=u64::from(options.max())).contains(n))
            .map(|n| Some(n as u8))
            .ok_or_else(err),
        _ => Err(err()),
    }
}

#[cfg(test)]
mod tests;
