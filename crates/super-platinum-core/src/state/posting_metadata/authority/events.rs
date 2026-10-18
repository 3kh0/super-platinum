use super::*;
use crate::slack::events::RtEvent;

/// Explicit runtime hook for an accepted account/config/role/order-loss event.
/// Recovery needs a new BootRead; replaying old BootData cannot bind authority.
pub fn retire(core: &mut CoreAppState, team: &str) {
    if let Some(workspace) = core.workspaces.get_mut(team) {
        workspace.posting_metadata.invalidate();
    }
}

/// Called only after the socket account/Transport/generation fence. Partials
/// invalidate old channel evidence; they are never promoted to full snapshots.
/// Event names/shapes come from channel_lifecycle and retained t0MN handlers.
pub fn observe_event(core: &mut CoreAppState, team: &str, generation: u64, event: &RtEvent) {
    let Some(workspace) = core.workspaces.get(team) else {
        return;
    };
    if workspace.rt_generation != generation {
        return;
    }
    let self_user = workspace.self_user_id.clone();
    observe_join_connections(core, team, event);
    let mut channels: Vec<String> = observe_converted_connections(core, team, event)
        .into_iter()
        .collect();
    let mut all = false;
    match event {
        RtEvent::UserChanged(user) if user.id == self_user => all = true,
        RtEvent::Message(message) => {
            let relevant = match message.subtype.as_deref() {
                Some(
                    "channel_name" | "group_name" | "channel_topic" | "group_topic"
                    | "channel_purpose" | "group_purpose" | "channel_archive" | "group_archive"
                    | "channel_unarchive" | "group_unarchive",
                ) => true,
                Some("channel_join" | "group_join" | "channel_leave" | "group_leave") => {
                    message.user.as_deref() == Some(&self_user)
                }
                _ => false,
            };
            if relevant {
                if let Some(channel) = &message.channel {
                    channels.push(channel.clone());
                } else {
                    all = true;
                }
            }
        }
        RtEvent::Unknown(raw) => match raw.kind.as_str() {
            "feature_access_policies_updated"
            | "team_pref_change"
            | "team_plan_change"
            | "team_plan_freeze_state_changed"
            | "team_domain_change"
            | "team_connection_severed"
            | "user_added_to_team"
            | "user_removed_from_team"
            | "user_invalidated"
            | "user_permissions_added"
            | "user_permissions_removed"
            | "user_permissions_invalidated"
            | "account_type_permissions_changed"
            | "account_type_permissions_refreshed" => all = true,
            "channel_updated" => {
                // t0MN accepts either channel:string or channels:string[].
                if let Some(ids) = raw.rest.get("channels") {
                    if let Some(ids) = super::super::strings(ids) {
                        channels = ids;
                    } else {
                        all = true;
                    }
                } else if let Some(id) = raw
                    .rest
                    .get("channel")
                    .and_then(Value::as_str)
                    .filter(|v| id(v))
                {
                    channels.push(id.into());
                } else {
                    all = true;
                }
            }
            "channel_joined"
            | "group_joined"
            | "mpim_joined"
            | "channel_created"
            | "im_created"
            | "channel_left"
            | "group_left"
            | "channel_rename"
            | "group_rename"
            | "channel_archive"
            | "group_archive"
            | "channel_unarchive"
            | "group_unarchive"
            | "member_joined_channel"
            | "member_left_channel" => {
                if let Some(channel) = raw
                    .rest
                    .get("channel")
                    .and_then(|v| v.as_str().or_else(|| v.get("id").and_then(Value::as_str)))
                    .filter(|v| id(v))
                {
                    channels.push(channel.into());
                } else {
                    all = true;
                }
            }
            _ => {}
        },
        _ => {}
    }
    if all {
        retire(core, team);
        return;
    }
    if channels.is_empty() {
        return;
    }
    // Only the accepted socket scope can append positives. A channel_updated
    // partial is not promoted to posting authority; its explicit connection
    // arrays are sufficient for j08h's monotonic positive-only update.
    if let RtEvent::Unknown(raw) = event
        && raw.kind == "channel_updated"
        && let Some(updates) = raw.rest.get("updates").filter(|v| v.is_object())
        && channels
            .iter()
            .any(|id| core.workspaces[team].channels.contains_key(id))
        && let Ok(authority) = Authority::capture(core, team)
    {
        let _ = authority
            .metadata_mut(core)
            .unwrap()
            .external
            .channel(updates);
    }
    let workspace = core.workspaces.get_mut(team).unwrap();
    let metadata = &mut workspace.posting_metadata;
    // Even before the first bound boot, an accepted event makes its in-flight
    // complete inventory response too old to adopt.
    metadata.runtime.pending_boot = None;
    if let Some(scope) = metadata.runtime.bound.clone()
        && let Some(config) = &scope.configuration
    {
        for channel in channels {
            invalidate_channel(metadata, config, &channel);
        }
    }
}

fn observe_join_connections(core: &mut CoreAppState, team: &str, event: &RtEvent) {
    let RtEvent::Unknown(raw) = event else {
        return;
    };
    let joined = matches!(
        raw.kind.as_str(),
        "channel_joined" | "group_joined" | "mpim_joined"
    );
    let created = matches!(raw.kind.as_str(), "channel_created" | "im_created");
    if !joined && !created {
        return;
    }
    let Ok(authority) = Authority::capture(core, team) else {
        return;
    };
    let metadata = authority.metadata_mut(core).unwrap();
    let Some(channel) = raw.rest.get("channel").filter(|v| v.is_object()) else {
        metadata.external.retire();
        return;
    };
    if !channel.get("id").and_then(Value::as_str).is_some_and(id) {
        metadata.external.retire();
        return;
    }
    // nc8t: joins always upsert via46wa.F; created IMs always upsert,
    // other created channels only for an unrestricted current member (HkVr.DM).
    if created && channel.get("is_im") != Some(&Value::Bool(true)) {
        match metadata
            .member
            .get("is_restricted")
            .and_then(Value::as_bool)
        {
            Some(true) => return,
            Some(false) => {}
            None => {
                metadata.external.retire();
                return;
            }
        }
    }
    let _ = metadata.external.channel(channel);
}

// nc8t remaps group→channel, checks current known/nonarchived state, then
// dispatches46wa.F. XDd5.M OR1C9z.S reduces to XDd5.M: the latter's PJfu.O
// already requires XDd5.M. Existing membership is copied only for the ordinary
// channel upsert; j08h reads the actual connection arrays unchanged.
fn observe_converted_connections(
    core: &mut CoreAppState,
    team: &str,
    event: &RtEvent,
) -> Option<String> {
    let RtEvent::Unknown(raw) = event else {
        return None;
    };
    let field = match raw.kind.as_str() {
        "channel_converted_to_shared" => "channel",
        "group_converted_to_shared" => "group",
        _ => return None,
    };
    let channel = raw.rest.get(field)?.as_object()?;
    let channel_id = channel.get("id")?.as_str().filter(|v| id(v))?;
    let existing = core.workspaces.get(team)?.channels.get(channel_id)?;
    if existing.is_archived || !json_truthy(channel.get("is_org_shared")) {
        return None;
    }
    let authority = Authority::capture(core, team).ok()?;
    let _ = authority
        .metadata_mut(core)
        .ok()?
        .external
        .channel(&Value::Object(channel.clone()));
    Some(channel_id.into())
}

fn json_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
