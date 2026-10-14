//! o5++ searches channels/group DMs and members. Member IDs remain action values;
//! the client never opens or joins a conversation to make a selection.
use super::*;
use directory::Choice;
use serde_json::Value;
use super_platinum_core::{
    block_actions::form::ConversationFilter,
    slack::models::{Channel, User},
    state::Workspace,
};
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Filter {
    pub spec: ConversationFilter,
    pub enterprise_id: Option<String>,
    pub default_channels: bool,
}
impl Filter {
    pub fn allows(&self, kind: &str) -> bool {
        !(kind == "mpim" && self.default_channels) && self.spec.allows(kind)
    }
}
/// Shared workspace policy; callers keep their own surface ownership tickets.
pub(crate) fn workspace_filter(shell: &ShellState, team: &str, raw: &Value) -> Option<Filter> {
    let spec = ConversationFilter::parse(raw).ok()?;
    let enterprise_id = shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(team))
        .and_then(|w| w.enterprise_id.clone());
    let default_channels = spec.include.is_none()
        && raw
            .get("filter")
            .and_then(Value::as_object)
            .is_some_and(|v| !v.is_empty());
    Some(Filter {
        spec,
        enterprise_id,
        default_channels,
    })
}
pub(crate) fn unavailable(filter: &Filter) -> Option<&'static str> {
    (filter.only_from_team_id.is_some() && filter.enterprise_id.is_some()).then_some(
        "Workspace-restricted conversation selection is unavailable for this Enterprise Grid account.")
}

impl std::ops::Deref for Filter {
    type Target = ConversationFilter;
    fn deref(&self) -> &Self::Target {
        &self.spec
    }
}

