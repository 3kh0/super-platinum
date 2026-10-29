//! Closed positive branch of6OO5.eJ. Actual shared channel processing follows it.
use super::*;
use serde_json::{Value, json};
fn invalid() -> Error {
    Error::Api("calendar_open_mpim_shape_unavailable".into())
}
fn id(value: &Value) -> Option<&str> {
    value.as_str().filter(|id| {
        !id.is_empty()
            && id.len() <= 255
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
/// This conversion supplies no canonical readiness, visibility or posting permit.
/// Caller must own the current original canonical cached row and actual intake.
pub fn convert_calendar_known_open_mpim(
    cached: &Value,
    wire: &Value,
) -> Result<Option<Value>, Error> {
    let raw = wire.as_object().ok_or_else(invalid)?;
    let member = id(raw.get("id").ok_or_else(invalid)?).ok_or_else(invalid)?;
    if cached.get("id").and_then(Value::as_str) != Some(member)
        || cached.get("is_mpim") != Some(&Value::Bool(true))
        || cached.get("is_open") != Some(&Value::Bool(true))
        || ["isUnknown", "isNonExistent"]
            .iter()
            .any(|key| cached.get(*key).is_some_and(|v| v != &Value::Bool(false)))
    {
        return Ok(None);
    }
    let members = raw
        .get("members")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if members.len() > 9 {
        return Ok(None);
    }
    if members.iter().any(|v| {
        !v.as_str()
            .is_some_and(crate::block_actions::form::valid_message_user)
    }) {
        return Err(invalid());
    }
    let mut result = cached.as_object().ok_or_else(invalid)?.clone();
    if let Some(details) = raw.get("details") {
        let details = details.as_object().ok_or_else(invalid)?;
        if details
            .get("id")
            .is_some_and(|v| v != &Value::String(member.into()))
        {
            return Err(invalid());
        }
        result.extend(details.clone());
    }
    result.insert("members".into(), Value::Array(members.clone()));
    // Full actual source override set; notably outer name is never overlaid.
    let forced = json!({"internal_team_ids":[],"is_archived":false,"is_channel":true,
        "is_general":false,"is_global_shared":true,"is_group":false,"is_im":false,
        "is_moved":0,"is_member":true,"is_mpim":true,"is_org_default":false,
        "is_org_mandatory":false,"is_org_shared":true,"is_pending_ext_shared":false,
        "is_private":true,"parent_conversation":null,"purpose":{},
        "connected_limited_team_ids":[],"is_open":true});
    result.extend(forced.as_object().unwrap().clone());
    Ok(Some(Value::Object(result)))
}
#[cfg(test)]
mod tests;
