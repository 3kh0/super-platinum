//! Synthetic disconnected named fixture cache, never remote discovery.
use super::*;
pub(crate) fn seed(shell: &mut ShellState) -> Result<(), String> {
    if !super::super::super::enabled(shell)
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        return Err("Disconnected Channel seed required".into());
    }
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let w = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing workspace")?;
    let mut raw = vec![
        json!({"id":"C00000001","name":"alpha-public","is_channel":true,"is_member":false}),
        json!({"id":"G00000002","name":"beta-private","is_group":true,"is_private":true,"is_member":false}),
        json!({"id":"C00000003","name":"gamma-external","is_channel":true,"is_ext_shared":true,"shared_team_ids":["T_OTHER"]}),
    ];
    for i in 4..=10 {
        raw.push(json!({"id":format!("C{i:08}"),"name":format!("lane-{i:02}"),"is_channel":true}));
    }
    raw.extend([
 json!({"id":"G00000011","name":"record-member","is_group":true,"is_private":true,"is_member":true,"properties":{"record_channel":{"record_id":"S:a:b"}}}),
 json!({"id":"C00000012","name":"zulu-external","is_channel":true,"is_ext_shared":true,"shared_team_ids":["T_OTHER"]}),
 json!({"id":"C00000013","name":"archived","is_channel":true,"is_archived":true}),
 json!({"id":"D00000014","name":"dm","is_im":true}),
 json!({"id":"G00000015","name":"mpim","is_group":true,"is_mpim":true}),
 json!({"id":"C00000016","name":"unknown"}),
 json!({"id":"C00000017","is_channel":true}),
 json!({"id":"G00000018","name":"record-nonmember","is_group":true,"is_private":true,"is_member":false,"properties":{"record_channel":{"record_id":"S:a:b"}}})]);
    let channels = raw
        .into_iter()
        .map(|v| {
            serde_json::from_value::<super_platinum_core::slack::models::Channel>(v)
                .map(|c| (c.id.clone(), c))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
    if channels.len() != 18 || channels.keys().any(|id| w.frecency.contains_key(id)) {
        return Err("Channel fixture has nonneutral directory priority".into());
    }
    w.channels = channels;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Live Channel cache forbidden".into());
    }
    let team = shell.core.active_team.as_ref().ok_or("Missing team")?;
    let w = shell.core.workspaces.get(team).ok_or("Missing workspace")?;
    if w.channels.len() != 18 || w.channels.keys().any(|id| w.frecency.contains_key(id)) {
        return Err("Channel cache inventory/priority changed".into());
    }
    Ok(
        json!({"team":team,"self":w.self_user_id,"channels":w.channels,"users":w.users,"frecency":w.frecency}),
    )
}
