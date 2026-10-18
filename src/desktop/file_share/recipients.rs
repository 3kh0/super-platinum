//! Existing conversation selection evidence. No discovery, join or DM creation.
use super::*;
use serde_json::json;

/// Null for the unchanged joined-channel route. DM evidence includes the entire
/// local conversation and human peer for comparison only, not posting policy.
fn resolve<'a>(
    shell: &'a ShellState,
    id: &str,
) -> Option<(
    &'a super_platinum_core::slack::models::Channel,
    Option<&'a super_platinum_core::slack::models::User>,
)> {
    let team = shell.core.active_team.as_ref()?;
    let workspace = shell.core.workspaces.get(team)?;
    let c = workspace.channels.get(id)?;
    if c.id != id
        || !(9..=64).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || c.is_archived
        || c.is_ext_shared
        || [
            "isUnknown",
            "isNonExistent",
            "is_frozen",
            "is_pending_ext_shared",
            "is_file",
        ]
        .iter()
        .any(|key| c.extra.get(*key).is_some_and(|v| v != &Value::Bool(false)))
    {
        return None;
    }
    if c.is_im || c.is_mpim {
        if c.is_im == c.is_mpim
            || c.is_channel
            || c.extra.get("is_open") != Some(&Value::Bool(true))
            || (c.is_im && !id.starts_with('D'))
            || (c.is_mpim && !id.starts_with('G'))
        {
            return None;
        }
        let peer = if c.is_im {
            let id = c.user.as_deref()?;
            let peer = workspace.users.get(id)?;
            if peer.id != id
                || id == "USLACK"
                || id == "USLACKBOT"
                || peer.deleted
                || peer.is_bot
                || ["isUnknown", "isNonExistent"].iter().any(|key| {
                    peer.extra
                        .get(*key)
                        .is_some_and(|v| v != &Value::Bool(false))
                })
            {
                return None;
            }
            Some(peer)
        } else {
            None
        };
        Some((c, peer))
    } else {
        (matches!(id.as_bytes().first(), Some(b'C' | b'G'))
            && (c.is_channel || c.is_private || c.is_group)
            && c.extra.get("is_member") == Some(&Value::Bool(true)))
        .then_some((c, None))
    }
}
pub(super) fn eligible(shell: &ShellState, id: &str) -> bool {
    resolve(shell, id).is_some()
}
pub(super) fn capture(shell: &ShellState, id: &str) -> Option<Value> {
    let (c, peer) = resolve(shell, id)?;
    if !c.is_im && !c.is_mpim {
        return Some(Value::Null);
    }
    let value = json!({"channel":c,"peer":peer});
    bounded(&value).then_some(value)
}
pub(super) fn chosen(shell: &ShellState, dialog: &str, id: &str) -> bool {
    let Some(current) = capture(shell, id) else {
        return false;
    };
    current.is_null()
        || shell.file_share.selected_recipient.as_ref().is_some_and(
            |(owner, selected, original)| owner == dialog && selected == id && original == &current,
        )
}
/// A fresh response may update permissions, never retarget the selected peer or
/// switch between conversation families. Membership metadata, when captured for
/// a group DM, must remain exact. No absent members list is invented.
pub(super) fn same_target(local: &Value, raw: &Value) -> bool {
    let im = local["is_im"] == true;
    let mpim = local["is_mpim"] == true;
    (raw["is_im"] == true) == im
        && (raw["is_mpim"] == true) == mpim
        && (!im || raw["user"] == local["user"])
        && (!mpim
            || local
                .get("members")
                .is_none_or(|members| raw.get("members") == Some(members)))
}
pub(super) fn raw_peer(response: &Value, expected: &str) -> Option<Value> {
    if !bounded(response)
        || response["ok"] != true
        || response.get("error").is_some_and(|v| !v.is_null())
        || response
            .get("failed_ids")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
    {
        return None;
    }
    let rows = response["results"].as_array()?;
    let peer = rows.first()?;
    if rows.len() != 1
        || peer["id"].as_str() != Some(expected)
        || ["isUnknown", "isNonExistent"]
            .iter()
            .any(|key| peer.get(*key).is_some_and(|v| v != &Value::Bool(false)))
    {
        return None;
    }
    Some(peer.clone())
}
use super::policy_owner::bounded;
