//! Source-owned positive MPIM extension beside the independent legacy first page.
use super::*;
use crate::app_home::channel_fetches::{canonical, search as intake};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
mod admission;
mod origins;
mod participants;
mod read;
pub(super) use admission::{Prepared, prepare};
pub(super) use read::read;
#[derive(Clone, Default)]
pub(in crate::message_buttons::shortcuts::calendar) struct State {
    origins: origins::Origins,
    facts: Option<Arc<canonical::Canonical>>,
    generation: u64,
    hits: BTreeSet<String>,
    participants: Option<participants::Frame>,
}
impl State {
    pub(super) fn guarded(&self, id: &str) -> bool {
        self.origins.guarded(id)
    }
}
fn unavailable() -> Error {
    Error::Api("calendar_open_mpim_unavailable".into())
}
fn valid_id(id: &str) -> bool {
    id.len() <= 255 && domain::identifier(id)
}
pub(super) fn start_query(s: &mut ShellState, id: u64, generation: u64, ctx: &Context) {
    let facts = active(s, id)
        .filter(|c| !c.mpims.origins.is_empty())
        .and_then(|_| canonical::observed(s, &ctx.session.team_id).ok());
    if let Some(c) = active_mut(s, id) {
        c.mpims.facts = facts;
        c.mpims.generation = generation;
        c.mpims.hits.clear();
        c.mpims.participants = None;
    }
}
pub(super) struct Acquired {
    source: Arc<canonical::Canonical>,
    ids: Vec<String>,
}
pub(super) struct Outcome {
    pub(super) legacy: Vec<Channel>,
    pub(super) acquired: Option<Acquired>,
    pub(super) failed: bool,
}
pub(super) fn append(
    s: &mut ShellState,
    id: u64,
    generation: u64,
    ctx: &Context,
    acquired: Acquired,
    remote: &mut Vec<Destination>,
) -> bool {
    if !owned_current(s, id, generation, ctx)
        || !acquired.source.current_for(s, &ctx.session.team_id)
    {
        return false;
    }
    let Some(c) = active(s, id) else {
        return false;
    };
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return false;
    };
    let rows: HashMap<_, _> = acquired
        .source
        .rows()
        .iter()
        .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
        .collect();
    if acquired.ids.iter().any(|id| {
        !c.mpims.origins.guarded(id)
            || !w
                .channels
                .get(id)
                .zip(rows.get(id.as_str()))
                .is_some_and(|(native, row)| admission::open(row, native))
    }) {
        return false;
    }
    let channels: Vec<_> = acquired
        .ids
        .iter()
        .map(|id| w.channels[id].clone())
        .collect();
    let participants =
        participants::capture(s, ctx, &rows, &acquired.ids, &c.share_query, id, generation);
    let c = active_mut(s, id).unwrap();
    c.mpims.participants = participants;
    c.mpims.facts = Some(acquired.source);
    c.mpims.generation = generation;
    c.mpims.hits = acquired.ids.into_iter().collect();
    for channel in channels {
        if let Some(slot) = remote.iter_mut().find(|v| v.id() == channel.id) {
            // Keep provider order, replace stale same-ID payload with actual canonical data.
            *slot = Destination::Channel(channel);
        } else {
            remote.push(Destination::Channel(channel));
        }
    }
    true
}
pub(super) fn participant_candidates(
    s: &ShellState,
    id: u64,
    ctx: &Context,
    remote: &[Destination],
) -> BTreeSet<String> {
    let Some(c) = active(s, id) else {
        return BTreeSet::new();
    };
    if !owned_current(s, id, c.share_generation, ctx) {
        return BTreeSet::new();
    }
    let Some(prepared) = prepare(s, id) else {
        return BTreeSet::new();
    };
    let Some(frame) = &c.mpims.participants else {
        return BTreeSet::new();
    };
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return BTreeSet::new();
    };
    remote
        .iter()
        .filter_map(|dest| match dest {
            Destination::Channel(raw) if c.mpims.guarded(&raw.id) && frame.contains(&raw.id) => {
                let native = w.channels.get(&raw.id)?;
                prepared
                    .remote(native, c.share_generation)
                    .then(|| raw.id.clone())
            }
            _ => None,
        })
        .collect()
}
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
