//! One group resolution for ordered human tokens; never a fan-out sender.
use super::*;
use crate::app_home::channel_fetches::{
    canonical,
    search::{direct::PostingRead, open::OpenRead},
};
use super_platinum_core::{
    block_actions::posting_policy::Flags, slack::api, workflow_share::group as protocol,
};

pub(super) struct Opening {
    token: u64,
    peer_ids: Vec<String>,
    status: &'static str,
    request: Option<Value>,
    attempts: Vec<protocol::Attempt>,
    raw: Option<Value>,
    channel_id: Option<String>,
    error: Option<&'static str>,
    reused: bool,
}
impl Opening {
    pub(super) fn ready(&self) -> bool {
        self.status == "ready"
    }
    pub(super) fn new(people: &[peer::Candidate]) -> Self {
        Self {
            token: next(),
            peer_ids: people.iter().map(|p| p.id().to_owned()).collect(),
            status: "resolving",
            request: None,
            attempts: vec![],
            raw: None,
            channel_id: None,
            error: None,
            reused: false,
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"peerIds":self.peer_ids,"status":self.status,"request":self.request,"attempts":self.attempts,"raw":self.raw,"channelId":self.channel_id,"error":self.error,"reused":self.reused})
    }
}
fn ledger<'a>(s: &'a mut ShellState, q: &Query, token: u64) -> Option<&'a mut Opening> {
    let v = owned_mut(s, q)?;
    if v.group_opening.as_ref().is_some_and(|o| o.token == token) {
        return v.group_opening.as_mut();
    }
    v.group_history.iter_mut().find(|o| o.token == token)
}
/// Actual complete membership is independent of a sparse later posting row.
#[derive(Clone)]
pub(super) struct Witness {
    membership: Value,
    canonical: Arc<canonical::Canonical>,
    peers: Vec<Arc<peer::Peer>>,
    self_id: String,
}
impl Witness {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.canonical.current(s) && self.peers.iter().all(|p| p.current(s))
    }
    pub(super) fn eligible(&self, s: &ShellState, raw: &Value, flags: Flags, team: &str) -> bool {
        self.current(s)
            && protocol::eligible_with_membership(
                raw,
                &self.membership,
                flags,
                team,
                &self.self_id,
                &self
                    .peers
                    .iter()
                    .map(|p| p.effective().clone())
                    .collect::<Vec<_>>(),
            )
    }
    pub(super) fn label(&self) -> String {
        self.peers
            .iter()
            .map(|p| p.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"membership":self.membership,"selfId":self.self_id,"peers":self.peers.iter().map(|p|p.evidence()).collect::<Vec<_>>()})
    }
    pub(super) fn rebase(
        &self,
        s: &ShellState,
        canonical: Arc<canonical::Canonical>,
    ) -> Option<Self> {
        if self.current(s) {
            return Some(self.clone());
        }
        let id = self.membership["id"].as_str()?;
        // A still-current paired sparse observation retains the captured
        // membership above. A NEW canonical source must itself retain exactly
        // that membership; do not attach an old response to newer sparse data.
        let row = canonical.row(id)?;
        if row.get("members")? != &self.membership["members"] {
            return None;
        }
        Some(Self {
            canonical,
            ..self.clone()
        })
    }
}
pub(super) async fn resolve(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        Query::capture(&s, visit, id).and_then(|q| {
            let v = attempt(&s, id)?.confirmation_share.as_ref()?;
            let o = v
                .group_opening
                .as_ref()
                .filter(|o| v.loading && o.status == "resolving")?;
            Some((q, v.recipients.clone(), o.token))
        })
    };
    let Some((q, people, token)) = captured else {
        return;
    };
    let result = acquire(state, &q, token, &people).await;
    let mut s = state.write();
    if !q.current(&s) {
        if let Some(o) = ledger(&mut s, &q, token) {
            o.status = "retired";
        }
        return;
    }
    let adopted = result.and_then(|choice| {
        let previous = attempt(&s, id)
            .and_then(|a| a.confirmation_share.as_ref())
            .and_then(|v| v.listing.as_ref());
        let channel = choice.id().to_owned();
        channels::Listing::adopt(previous, &s, &q.ctx().session.team_id, choice)
            .map(|l| (channel, l))
    });
    let v = owned_mut(&mut s, &q).unwrap();
    v.loading = false;
    v.bound = false;
    v.epoch = next();
    match adopted {
        Ok((id, listing)) => {
            v.selected = id;
            v.listing = Some(listing);
            v.notice = None;
            if let Some(o) = v.group_opening.as_mut() {
                o.status = "ready";
            }
        }
        Err(_) => {
            v.selected.clear();
            v.notice=Some("This group conversation could not be verified. No message was sent. Remove a person and choose again.".into());
            if let Some(o) = v.group_opening.as_mut() {
                o.status = "failed";
                o.error = Some("destination_unavailable");
            }
        }
    }
    drop(s);
    bind(state, visit, id).await;
}
async fn peers(
    state: Signal<ShellState>,
    q: &Query,
    people: &[peer::Candidate],
) -> Result<Vec<Arc<peer::Peer>>, Error> {
    let mut result = Vec::new();
    for candidate in people {
        if !q.current(&state.peek())
            || !people.iter().all(|p| p.current(&state.peek()))
            || !result
                .iter()
                .all(|p: &Arc<peer::Peer>| p.current(&state.peek()))
        {
            return Err(unavailable());
        }
        result.push(peer::acquire_person(state, q, candidate).await?);
    }
    Ok(result)
}
async fn acquire(
    mut state: Signal<ShellState>,
    q: &Query,
    token: u64,
    people: &[peer::Candidate],
) -> Result<channels::Choice, Error> {
    let peers = peers(state, q, people).await?;
    let ctx = q.ctx();
    let self_id = ctx.session.user_id.clone();
    let current = |s: &ShellState| q.current(s) && peers.iter().all(|p| p.current(s));
    if !current(&state.peek()) || !q.authorized(state).await {
        return Err(unavailable());
    }
    let source = canonical::observed(&state.peek(), &ctx.session.team_id)?;
    let raw_peers = peers
        .iter()
        .map(|p| p.effective().clone())
        .collect::<Vec<_>>();
    let mapped = source
        .rows()
        .iter()
        .filter(|raw| {
            raw["is_open"] == true
                && protocol::Open::new(&raw_peers, &ctx.session.team_id, &self_id, Some(raw))
                    .is_ok()
        })
        .collect::<Vec<_>>();
    if mapped.len() > 1 {
        return Err(unavailable());
    }
    let open = protocol::Open::new(
        &raw_peers,
        &ctx.session.team_id,
        &self_id,
        mapped.first().copied(),
    )?;
    let (channel, membership_source) = if let Some(existing) = open.reusable_channel() {
        if !source.current(&state.peek()) {
            return Err(unavailable());
        }
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.reused = true;
        }
        (
            existing["id"].as_str().ok_or_else(unavailable)?.to_owned(),
            source.clone(),
        )
    } else {
        let read = OpenRead::capture(state, &ctx.session.team_id, &current)?;
        let request = open.request(&ctx.client, &ctx.session)?;
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.status = "opening";
            o.request = Some(request.inspect());
        }
        let observed = state;
        let admitted = || {
            current(&observed.peek())
                && source.current(&observed.peek())
                && read.current(&observed.peek())
        };
        let mut observation_state = state;
        let mut observer = |a: protocol::Attempt| {
            if let Some(o) = ledger(&mut observation_state.write(), q, token) {
                o.attempts.push(a);
            }
        };
        let result = if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::share::multiple::open(
                state,
                &request,
                &admitted,
                &mut observer,
            )
            .await
            .unwrap_or_else(|| Err(unavailable()))
        } else {
            ctx.transport
                .execute_workflow_share_group_current(&request, &admitted, &mut observer)
                .await
        };
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.raw = result
                .as_ref()
                .ok()
                .cloned()
                .or_else(|| o.attempts.iter().rev().find_map(|a| a.raw.clone()));
        }
        if let Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
        }) = &result
        {
            if let Some(v) = owned_mut(&mut state.write(), q) {
                v.retry_after =
                    tokio::time::Instant::now().checked_add(Duration::from_secs(*seconds));
            }
        }
        if !admitted() || !q.authorized(state).await || !admitted() {
            return Err(unavailable());
        }
        let opened = open.decode(result?)?;
        let channel = opened.id().to_owned();
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.status = "publishing";
            o.channel_id = Some(channel.clone());
        }
        let published = read
            .publish(state, opened.channel().clone(), &current)
            .await?;
        (channel, published)
    };
    let membership = membership_source
        .row(&channel)
        .cloned()
        .ok_or_else(unavailable)?;
    protocol::Open::new(
        &raw_peers,
        &ctx.session.team_id,
        &self_id,
        Some(&membership),
    )?;
    if let Some(o) = ledger(&mut state.write(), q, token) {
        o.status = "validating";
        o.channel_id = Some(channel.clone());
    }
    refresh(
        state,
        q,
        &channel,
        peers,
        self_id,
        membership,
        membership_source,
    )
    .await
}
async fn refresh(
    state: Signal<ShellState>,
    q: &Query,
    channel: &str,
    peers: Vec<Arc<peer::Peer>>,
    self_id: String,
    membership: Value,
    source: Arc<canonical::Canonical>,
) -> Result<channels::Choice, Error> {
    let ctx = q.ctx();
    let current = |s: &ShellState| q.current(s) && peers.iter().all(|p| p.current(s));
    let read = PostingRead::capture(state, &ctx.session.team_id, channel, &current)?;
    let (raw, _) = q
        .read_guarded(
            state,
            api::conversations_info(&ctx.client, &ctx.session, channel.to_owned()),
            false,
            &|s| current(s) && source.current(s) && read.current(s),
        )
        .await?;
    if raw["ok"] != true
        || raw.get("error").is_some_and(|v| !v.is_null())
        || raw["channel"]["id"].as_str() != Some(channel)
        || !source.current(&state.peek())
    {
        return Err(unavailable());
    }
    // Type/membership/flags admission is performed by the final Choice; raw
    // response remains unmodified at the shared canonical/posting sink.
    let canonical = read
        .publish(state, raw["channel"].clone(), &current)
        .await?;
    if !current(&state.peek()) || !q.authorized(state).await {
        return Err(unavailable());
    }
    channels::group_choice(
        state,
        q,
        channel,
        Witness {
            membership,
            canonical,
            peers,
            self_id,
        },
    )
    .await
}
pub(super) async fn validate(
    state: Signal<ShellState>,
    q: &Query,
    id: &str,
) -> Result<channels::Choice, Error> {
    let captured = {
        let s = state.peek();
        let v = attempt(&s, q.id)
            .and_then(|a| a.confirmation_share.as_ref())
            .ok_or_else(unavailable)?;
        let choice = v
            .listing
            .as_ref()
            .and_then(|l| l.find(id))
            .filter(|c| c.current(&s))
            .ok_or_else(unavailable)?;
        let witness = choice.group().ok_or_else(unavailable)?.clone();
        (v.recipients.clone(), witness)
    };
    let (people, witness) = captured;
    let peers = peers(state, q, &people).await?;
    if !witness.current(&state.peek()) {
        return Err(unavailable());
    }
    refresh(
        state,
        q,
        id,
        peers,
        witness.self_id,
        witness.membership,
        witness.canonical,
    )
    .await
}
