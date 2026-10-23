//! Exact initialization and ordinary native policy selectors from complete acquired carriers.
use serde_json::Value;
use super_platinum_core::composer_emoji::standard::Version;
use super_platinum_core::slack::{Error, api};
mod policy;
pub(in crate::slack_lists::comments::own_edit) use policy::{NativePolicy, native_policy};

fn unavailable() -> Error {
    Error::Api("list_plain_preferences_unavailable".into())
}

/// 4OuQ._Z -> 7zY3.I -> tCC+.O, using the complete actual experiments response.
/// Absence in that acquired map is source null/V16; an absent carrier is not.
pub(in crate::slack_lists::comments::own_edit) fn emoji_version(
    response: &Value,
) -> Result<Version, Error> {
    if response["ok"] != true || response.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    let assignments = response["assignments"]
        .as_object()
        .ok_or_else(unavailable)?;
    let group = match assignments.get("emoji_v17") {
        None | Some(Value::Null) => None,
        Some(Value::Object(record)) => match record.get("group") {
            None | Some(Value::Null) => None,
            Some(Value::String(group)) => Some(group.as_str()),
            _ => return Err(unavailable()),
        },
        _ => return Err(unavailable()),
    };
    Ok(if group == Some("on") {
        Version::V17
    } else {
        Version::V16
    })
}

pub(super) fn emoji_as_text(response: &Value) -> Result<bool, Error> {
    let prefs = policy::carrier(response, "prefs")?;
    // pfK8.P/4iNl.ty: strict equality. Only absence in the positively acquired
    // complete map is known false; unavailable/partial carriers are not that.
    Ok(prefs.get("emoji_mode").and_then(Value::as_str) == Some("as_text"))
}

pub(in crate::slack_lists::comments::own_edit) fn highlights(
    response: &Value,
    self_id: &str,
    memberships: &[String],
) -> Result<Vec<String>, Error> {
    if self_id.is_empty() || memberships.iter().any(String::is_empty) {
        return Err(unavailable());
    }
    let prefs = api::parse_notification_preferences(response)?;
    let keywords = match prefs.raw()["global"].get("global_keywords") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.as_str()),
        _ => return Err(unavailable()),
    };
    let mut words = vec![format!(
        "<@{}",
        self_id.strip_prefix('@').unwrap_or(self_id)
    )];
    if let Some(keywords) = keywords {
        words.extend(keywords.split(',').map(str::to_owned));
    }
    words.extend(memberships.iter().map(|id| format!("<!subteam^{id}")));
    Ok(words)
}

/// Explicit application-English presentation adaptation, not acquired account
/// locale. Other dynamic ranges require their own original source observations.
pub(super) fn english_broadcast(range: &str) -> Result<String, Error> {
    match range {
        "here" | "channel" | "everyone" => Ok(format!("@{range}")),
        _ => Err(unavailable()),
    }
}
