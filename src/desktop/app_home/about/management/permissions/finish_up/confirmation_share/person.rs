//! Selecting a known person resolves the destination before message Send.
//! Open receipts never imply message delivery or fresh posting permission.
use super::*;
use crate::app_home::channel_fetches::{
    canonical,
    search::{direct::PostingRead, open::OpenRead},
};
use super_platinum_core::{slack::api, workflow_share::person as protocol};

pub(super) struct Opening {
    token: u64,
    peer_id: String,
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
    pub(super) fn new(peer: &str) -> Self {
        Self {
            token: next(),
            peer_id: peer.into(),
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
        json!({"peerId":self.peer_id,"status":self.status,"request":self.request,
            "attempts":self.attempts,"raw":self.raw,"channelId":self.channel_id,
            "error":self.error,"reused":self.reused})
    }
}
fn ledger<'a>(s: &'a mut ShellState, q: &Query, token: u64) -> Option<&'a mut Opening> {
    let v = owned_mut(s, q)?;
    if v.opening.as_ref().is_some_and(|o| o.token == token) {
        return v.opening.as_mut();
    }
    v.opening_history.iter_mut().find(|o| o.token == token)
}
fn status(mut state: Signal<ShellState>, q: &Query, token: u64, status: &'static str) {
    if let Some(o) = ledger(&mut state.write(), q, token) {
        o.status = status;
    }
}
pub(super) async fn resolve(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        Query::capture(&s, visit, id).and_then(|q| {
            let v = attempt(&s, id)?.confirmation_share.as_ref()?;
            let o = v
                .opening
                .as_ref()
                .filter(|o| v.loading && o.status == "resolving")?;
            let candidate = v
                .recipients
                .first()
                .filter(|_| v.recipients.len() == 1)?
                .clone();
            Some((q, candidate, o.token))
        })
    };
    let Some((q, candidate, token)) = captured else {
        return;
    };
    let result = acquire(state, &q, token, &candidate).await;
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
        Ok((channel, listing)) => {
            v.selected = channel;
            v.listing = Some(listing);
            v.notice = None;
            if let Some(o) = v.opening.as_mut() {
                o.status = "ready";
            }
        }
        Err(_) => {
            v.selected.clear();
            v.notice = Some("This conversation could not be verified. No message was sent. Remove the person and choose again.".into());
            if let Some(o) = v.opening.as_mut() {
                o.status = "failed";
                o.error = Some("destination_unavailable");
            }
        }
    }
    drop(s);
    bind(state, visit, id).await;
}
async fn acquire(
    mut state: Signal<ShellState>,
    q: &Query,
    token: u64,
    candidate: &peer::Candidate,
) -> Result<channels::Choice, Error> {
    let peer = peer::acquire_person(state, q, candidate).await?;
    let ctx = q.ctx();
    let current = |s: &ShellState| q.current(s) && peer.current(s);
    if !current(&state.peek()) || !q.authorized(state).await {
        return Err(unavailable());
    }
    let canonical = canonical::observed(&state.peek(), &ctx.session.team_id)?;
    let mapped = canonical
        .rows()
        .iter()
        .filter(|c| c["is_im"] == true && c["user"].as_str() == Some(candidate.id()))
        .collect::<Vec<_>>();
    if mapped.len() > 1 {
        return Err(unavailable());
    }
    let open = protocol::Open::new(
        peer.effective(),
        &ctx.session.team_id,
        mapped.first().copied(),
    )?;
    let channel = if let Some(existing) = open.reusable_channel() {
        if !canonical.current(&state.peek()) {
            return Err(unavailable());
        }
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.reused = true;
        }
        existing["id"].as_str().ok_or_else(unavailable)?.to_owned()
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
                && canonical.current(&observed.peek())
                && read.current(&observed.peek())
        };
        let mut observation_state = state;
        let mut observer = |attempt: protocol::Attempt| {
            if let Some(o) = ledger(&mut observation_state.write(), q, token) {
                o.attempts.push(attempt);
            }
        };
        let result = if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::share::person::open(
                state,
                &request,
                &admitted,
                &mut observer,
            )
            .await
            .unwrap_or_else(|| Err(unavailable()))
        } else {
            ctx.transport
                .execute_workflow_share_person_current(&request, &admitted, &mut observer)
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
        if let Some(o) = ledger(&mut state.write(), q, token) {
            o.channel_id = Some(opened.id().into());
        }
        status(state, q, token, "publishing");
        read.publish(state, opened.channel().clone(), &current)
            .await?;
        opened.id().to_owned()
    };
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    if let Some(o) = ledger(&mut state.write(), q, token) {
        o.channel_id = Some(channel.clone());
    }
    status(state, q, token, "validating");
    let read = PostingRead::capture(state, &ctx.session.team_id, &channel, &current)?;
    let request = api::conversations_info(&ctx.client, &ctx.session, channel.clone());
    let (response, _) = q
        .read_guarded(state, request, false, &|s| current(s) && read.current(s))
        .await?;
    // Correlate this actual info response to the same peer and channel too.
    let checked =
        protocol::Open::new(peer.effective(), &ctx.session.team_id, None)?.decode(response)?;
    if checked.id() != channel {
        return Err(unavailable());
    }
    read.publish(state, checked.channel().clone(), &current)
        .await?;
    if !current(&state.peek()) || !q.authorized(state).await {
        return Err(unavailable());
    }
    channels::opened_choice(state, q, &channel, peer).await
}
