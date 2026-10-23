use crate::block_actions::posting_policy::Flags;
use serde_json::Value;
use std::collections::BTreeSet;

/// Peer freshness, canonical identity and posting-policy acquisition remain
/// caller obligations. Flags must come from independent actual policy facts.
pub fn eligible(raw: &Value, flags: Flags, team: &str, self_id: &str, peers: &[Value]) -> bool {
    eligible_with_membership(raw, raw, flags, team, self_id, peers)
}
/// A fresh posting response may omit members. Keep the independently acquired
/// canonical membership object separate, with both owners current at the host.
/// Never copy requested members into a sparse posting or open response.
pub fn eligible_with_membership(
    posting: &Value,
    membership: &Value,
    flags: Flags,
    team: &str,
    self_id: &str,
    peers: &[Value],
) -> bool {
    flags.permits_root_post()
        && membership["fromAnotherTeam"] == false
        && posting["id"] == membership["id"]
        && channel_shape(posting)
        && members(peers, team, self_id).is_some_and(|(_, ids)| {
            channel_matches(membership, &ids)
                && (posting.get("members").is_none() || channel_matches(posting, &ids))
        })
}
pub(super) fn members(
    peers: &[Value],
    team: &str,
    self_id: &str,
) -> Option<(Vec<Value>, Vec<String>)> {
    if !id_kind(self_id, b"UW") {
        return None;
    }
    let mut selected = Vec::<Value>::new();
    for p in peers {
        if !super::super::person::eligible(p, team) {
            return None;
        }
        if let Some(old) = selected.iter().find(|old| old["id"] == p["id"]) {
            if old != p {
                return None;
            }
        } else {
            selected.push(p.clone());
        }
    }
    // Source Send has an eight-selected-member limit. Rejecting before open
    // is this bounded factory policy, not a claim about source open timing.
    if !(2..=8).contains(&selected.len()) {
        return None;
    }
    let mut ids = vec![self_id.to_owned()];
    for p in &selected {
        let id = p["id"].as_str()?;
        if !ids.iter().any(|v| v == id) {
            ids.push(id.to_owned());
        }
    }
    // Self plus only one other person degenerates to DM, outside this MPIM API.
    (ids.len() >= 3).then_some((selected, ids))
}
pub(super) fn channel_matches(raw: &Value, expected: &[String]) -> bool {
    if !channel_shape(raw) {
        return false;
    }
    let Some(actual) = raw["members"].as_array() else {
        return false;
    };
    let Some(ids) = actual
        .iter()
        .map(|v| v.as_str().filter(|id| id_kind(id, b"UW")))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let unique = ids.iter().copied().collect::<BTreeSet<_>>();
    if ids.len() != expected.len()
        || unique.len() != ids.len()
        || !expected.iter().all(|id| unique.contains(id.as_str()))
    {
        return false;
    }
    true
}
fn channel_shape(raw: &Value) -> bool {
    if !raw["id"].as_str().is_some_and(|id| id_kind(id, b"CG")) || raw["is_mpim"] != true {
        return false;
    }
    for field in [
        "is_im",
        "is_archived",
        "is_frozen",
        "isUnknown",
        "isNonExistent",
        "is_ext_shared",
        "is_pending_ext_shared",
        "is_file",
        "fromAnotherTeam",
    ] {
        if !matches!(
            raw.get(field),
            None | Some(Value::Null) | Some(Value::Bool(false))
        ) {
            return false;
        }
    }
    for field in [
        "is_channel",
        "is_private",
        "is_group",
        "is_member",
        "is_open",
        "is_read_only",
        "is_thread_only",
    ] {
        if !matches!(
            raw.get(field),
            None | Some(Value::Null) | Some(Value::Bool(_))
        ) {
            return false;
        }
    }
    if !matches!(raw.get("user"), None | Some(Value::Null)) {
        return false;
    }
    match raw.get("properties") {
        None | Some(Value::Null) => true,
        Some(Value::Object(p)) => match p.get("huddles") {
            None | Some(Value::Null) => true,
            Some(Value::Object(h)) => matches!(
                h.get("is_huddle_channel"),
                None | Some(Value::Null) | Some(Value::Bool(false))
            ),
            _ => false,
        },
        _ => false,
    }
}
fn id_kind(id: &str, prefixes: &[u8]) -> bool {
    (9..=64).contains(&id.len())
        && prefixes.contains(&id.as_bytes()[0])
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
