//! Dialog/query-owned app-DM metadata. No global app cache or guessed grants.
use super::*;
use super_platinum_core::{
    block_actions::{app_eligibility as policy, home},
    slack::models::Channel,
};

#[derive(Clone, Default)]
pub(super) struct Metadata {
    homes: BTreeMap<String, Entry>,
}
impl Metadata {
    pub(super) fn invalidate(&mut self) {
        self.homes.clear();
    }
}
#[derive(Clone)]
struct Entry {
    generation: u64,
    peer: String,
    app: String,
    channel: Channel,
    home: policy::AppHomePosting,
    profile: Option<policy::AppProfile>,
}
#[derive(Clone)]
struct Ticket {
    id: u64,
    generation: u64,
    channel: String,
    peer: String,
    app: String,
}
fn capture(s: &ShellState, id: u64, generation: u64, ch: &Channel) -> Option<Ticket> {
    if !sharing::query_current(s, id, generation)
        || !sharing::channel_base(s, context(s, id)?.as_ref(), ch)
        || !ch.is_im
    {
        return None;
    }
    let ctx = context(s, id)?;
    let peer = s
        .core
        .workspaces
        .get(&ctx.session.team_id)?
        .users
        .get(ch.user.as_ref()?)?;
    if !policy::app_dm(peer).ok()? {
        return None;
    }
    Some(Ticket {
        id,
        generation,
        channel: ch.id.clone(),
        peer: peer.id.clone(),
        app: policy::app_id(peer)?.into(),
    })
}
fn ticket_current(s: &ShellState, ticket: &Ticket) -> bool {
    if !sharing::query_current(s, ticket.id, ticket.generation) {
        return false;
    }
    let Some(ctx) = context(s, ticket.id) else {
        return false;
    };
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return false;
    };
    w.users.get(&ticket.peer).is_some_and(|u| {
        !u.deleted
            && policy::app_dm(u).is_ok_and(|app| app)
            && policy::app_id(u) == Some(&ticket.app)
    }) && w.channels.get(&ticket.channel).is_none_or(|c| {
        c.is_im && c.user.as_deref() == Some(&ticket.peer) && sharing::channel_base(s, &ctx, c)
    })
}
fn adopt(
    s: &mut ShellState,
    ticket: &Ticket,
    raw: &serde_json::Value,
    profile: Option<policy::AppProfile>,
) -> bool {
    if !ticket_current(s, ticket) {
        return false;
    }
    let Ok(home) = policy::AppHomePosting::decode(&ticket.channel, &ticket.app, raw) else {
        return false;
    };
    let Ok(channel) = serde_json::from_value::<Channel>(raw["channel"].clone()) else {
        return false;
    };
    let ctx = context(s, ticket.id).unwrap();
    if !channel.is_im
        || channel.user.as_deref() != Some(&ticket.peer)
        || !sharing::channel_base(s, &ctx, &channel)
        || profile.as_ref().is_none_or(|p| p.app() != ticket.app)
    {
        return false;
    }
    let entry = Entry {
        generation: ticket.generation,
        peer: ticket.peer.clone(),
        app: ticket.app.clone(),
        channel,
        home,
        profile,
    };
    active_mut(s, ticket.id)
        .unwrap()
        .metadata
        .homes
        .insert(ticket.channel.clone(), entry);
    true
}
pub(super) fn allowed(s: &ShellState, ctx: &Context, ch: &Channel) -> bool {
    let Some(d) = s.message_buttons.shortcuts.dialog.as_ref() else {
        return false;
    };
    if d.owner.ctx.epoch != ctx.epoch || d.owner.ctx.session.team_id != ctx.session.team_id {
        return false;
    }
    let Some(c) = active(s, d.id) else {
        return false;
    };
    let Some(entry) = c.metadata.homes.get(&ch.id) else {
        return false;
    };
    let ticket = Ticket {
        id: d.id,
        generation: entry.generation,
        channel: ch.id.clone(),
        peer: entry.peer.clone(),
        app: entry.app.clone(),
    };
    ticket_current(s, &ticket)
        && ch.user.as_deref() == Some(&entry.peer)
        && sharing::channel_base(s, ctx, &entry.channel)
        && entry
            .home
            .allows(&ch.id, &entry.app, true, entry.profile.as_ref())
}

/// Only the bounded currently offered/selected bot conversations are hydrated.
/// Each await is fenced by exact dialog/query/peer/app identity. A new query
/// invalidates the old grants immediately; Cancel or newer edits stop the reads.
pub(super) async fn refresh(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Context,
    remote: &[sharing::Destination],
) {
    let tickets = {
        let s = state.peek();
        let Some(c) = active(&s, id) else {
            return;
        };
        let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
            return;
        };
        let query = c.share_query.trim().to_lowercase();
        let mut channels: BTreeMap<String, Channel> = BTreeMap::new();
        for ch in w
            .channels
            .values()
            .chain(remote.iter().filter_map(|d| match d {
                sharing::Destination::Channel(c) => Some(c),
                _ => None,
            }))
        {
            if !ch.is_im {
                continue;
            }
            let label = ch
                .name
                .clone()
                .or_else(|| {
                    ch.user
                        .as_ref()
                        .map(|u| super_platinum_core::state::display_name(w.users.get(u), u))
                })
                .unwrap_or_default();
            if ch.id == c.share_channel || query.is_empty() || label.to_lowercase().contains(&query)
            {
                channels.insert(ch.id.clone(), ch.clone());
            }
        }
        let mut tickets: Vec<_> = channels
            .values()
            .filter_map(|ch| capture(&s, id, generation, ch))
            .collect();
        tickets.sort_by_key(|t| (t.channel != c.share_channel, t.channel.clone()));
        tickets.truncate(100);
        tickets
    };
    for ticket in tickets {
        if !ticket_current(&state.peek(), &ticket) {
            return;
        }
        let Ok(request) = home::fetch(&ctx.client, &ctx.session, &ticket.channel) else {
            continue;
        };
        let ingress =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session)
                .for_channels([&ticket.channel]);
        let Ok(raw) = load::read(state, id, ctx, request).await else {
            continue;
        };
        if !ticket_current(&state.peek(), &ticket) {
            return;
        }
        // Current FS2H always consults app ACL metadata for an identified
        // app DM. The historical app_acls_for_all_apps assignment no longer
        // controls this branch. Missing/failed acquisition cannot grant it.
        if !ticket_current(&state.peek(), &ticket) {
            return;
        }
        let Ok(request) = policy::profile_request(&ctx.client, &ctx.session, &ticket.app) else {
            continue;
        };
        let Ok(response) = load::read(state, id, ctx, request).await else {
            continue;
        };
        let Ok(profile) = policy::AppProfile::decode(&ticket.app, &response) else {
            continue;
        };
        let accepted = adopt(&mut state.write(), &ticket, &raw, Some(profile));
        if accepted {
            if let Ok(channel) = serde_json::from_value::<Channel>(raw["channel"].clone()) {
                ingress.channels(&state, &[channel]);
            }
        }
    }
}

#[cfg(test)]
mod tests;
