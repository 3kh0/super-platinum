//! Known source-backed rLUS/XXup usergroup spelling; no acquisition policy.
use super::Unavailable;
use crate::slack::models::UserGroup;
use serde_json::Value;

fn valid_id(id: &str) -> bool {
    id.len() >= 9
        && id.len() <= 255
        && id.starts_with('S')
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub(super) fn id(leaf: &Value) -> Result<&str, Unavailable> {
    // Styling and tab/opaque members remain data; only the raw id field selects.
    leaf.get("usergroup_id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or(Unavailable::UnsupportedTitle)
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}

/// Format a known ordinary row only. The caller must bind this exact typed/raw
/// row to current shared source history and account/team ownership before mount.
/// No id-only legacy row, unknown placeholder or localized fallback is admitted.
/// Existing deactivated/hidden/empty handles are not completion-filtered.
pub fn known_group_label(id: &str, group: &UserGroup) -> Result<String, Unavailable> {
    if !valid_id(id) || group.id != id {
        return Err(Unavailable::InvalidMetadata);
    }
    let raw = group
        .label_source
        .as_ref()
        .ok_or(Unavailable::InvalidMetadata)?;
    if raw.get("id").and_then(Value::as_str) != Some(id)
        || ["isUnknown", "isNonExistent"]
            .iter()
            .any(|key| raw.get(*key).is_some_and(truth))
    {
        return Err(Unavailable::InvalidMetadata);
    }
    group.profile_label().ok_or(Unavailable::InvalidMetadata)
}

#[cfg(test)]
mod tests;
