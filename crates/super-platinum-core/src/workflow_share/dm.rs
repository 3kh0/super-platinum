//! Existing human IM reduction of FS2H, selected through GPeC's channel branch.
//! Local human peers are this native slice's boundary, not a claim that Slack
//! universally denies app, bot or external IMs. No conversation is opened here.
use crate::block_actions::posting_policy::Flags;
use serde_json::Value;

/// Caller owns the original channel/member observations and their freshness.
/// `fromAnotherTeam` is the canonical derived fact, never a wire default.
/// Flags must be actually acquired effective policy; unknown is not false.
pub fn existing_dm_eligible(channel: &Value, peer: &Value, flags: Flags, team: &str) -> bool {
    if !scope(team)
        || !flags.permits_root_post()
        || !channel["id"].as_str().is_some_and(|id| id_kind(id, b"D"))
        || channel["is_im"] != true
        || channel["fromAnotherTeam"] != false
        || !peer["id"].as_str().is_some_and(human_id)
        || channel["user"].as_str() != peer["id"].as_str()
    {
        return false;
    }
    // Source type/archive/freeze/pending-IM/huddle exclusions; contradictory
    // types and other conversation families are deliberately unsupported.
    if !clear(
        channel,
        &[
            "is_channel",
            "is_private",
            "is_group",
            "is_mpim",
            "is_archived",
            "is_frozen",
            "isUnknown",
            "isNonExistent",
            "is_pending_ext_shared",
            "is_ext_shared",
            "is_file",
        ],
    ) || !clear(
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
    ) {
        return false;
    }
    // These app association and external exclusions bound ordinary-human
    // support. FS2H's app collaborator/interactable branches are not emulated.
    if !matches!(peer.get("bot_id"), None | Some(Value::Null)) {
        return false;
    }
    match peer.get("profile") {
        None | Some(Value::Null) => {}
        Some(Value::Object(profile)) => {
            if !matches!(profile.get("api_app_id"), None | Some(Value::Null)) {
                return false;
            }
        }
        _ => return false,
    }
    if !local_peer(peer, team) {
        return false;
    }
    let properties = match channel.get("properties") {
        None | Some(Value::Null) => return true,
        Some(Value::Object(value)) => value,
        _ => return false,
    };
    match properties.get("huddles") {
        None | Some(Value::Null) => true,
        Some(Value::Object(huddles)) => matches!(
            huddles.get("is_huddle_channel"),
            None | Some(Value::Null) | Some(Value::Bool(false))
        ),
        _ => false,
    }
}

fn clear(raw: &Value, fields: &[&str]) -> bool {
    fields.iter().all(|field| {
        matches!(
            raw.get(*field),
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
fn id_kind(id: &str, prefixes: &[u8]) -> bool {
    id.len() >= 9 && prefixes.contains(&id.as_bytes()[0]) && scope(id)
}
fn human_id(id: &str) -> bool {
    id_kind(id, b"UW")
        && !matches!(
            id,
            "USLACK" | "USLACKBOT" | "USLACKSECURITY" | "USEARCH" | "UCHANNELEXPERT"
        )
}
fn local_peer(peer: &Value, team: &str) -> bool {
    let home = match peer.get("team_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if scope(id) => Some(id.as_str()),
        _ => return false,
    };
    let teams = match peer.get("enterprise_user") {
        None | Some(Value::Null) => None,
        Some(Value::Object(enterprise)) => match enterprise.get("teams") {
            None | Some(Value::Null) => None,
            Some(Value::Array(teams)) if teams.iter().all(|id| id.as_str().is_some_and(scope)) => {
                Some(teams)
            }
            _ => return false,
        },
        _ => return false,
    };
    home == Some(team) || teams.is_some_and(|teams| teams.iter().any(|id| id == team))
}

#[cfg(test)]
mod tests;
