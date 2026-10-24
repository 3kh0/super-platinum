//! Closed known ordinary channel spelling with explicit placeholder presentation.
use super::Unavailable;
use serde_json::Value;

/// Caller-owned application presentation, not an observed Slack account locale.
/// This closed context represents only the retained English `channel_token` /
/// `private-channel` spelling. It grants no source or membership authority and
/// deliberately accepts no arbitrary translation strings or locale descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateChannelText {
    EnglishPlaceholder,
}

/// Application-owned English presentation, independently retained alongside a
/// sealed shared-selector observation. This is not an observed Slack locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelText {
    EnglishPlaceholders,
}

/// A caller must recheck Lookup::current against the original source before
/// mounting. Once mounted the editor owns its captured label. Arbitrary raw
/// marker flags cannot construct this proof or use the existing raw helpers.
pub fn observed_channel_label(
    id: &str,
    lookup: &crate::state::directory_channels::unknown::Lookup,
    context: Option<ChannelText>,
) -> Result<String, Unavailable> {
    use crate::state::directory_channels::unknown::LookupKind;
    if !valid_id(id) || lookup.id() != id {
        return Err(Unavailable::InvalidMetadata);
    }
    let english = context == Some(ChannelText::EnglishPlaceholders);
    match lookup.kind() {
        LookupKind::Known => known_ordinary_channel_label_with_context(
            id,
            lookup.row().ok_or(Unavailable::InvalidMetadata)?,
            english.then_some(PrivateChannelText::EnglishPlaceholder),
        ),
        LookupKind::Unknown if english => Ok("#unknown-channel".into()),
        LookupKind::NonExistent if english => Ok("#private-channel".into()),
        _ => Err(Unavailable::UnsupportedTitle),
    }
}

fn valid_id(id: &str) -> bool {
    id.len() >= 2
        && id.len() <= 128
        && id.starts_with('C')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub(super) fn id(leaf: &Value) -> Result<&str, Unavailable> {
    // A tab changes the selector/formatting route. Nullish/empty is the ordinary
    // branch; unknown opaque fields and styling are preserved in source.
    if !matches!(leaf.get("tab_id"), None | Some(Value::Null))
        && leaf["tab_id"].as_str() != Some("")
    {
        return Err(Unavailable::UnsupportedTitle);
    }
    leaf.get("channel_id")
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .ok_or(Unavailable::UnsupportedTitle)
}

/// Caller must bind this exact row to its current canonical account/team source.
/// This helper proves only the bounded public ordinary shape and matching ID.
/// Archived ordinary rows retain their name; absent, private and special rows
/// do not gain guessed labels or acquisition/locale authority.
pub fn public_channel_label(id: &str, row: &Value) -> Result<String, Unavailable> {
    ordinary_label(id, row, false, None)
}

/// Known public or modern private-member ordinary channels only. The private
/// branch requires explicit boolean `is_private=true` and `is_member=true` on
/// the same canonical row; no membership is inferred from its supplied name,
/// native display, or another source. Missing/false/malformed membership never
/// discloses a private name. Legacy groups and localized fallbacks stay refused.
/// The caller owns exact canonical account/team/source custody until mounting.
pub fn known_ordinary_channel_label(id: &str, row: &Value) -> Result<String, Unavailable> {
    ordinary_label(id, row, true, None)
}

/// Additionally admits the placeholder for an explicitly known ordinary modern
/// private channel with boolean `is_member=false`. Missing/malformed membership
/// still refuses. The caller must retain this independent display context and
/// the exact canonical account/team/source observation until mounting; neither
/// a supplied private name nor a Quip override grants placeholder authority.
pub fn known_ordinary_channel_label_with_context(
    id: &str,
    row: &Value,
    private_text: Option<PrivateChannelText>,
) -> Result<String, Unavailable> {
    ordinary_label(id, row, true, private_text)
}

fn ordinary_label(
    id: &str,
    row: &Value,
    private_member: bool,
    private_text: Option<PrivateChannelText>,
) -> Result<String, Unavailable> {
    if !valid_id(id) || row["id"].as_str() != Some(id) || row["is_channel"] != true {
        return Err(Unavailable::InvalidMetadata);
    }
    let placeholder = match row.get("is_private") {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Bool(true)) if private_member && row["is_member"] == Value::Bool(true) => false,
        Some(Value::Bool(true))
            if private_member
                && row["is_member"] == Value::Bool(false)
                && private_text == Some(PrivateChannelText::EnglishPlaceholder) =>
        {
            true
        }
        _ => return Err(Unavailable::UnsupportedTitle),
    };
    for key in [
        "is_group",
        "is_im",
        "is_mpim",
        "is_file",
        "isUnknown",
        "isNonExistent",
        "is_deleted",
    ] {
        if row
            .get(key)
            .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
        {
            return Err(Unavailable::UnsupportedTitle);
        }
    }
    let properties = row.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object()) {
        return Err(Unavailable::UnsupportedTitle);
    }
    for (group, field) in [
        ("huddles", "is_huddle_channel"),
        ("meetings_channel", "is_meeting_channel"),
        ("record_channel", "record_id"),
    ] {
        let group = properties
            .and_then(|p| p.get(group))
            .filter(|v| !v.is_null());
        if group.is_some_and(|v| !v.is_object()) {
            return Err(Unavailable::UnsupportedTitle);
        }
        if group.and_then(|v| v.get(field)).is_some_and(|v| {
            !matches!(v, Value::Null | Value::Bool(false)) && v.as_str() != Some("")
        }) {
            return Err(Unavailable::UnsupportedTitle);
        }
    }
    // Keep the named special-channel exclusion, but a safe placeholder does
    // not require a usable private name. bQFi's private/nonmember return branch
    // ignores the display selector result; do not invent name authority here.
    if row["name"].as_str().is_some_and(|s| s.starts_with("mc:")) {
        return Err(Unavailable::InvalidMetadata);
    }
    if placeholder {
        return Ok("#private-channel".to_owned());
    }
    let name = row["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(Unavailable::InvalidMetadata)?;
    Ok(format!("#{name}"))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod private_tests;

#[cfg(test)]
mod nonmember_tests;

#[cfg(test)]
mod lookup_tests;
