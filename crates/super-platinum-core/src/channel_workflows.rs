//! Read-only Channel Workflows projections (3agQ/djZg/i7ua/GN/G).
//! These captured rows grant neither execution nor channel/posting authority.
//! Native bounds are explicit refusals, never truncated successful collections.
use crate::config::WorkspaceSession;
use crate::slack::{Error, PreparedRequest, SlackClient};
use serde_json::{Value, json};

pub mod details;
mod ordering;
mod previews;
pub use previews::{Preview, PreviewKind, Previews, Row, Rows};
#[cfg(test)]
mod tests;

pub const MAX_PREVIEW_IDS: usize = 250;
pub const MAX_COLLECTION_ROWS: usize = 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    method: &'static str,
    reason: &'static str,
    arguments: Value,
}
impl Read {
    pub fn bookmarks(channel: &str) -> Result<Self, Error> {
        if !identifier(channel) {
            return Err(invalid());
        }
        Ok(Self {
            method: "bookmarks.list",
            reason: "bookmarks-store/conditional-fetching",
            arguments: json!({"channel":channel,"include_folders":true,"include_legacy_workflows":true}),
        })
    }
    pub fn previews(ids: &[String]) -> Result<Self, Error> {
        validate_ids(ids)?;
        if ids.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            method: "workflows.triggers.preview",
            reason: "hermes_trigger_previews_ufm",
            arguments: json!({"trigger_ids":ids.join(",")}),
        })
    }
    pub fn method(&self) -> &'static str {
        self.method
    }
    pub fn reason(&self) -> &'static str {
        self.reason
    }
    pub fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub fn request(&self, client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
        let mut fields: Vec<(&str, String)> = self
            .arguments
            .as_object()
            .expect("closed read")
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str(),
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string()),
                )
            })
            .collect();
        fields.push(("_x_reason", self.reason.into()));
        client.rest_form(session, self.method, fields)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    trigger_id: String,
    title: Option<String>,
    raw: Value,
}
impl Reference {
    pub fn trigger_id(&self) -> &str {
        &self.trigger_id
    }
    /// Only a featured denied row may display this source title.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collections {
    channel: Value,
    bookmarks: Value,
    featured: Vec<Reference>,
    bookmarked: Vec<Reference>,
    ids: Vec<String>,
}
impl Collections {
    pub fn capture(channel: &Value, bookmarks: &Value) -> Result<Self, Error> {
        let channel_id = channel
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| identifier(s))
            .ok_or_else(invalid)?;
        if channel
            .get("properties")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        {
            return Err(invalid());
        }
        let featured_raw = match channel.pointer("/properties/channel_workflows") {
            None | Some(Value::Null) => &[][..],
            Some(Value::Array(rows)) => rows.as_slice(),
            _ => return Err(invalid()),
        };
        success(bookmarks)?;
        let bookmark_raw = bookmarks
            .get("bookmarks")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if featured_raw.len() > MAX_COLLECTION_ROWS
            || bookmark_raw.len() > MAX_COLLECTION_ROWS
            || bookmarks
                .pointer("/response_metadata/next_cursor")
                .is_some_and(|v| !v.is_null() && v != "")
            || bookmarks.get("has_more").is_some_and(|v| v != false)
        {
            return Err(invalid());
        }
        let mut featured = Vec::new();
        for raw in featured_raw {
            let trigger_id = string_id(raw, "workflow_trigger_id")?;
            let title = optional_string(raw, "title")?;
            featured.push(Reference {
                trigger_id,
                title,
                raw: raw.clone(),
            });
        }
        let mut ordered = Vec::new();
        for row in bookmark_raw {
            if !row.is_object() || row.get("channel_id").is_some_and(|v| v != channel_id) {
                return Err(invalid());
            }
            ordering::validate(row.get("rank"))?;
            ordered.push(row);
        }
        // Stable insertion order also preserves ties for mixed JS primitive ranks.
        ordering::sort(&mut ordered);
        let mut bookmarked = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for raw in ordered {
            if !matches!(raw["type"].as_str(), Some("shortcut" | "auto_gen_shortcut")) {
                continue;
            }
            let Some(value) = raw.get("shortcut_id") else {
                continue;
            };
            let trigger_id = value
                .as_str()
                .filter(|s| identifier(s))
                .ok_or_else(invalid)?
                .to_owned();
            if seen.insert(trigger_id.clone()) {
                bookmarked.push(Reference {
                    trigger_id,
                    title: None,
                    raw: raw.clone(),
                });
            }
        }
        let mut ids = Vec::new();
        seen.clear();
        for row in featured.iter().chain(&bookmarked) {
            if seen.insert(row.trigger_id.clone()) {
                ids.push(row.trigger_id.clone());
            }
        }
        validate_ids(&ids)?;
        Ok(Self {
            channel: channel.clone(),
            bookmarks: bookmarks.clone(),
            featured,
            bookmarked,
            ids,
        })
    }
    pub fn channel_raw(&self) -> &Value {
        &self.channel
    }
    pub fn bookmarks_raw(&self) -> &Value {
        &self.bookmarks
    }
    pub fn featured(&self) -> &[Reference] {
        &self.featured
    }
    pub fn bookmarked(&self) -> &[Reference] {
        &self.bookmarked
    }
    pub fn preview_ids(&self) -> &[String] {
        &self.ids
    }
}

fn invalid() -> Error {
    Error::Api("channel_workflows_invalid".into())
}
fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn string_id(raw: &Value, key: &str) -> Result<String, Error> {
    raw.get(key)
        .and_then(Value::as_str)
        .filter(|s| identifier(s))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn optional_string(raw: &Value, key: &str) -> Result<Option<String>, Error> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(invalid()),
    }
}
fn success(raw: &Value) -> Result<(), Error> {
    if raw.is_object() && raw["ok"] == true && raw.get("error").is_none_or(Value::is_null) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn validate_ids(ids: &[String]) -> Result<(), Error> {
    let mut seen = std::collections::HashSet::new();
    if ids.len() > MAX_PREVIEW_IDS || ids.iter().any(|s| !identifier(s) || !seen.insert(s)) {
        Err(invalid())
    } else {
        Ok(())
    }
}
