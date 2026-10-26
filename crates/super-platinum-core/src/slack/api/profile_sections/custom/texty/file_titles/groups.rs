//! Known source-backed rLUS/XXup usergroup spelling; no acquisition policy.
use super::Unavailable;
use crate::slack::models::UserGroup;
use serde_json::Value;

/// Explicit application presentation; not an observed Slack locale or source
/// authority. The restricted-self branch must separately retain its own proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupTitle(());
impl GroupTitle {
    pub fn english() -> Self {
        Self(())
    }
    pub fn unknown_label(self) -> &'static str {
        "@unknown-group"
    }
}

/// Canonical effective handle/flags are authoritative for this sealed lookup.
/// Raw terminal/partial input can omit a handle while JJAw retains its previous
/// value. Never mutate raw label_source or use it as the merged label preimage.
/// Recheck lookup.current before mounting; mounted Catalog keeps its own label.
pub fn group_label_from_lookup(
    id: &str,
    lookup: &crate::state::directory_usergroups::unknown::Lookup,
    context: Option<GroupTitle>,
) -> Result<String, Unavailable> {
    use crate::state::directory_usergroups::unknown::LookupKind;
    if !valid_id(id) || lookup.id() != id || lookup.effective()["id"].as_str() != Some(id) {
        return Err(Unavailable::InvalidMetadata);
    }
    match lookup.kind() {
        LookupKind::Unknown | LookupKind::NonExistent => context
            .map(|context| context.unknown_label().into())
            .ok_or(Unavailable::UnsupportedTitle),
        LookupKind::Known => UserGroup::from_observed(lookup.effective())
            .ok()
            .and_then(|group| group.profile_label())
            .ok_or(Unavailable::InvalidMetadata),
    }
}

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

#[cfg(test)]
mod missing_tests;