pub(crate) fn channel_label(workspace: &Workspace, channel: &Channel) -> String {
    if channel.is_im {
        return channel
            .user
            .as_deref()
            .map(|id| workspace.display_name(id))
            .unwrap_or_else(|| channel.id.clone());
    }
    if channel.is_mpim {
        let names = channel
            .extra
            .get("members")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .filter(|id| *id != workspace.self_user_id)
                    .map(|id| workspace.display_name(id))
                    .collect::<Vec<_>>()
            });
        if let Some(names) = names.filter(|names| !names.is_empty()) {
            return names.join(", ");
        }
        return channel.name.clone().unwrap_or_else(|| channel.id.clone());
    }
    channel
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|name| format!("#{name}"))
        .unwrap_or_else(|| channel.id.clone())
}
pub(super) fn member_eligible(w: &Workspace, f: &Filter, u: &User) -> bool {
    if !f.allows("im") || !super_platinum_core::block_actions::form::valid_message_user(&u.id) {
        return false;
    }
    if let Some(team) = &f.only_from_team_id {
        return u.id == w.self_user_id || (external(w, f, u) && member_team(w, u) == team);
    }
    directory::eligible(w, u)
        && (!f.exclude_bot_users
            || (!u.is_bot
                && !matches!(u.id.as_str(), "USLACKBOT" | "USLACKSECURITY")
                && u.extra.get("is_slackbot") != Some(&Value::Bool(true))
                && u.extra.get("is_slacksecuritybot") != Some(&Value::Bool(true))))
}
pub(super) fn channel_eligible(f: &Filter, c: &Channel) -> bool {
    let kind = if c.is_mpim {
        "mpim"
    } else if c.is_im {
        return false;
    } else if c.is_private || c.is_group {
        "private"
    } else if c.is_channel {
        "public"
    } else {
        return false;
    };
    super_platinum_core::block_actions::form::valid_message_conversation(&c.id)
        && f.allows(kind)
        && (c.is_mpim || f.default_channels || !c.is_archived)
        && (!f.exclude_external_shared_channels || !c.is_ext_shared)
        && f.only_from_team_id.as_ref().is_none_or(|team| {
            c.extra
                .get("connectedTeamIds")
                .or_else(|| c.extra.get("connected_team_ids"))
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(team)))
        })
}
fn newer(remote: Option<u64>, cached: Option<u64>) -> bool {
    matches!((remote,cached),(Some(r),Some(c)) if r>c)
}
fn effective_user<'a>(w: &'a Workspace, u: &'a User) -> &'a User {
    w.users
        .get(&u.id)
        .filter(|old| {
            !newer(
                u.extra.get("updated").and_then(Value::as_u64),
                old.extra.get("updated").and_then(Value::as_u64),
            )
        })
        .unwrap_or(u)
}
fn effective_channel<'a>(w: &'a Workspace, c: &'a Channel) -> &'a Channel {
    w.channels
        .get(&c.id)
        .filter(|old| !newer(c.updated, old.updated))
        .unwrap_or(c)
}
pub(crate) fn choice_current(w: &Workspace, f: &Filter, choice: &Choice) -> bool {
    let Some(raw) = &choice.canonical else {
        return false;
    };
    if super_platinum_core::block_actions::form::valid_message_user(&choice.id) {
        serde_json::from_value::<User>(raw.clone())
            .ok()
            .is_some_and(|u| member_eligible(w, f, effective_user(w, &u)))
    } else {
        serde_json::from_value::<Channel>(raw.clone())
            .ok()
            .is_some_and(|c| channel_eligible(f, effective_channel(w, &c)))
    }
}
pub(crate) fn candidates<'a>(
    w: &Workspace,
    f: &Filter,
    users: impl Iterator<Item = (&'a User, bool)>,
    channels: impl Iterator<Item = (&'a Channel, bool)>,
    query: &str,
) -> Vec<Choice> {
    let query = query.trim().trim_start_matches(['#', '@']).to_lowercase();
    let mut choices: Vec<(bool, String, Choice)> = Vec::new();
    let mut add =
        |id: &str, label: String, remote: bool, alternate: &str, canonical: &dyn Fn() -> Value| {
            let folded = label.to_lowercase();
            if (!remote
                && !query.is_empty()
                && !folded.contains(&query)
                && !alternate.to_lowercase().contains(&query))
                || choices.iter().any(|(_, _, c)| c.id == id)
            {
                return;
            }
            let mut item = (
                !query.is_empty() && folded.trim_start_matches('#') != query,
                folded,
                Choice {
                    id: id.into(),
                    label,
                    canonical: None,
                },
            );
            let index = choices.partition_point(|old| {
                (&old.0, &old.1, &old.2.id) <= (&item.0, &item.1, &item.2.id)
            });
            if index < 24 {
                item.2.canonical = Some(canonical());
                choices.insert(index, item);
                choices.truncate(24);
            }
        };
    let mut seen = std::collections::HashSet::new();
    for (remote_user, remote) in users {
        if !seen.insert(remote_user.id.as_str()) {
            continue;
        }
        let u = effective_user(w, remote_user);
        if member_eligible(w, f, u) {
            add(
                &u.id,
                super_platinum_core::state::display_name(Some(u), &u.id),
                remote,
                u.real_name.as_deref().unwrap_or_default(),
                &|| serde_json::to_value(u).unwrap(),
            );
        }
    }
    for (remote_channel, remote) in channels {
        if !seen.insert(remote_channel.id.as_str()) {
            continue;
        }
        let c = effective_channel(w, remote_channel);
        if channel_eligible(f, c) {
            add(
                &c.id,
                channel_label(w, c),
                remote,
                c.name.as_deref().unwrap_or_default(),
                &|| serde_json::to_value(c).unwrap(),
            );
        }
    }
    choices.into_iter().map(|(_, _, c)| c).collect()
}
pub(crate) fn cached(shell: &ShellState, source: &Source, f: &Filter, query: &str) -> Vec<Choice> {
    shell
        .core
        .workspaces
        .get(&source.owner().team_id)
        .map_or_else(Vec::new, |w| {
            candidates(
                w,
                f,
                w.users.values().map(|u| (u, false)),
                w.channels.values().map(|c| (c, false)),
                query,
            )
        })
}

fn member_team<'a>(w: &'a Workspace, u: &'a User) -> &'a str {
    let enterprise = u.extra.get("enterprise_user");
    u.extra
        .get("team_id")
        .and_then(Value::as_str)
        .or_else(|| {
            enterprise
                .filter(|e| e.get("id").and_then(Value::as_str) == Some(&u.id))
                .and_then(|e| e.get("enterprise_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or(&w.team_id)
}
fn external(w: &Workspace, f: &Filter, u: &User) -> bool {
    let team = member_team(w, u);
    let enterprise = u.extra.get("enterprise_user");
    let local = team == w.team_id
        || enterprise
            .and_then(|v| v.get("teams"))
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&w.team_id)));
    let org_local = f.enterprise_id.as_ref().is_some_and(|org| {
        team == org
            || enterprise
                .and_then(|e| e.get("enterprise_id"))
                .and_then(Value::as_str)
                == Some(org)
    });
    !local && !org_local
}

#[cfg(test)]
mod tests;
