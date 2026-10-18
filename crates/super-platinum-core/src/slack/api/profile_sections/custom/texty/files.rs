//! Profile's gated file category: c4db.Yn, 4wTh, 20Mt and qKv8.
//! These helpers neither admit files for insertion nor establish file history.
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::Value;
mod cache;
pub use cache::{QueryCache, RequestToken};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_file_completion_unavailable".into())
}
fn positive(raw: &Value) -> bool {
    raw.is_object()
        && raw.get("ok") == Some(&Value::Bool(true))
        && raw.get("error").is_none_or(Value::is_null)
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    enabled: bool,
}
impl Policy {
    /// A complete successful experiments receipt is required even for off.
    pub fn capture(raw: &Value) -> Result<Self, Error> {
        if !positive(raw) {
            return Err(unavailable());
        }
        let assignments = raw["assignments"].as_object().ok_or_else(unavailable)?;
        let enabled = match assignments.get("texty_autocomplete_files") {
            None => false,
            Some(row) => row["group"].as_str().ok_or_else(unavailable)? == "on",
        };
        Ok(Self { enabled })
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// Input is the actual shared query, after the caller's one-@ removal.
    /// No trim, normalization, filetype prefix or Unicode-script inference.
    pub fn remote_eligible(&self, query: &str) -> bool {
        self.enabled && remote_threshold(query)
    }
}

/// CwFm.A over the exact ZOnG.kw ranges, or JS string.length > 3.
pub fn remote_threshold(query: &str) -> bool {
    query.encode_utf16().count() > 3
        || query.chars().any(|c| {
            matches!(c as u32,
                0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff |
                0xff00..=0xffef | 0xac00..=0xd7af | 0x1100..=0x11ff |
                0x3130..=0x318f | 0xa960..=0xa97f | 0xd7b0..=0xd7ff |
                0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x2e80..=0x2eff |
                0x2f00..=0x2fdf | 0x2ff0..=0x2fff | 0x3001..=0x303f |
                0x31c0..=0x31ef | 0x3200..=0x32ff | 0x3300..=0x33ff |
                0xf900..=0xfaff | 0xfe30..=0xfe4f)
        })
}

/// No count, cursor, type filter, channel or org-wide argument at this caller.
/// Dispatch permission and original account/query custody belong to the caller.
pub fn request(client: &SlackClient, owner: &WorkspaceSession, query: &str) -> PreparedRequest {
    client.rest_form(
        owner,
        "search.autocomplete.files",
        vec![
            ("query", query.into()),
            ("include_shares", "true".into()),
            ("_x_reason", "texty-autocomplete".into()),
        ],
    )
}

/// Array encounter order, duplicates and opaque fields are preserved. This is
/// not a type/status filter or a successful source-adoption receipt. Malformed
/// rows are a native refusal, never a fabricated successful empty result.
pub fn response(raw: &Value) -> Result<Vec<Value>, Error> {
    if !positive(raw) {
        return Err(unavailable());
    }
    let rows = raw["files"].as_array().ok_or_else(unavailable)?;
    if rows
        .iter()
        .any(|row| !row.is_object() || row["id"].as_str().is_none_or(str::is_empty))
    {
        return Err(unavailable());
    }
    Ok(rows.clone())
}
