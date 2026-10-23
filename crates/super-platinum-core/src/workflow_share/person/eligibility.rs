use serde_json::Value;

/// Known local ordinary-human subset of the member picker, not FS2H channel
/// permission. Self and guests are not categorically denied. App/group/external
/// destinations remain outside this slice, not a source-wide prohibition.
pub fn eligible(peer: &Value, team: &str) -> bool {
    if !scope(team)
        || !peer["id"].as_str().is_some_and(|id| {
            id_kind(id, b"UW")
                && !matches!(
                    id,
                    "USLACK" | "USLACKBOT" | "USLACKSECURITY" | "USEARCH" | "UCHANNELEXPERT"
                )
        })
        || !clear(
            peer,
            &[
                "deleted",
                "isUnknown",
                "isNonExistent",
                "is_bot",
                "is_app_user",
                "is_slackbot",
                "is_slacksecuritybot",
                "is_workflow_bot",
                "is_profile_only_user",
                "is_agentforce_bot",
                "isExternal",
                "is_stranger",
            ],
        )
        || !matches!(peer.get("bot_id"), None | Some(Value::Null))
    {
        return false;
    }
    match peer.get("profile") {
        None | Some(Value::Null) => {}
        Some(Value::Object(p)) if matches!(p.get("api_app_id"), None | Some(Value::Null)) => {}
        _ => return false,
    }
    let home = match peer.get("team_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if scope(id) => Some(id.as_str()),
        _ => return false,
    };
    let teams = match peer.get("enterprise_user") {
        None | Some(Value::Null) => None,
        Some(Value::Object(e)) => match e.get("teams") {
            None | Some(Value::Null) => None,
            Some(Value::Array(t)) if t.iter().all(|id| id.as_str().is_some_and(scope)) => Some(t),
            _ => return false,
        },
        _ => return false,
    };
    home == Some(team) || teams.is_some_and(|t| t.iter().any(|id| id == team))
}
pub(super) fn clear(raw: &Value, fields: &[&str]) -> bool {
    fields.iter().all(|f| {
        matches!(
            raw.get(*f),
            None | Some(Value::Null) | Some(Value::Bool(false))
        )
    })
}
fn scope(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub(super) fn id_kind(id: &str, prefixes: &[u8]) -> bool {
    id.len() >= 9 && id.len() <= 64 && prefixes.contains(&id.as_bytes()[0]) && scope(id)
}
