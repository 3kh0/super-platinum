//! Ordered mention and whole-file labels captured with the UI.
//! Adapted from profile-rich-mounted-v1-prep; no title/catalog acquisition lives here.
use super::super::{Error, Value, invalid};
use super::delta::Kind;

/// Private profile data, intentionally neither Debug nor Deserialize. The caller
/// owns directory/session freshness; this binds each captured raw occurrence.
#[derive(Clone)]
pub struct RichTextContext {
    team: String,
    user: String,
    sequence: Vec<(String, String, Kind)>,
    bytes: usize,
}
impl RichTextContext {
    pub fn new_ordered(team: &str, user: &str) -> Result<Self, Error> {
        if team.is_empty() || user.is_empty() {
            return Err(invalid("The rich-text account is unavailable."));
        }
        Ok(Self {
            team: team.into(),
            user: user.into(),
            sequence: Vec::new(),
            bytes: 0,
        })
    }
    pub(crate) fn owns(&self, team: &str, user: &str) -> bool {
        self.team == team && self.user == user
    }
    /// Capture the current token, including style and unknown metadata. Display
    /// labels are Delta attributes; ordinary mentions still measure their IDs.
    pub fn mention(&mut self, token: &Value, label: String) -> Result<(), Error> {
        let key = match token["type"].as_str() {
            Some("user") => "user_id",
            Some("usergroup") => "usergroup_id",
            Some("channel") => {
                match token.get("tab_id") {
                    None | Some(Value::Null) => {}
                    Some(Value::String(tab)) if tab.is_empty() => {}
                    _ => return Err(invalid("Channel tab metadata is unavailable.")),
                }
                "channel_id"
            }
            _ => return Err(invalid("This is not a resolved ordinary mention.")),
        };
        // E1L/ inserts an empty string for a known group with an empty handle.
        // Delta omits that operation, while the raw occurrence stays captured.
        if label.is_empty() && key != "usergroup_id" {
            return Err(invalid("This mention label is unavailable."));
        }
        let id = token[key]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid("This mention identity is unavailable."))?;
        self.push(
            token,
            label.clone(),
            Kind::Mention {
                id: id.into(),
                label,
            },
        )
    }
    /// Capture a canonical label for an ordinary whole canvas or file. The
    /// caller owns the original file/title source; neither `text` nor `url` is
    /// label authority. E1L/ inserts this label as a slug, and 3bvJ measures its
    /// UTF-16 text, including every duplicate occurrence. No title is generated.
    pub fn file(&mut self, token: &Value, label: String) -> Result<(), Error> {
        let kind = token["type"]
            .as_str()
            .filter(|kind| matches!(*kind, "canvas" | "file"))
            .ok_or_else(|| invalid("This is not a resolved whole-file token."))?;
        let id = token["file_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid("This file identity is unavailable."))?;
        if token.get("style").is_some_and(|style| !style.is_object()) {
            return Err(invalid("This rich-text element is malformed."));
        }
        // pND/ consumes these canvas fields. bFOf ignores them on a file;
        // ignored future fields remain opaque, not new semantic permissions.
        if kind == "canvas"
            && (!absent_or_falsey(token.get("section_id"))
                || !absent_or_falsey(token.get("is_skill_invocation")))
        {
            return Err(invalid("This canvas variant is not supported."));
        }
        let mut slug = serde_json::json!({"type":kind,"id":id,"label":label});
        if let Some(url) = token.get("url") {
            slug["url"] = url.clone();
        }
        if kind == "canvas"
            && let Some(section) = token.get("section_id")
        {
            slug["secondaryObjectId"] = section.clone();
        }
        self.push(
            token,
            label,
            Kind::Slug {
                identity: slug.to_string(),
            },
        )
    }
    /// Explicit section-label custody. Unlike `file`, this admits a nonempty
    /// Canvas section identity. Authoritative label units remain in the slug
    /// identity; a lossy UTF-8 copy is used ONLY by the existing length reducer.
    /// Each unpaired surrogate consumes one UTF-16 unit before and after that
    /// internal conversion. The renderer/catalog must compare exact units.
    pub fn canvas_section(&mut self, token: &Value, label_units: &[u16]) -> Result<(), Error> {
        if token["type"] != "canvas"
            || token["file_id"].as_str().is_none_or(str::is_empty)
            || token["section_id"].as_str().is_none_or(str::is_empty)
            || !absent_or_falsey(token.get("is_skill_invocation"))
            || token.get("style").is_some_and(|style| !style.is_object())
            || label_units.len() > super::MAX_BYTES / 2
        {
            return Err(invalid("This canvas section is unavailable."));
        }
        let label = String::from_utf16_lossy(label_units);
        let mut slug = serde_json::json!({"type":"canvas","id":token["file_id"],
            "secondaryObjectId":token["section_id"],"labelUtf16":label_units});
        if let Some(url) = token.get("url") {
            slug["url"] = url.clone();
        }
        self.push(
            token,
            label,
            Kind::Slug {
                identity: slug.to_string(),
            },
        )
    }
    fn push(&mut self, token: &Value, label: String, kind: Kind) -> Result<(), Error> {
        let mut budget = (super::MAX_NODES, super::MAX_BYTES);
        super::bounded(token, 0, &mut budget)?;
        let key = token.to_string();
        let bytes = self
            .bytes
            .checked_add(key.len())
            .and_then(|n| n.checked_add(label.len()))
            .ok_or_else(|| invalid("The rich-text resolution is too large."))?;
        if bytes > super::MAX_BYTES || self.sequence.len() >= super::MAX_NODES {
            return Err(invalid("The rich-text resolution is too large."));
        }
        self.sequence.push((key, label, kind));
        self.bytes = bytes;
        Ok(())
    }
    pub(super) fn resolve_next(
        &self,
        token: &Value,
        cursor: &mut usize,
    ) -> Result<Option<(String, Kind)>, Error> {
        if !matches!(
            token["type"].as_str(),
            Some("user" | "channel" | "usergroup" | "canvas" | "file")
        ) {
            return Ok(None);
        }
        let Some((expected, text, kind)) = self.sequence.get(*cursor) else {
            return Err(invalid(
                "The rich-text resolution no longer matches this capture.",
            ));
        };
        if expected != &token.to_string() {
            return Err(invalid("The rich-text resolution order changed."));
        }
        *cursor += 1;
        Ok(Some((text.clone(), kind.clone())))
    }
    pub(super) fn finish_sequence(&self, cursor: usize) -> Result<(), Error> {
        if self.sequence.len() != cursor {
            return Err(invalid("The rich-text capture omitted resolved content."));
        }
        Ok(())
    }
}

fn absent_or_falsey(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => true,
        Some(Value::String(text)) => text.is_empty(),
        Some(Value::Number(number)) => number.as_f64() == Some(0.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
