//! Calendar-owned posting acquisition. Search/cache objects never grant posting.
//! The shared producer retains exact boot/configuration and raw-channel custody.
use super::*;
use serde_json::Value;
use super_platinum_core::{
    block_actions::posting_policy,
    slack::{PreparedRequest, models::Channel},
    state::posting_metadata::{
        Unknown,
        authority::{Authority, BootRead, RawChannel},
    },
};
mod reads;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) fn test_acquire(s: &mut ShellState, id: u64, channels: Vec<Value>) {
    tests::acquire(s, id, channels);
}

#[derive(Clone, Default)]
pub(super) struct Facts {
    channels: BTreeMap<String, Witness>,
}
impl Facts {
    pub(super) fn invalidate(&mut self) {
        self.channels.clear();
    }
}
#[derive(Clone)]
struct Witness {
    generation: u64,
    authority: Authority,
    raw: RawChannel,
}
impl Witness {
    fn current(&self, s: &ShellState, ctx: &Context, generation: u64) -> bool {
        if self.generation != generation
            || !self.authority.current(&s.core)
            || !self.raw.current(&s.core)
        {
            return false;
        }
        let Ok(channel) = serde_json::from_value::<Channel>(self.raw.value().clone()) else {
            return false;
        };
        sharing::channel_base(s, ctx, &channel)
            && self
                .authority
                .metadata(&s.core)
                .and_then(|m| m.effective(self.authority.configuration(), self.raw.value()))
                .is_ok_and(|flags| flags.permits_root_post())
    }
}
pub(super) fn allowed(s: &ShellState, ctx: &Context, channel: &Channel) -> bool {
    let Some(d) = s.message_buttons.shortcuts.dialog.as_ref() else {
        return false;
    };
    let Some(c) = active(s, d.id) else {
        return false;
    };
    std::ptr::eq(ctx, d.owner.ctx.as_ref())
        && d.owner.ctx.epoch == ctx.epoch
        && d.owner.ctx.session.team_id == ctx.session.team_id
        && c.posting
            .channels
            .get(&channel.id)
            .is_some_and(|w| w.current(s, ctx, c.share_generation))
}

/// One search owns its entire finite read budget. New query/Cancel/Create/
/// account/credential changes refuse dispatch and adoption through this fence.
struct Query {
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    remaining: usize,
}
impl Query {
    fn current(&self, s: &ShellState) -> bool {
        sharing::query_current(s, self.id, self.generation)
            && context(s, self.id).is_some_and(|ctx| Arc::ptr_eq(&ctx, &self.ctx))
    }
    async fn read(
        &mut self,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if self.remaining == 0 || !self.current(&state.peek()) {
            return Err(unavailable());
        }
        self.remaining -= 1;
        // Existing read path preserves owner-bound Retry-After and credential
        // checks. No write, automatic policy retry or global tick is added.
        let result = load::read(state, self.id, &self.ctx, request).await;
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
}

/// Rank the same local/remote candidate domain before asking permission. Taking
/// the bounded first100 is presentation work, not an eligibility grant. The
/// selected ID comes first, so a hidden search result still gets revalidated.
fn candidates(
    s: &ShellState,
    id: u64,
    ctx: &Context,
    remote: &[sharing::Destination],
) -> Vec<String> {
    let Some(c) = active(s, id) else {
        return vec![];
    };
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return vec![];
    };
    let query = c.share_query.trim().to_lowercase();
    let mut rows = BTreeMap::new();
    for channel in w
        .channels
        .values()
        .chain(remote.iter().filter_map(|d| match d {
            sharing::Destination::Channel(c) => Some(c),
            _ => None,
        }))
        .chain(c.share_selected.iter().filter_map(|d| match d {
            sharing::Destination::Channel(c) => Some(c),
            _ => None,
        }))
    {
        if !sharing::channel_base(s, ctx, channel) {
            continue;
        }
        let name = channel
            .name
            .clone()
            .or_else(|| {
                channel
                    .user
                    .as_ref()
                    .map(|id| super_platinum_core::state::display_name(w.users.get(id), id))
            })
            .unwrap_or_else(|| channel.id.clone());
        if channel.id == c.share_channel || query.is_empty() || name.to_lowercase().contains(&query)
        {
            rows.insert(channel.id.clone(), name);
        }
    }
    let mut rows: Vec<_> = rows.into_iter().collect();
    rows.sort_by(|(aid, an), (bid, bn)| {
        (aid != &c.share_channel, an, aid).cmp(&(bid != &c.share_channel, bn, bid))
    });
    rows.into_iter().take(100).map(|(id, _)| id).collect()
}

pub(super) async fn refresh(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    remote: &[sharing::Destination],
) -> bool {
    let (mut query, ids) = {
        let s = state.peek();
        let Some(ctx) = context(&s, id) else {
            return false;
        };
        if !sharing::query_current(&s, id, generation) {
            return false;
        }
        let ids = candidates(&s, id, &ctx, remote);
        (
            Query {
                id,
                generation,
                ctx,
                remaining: 256,
            },
            ids,
        )
    };
    if ids.is_empty() {
        return true;
    }
    let Ok((mut authority, mut recovered)) = reads::authority(state, &mut query).await else {
        return false;
    };
    // Unknown effective flags after retirement need a complete observation,
    // never omission-as-false from a sparse conversations.info response.
    loop {
        let mut complete = true;
        let mut restart = false;
        for channel in &ids {
            if !query.current(&state.peek()) {
                return false;
            }
            match reads::channel(state, &mut query, &authority, channel).await {
                Ok(witness) => {
                    let mut s = state.write();
                    if !query.current(&s) {
                        return false;
                    }
                    active_mut(&mut s, id)
                        .unwrap()
                        .posting
                        .channels
                        .insert(channel.clone(), witness);
                }
                Err(Unknown::Policy(posting_policy::Unknown::EffectiveFlags)) if !recovered => {
                    recovered = true;
                    let Ok(fresh) = reads::boot(state, &mut query).await else {
                        return false;
                    };
                    authority = fresh;
                    let mut s = state.write();
                    if !query.current(&s) {
                        return false;
                    }
                    active_mut(&mut s, id).unwrap().posting.invalidate();
                    restart = true;
                    break;
                }
                // A read failure maps to Stale after load::read has retained
                // its actual Retry-After. Stop this query; do not fan the same
                // offline/credential failure out across the remaining rows.
                Err(Unknown::Stale | Unknown::Identity | Unknown::Configuration) => return false,
                Err(_) => complete = false,
            }
            if query.remaining == 0 || cooling(&state.peek(), &query.ctx) {
                return false;
            }
        }
        if !restart {
            return complete;
        }
    }
}
fn unavailable() -> Error {
    Error::Api("calendar_posting_observation_unavailable".into())
}
