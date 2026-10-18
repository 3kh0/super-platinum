//! Ordered user, usergroup and ordinary-channel labels captured with the UI.
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
        self.sequence.push((
            key,
            label.clone(),
            Kind::Mention {
                id: id.into(),
                label,
            },
        ));
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
            Some("user" | "channel" | "usergroup")
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

#[cfg(test)]
mod tests;
