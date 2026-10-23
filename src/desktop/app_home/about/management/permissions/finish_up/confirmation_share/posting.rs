//! A restricted send refreshes channel facts independently of the failed row.
//! Closing Share, navigating or cancelling that local row does not cancel this
//! read. Original account/socket and ordered publication ownership still apply.
use super::*;
use crate::app_home::channel_fetches::{context::Context, search::direct::PostingRead};
use super_platinum_core::workflow_share::posting_refresh as protocol;

pub(super) struct Refresh {
    generation: u64,
    request: Value,
    raw: Option<Value>,
    outcome: &'static str,
    finished: bool,
    applied: bool,
}
impl Refresh {
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"request":self.request,"raw":self.raw,
            "outcome":self.outcome,"finished":self.finished,"applied":self.applied})
    }
}
fn operation(s: &mut ShellState, id: u64) -> Option<&mut run::Operation> {
    attempt_mut(s, id)?
        .confirmation_share
        .as_mut()?
        .operation
        .as_mut()
}
fn receipt(s: &mut ShellState, id: u64, generation: u64) -> Option<&mut Refresh> {
    operation(s, id)?
        .posting_refresh
        .iter_mut()
        .find(|r| r.generation == generation)
}
fn finish(mut state: Signal<ShellState>, id: u64, generation: u64, outcome: &'static str) {
    if let Some(r) = receipt(&mut state.write(), id, generation) {
        r.outcome = outcome;
        r.finished = true;
        r.applied = outcome == "applied";
    }
}

pub(super) fn start(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    owner: Arc<Context>,
    channel: &str,
    failure: Option<&Value>,
) {
    if !protocol::required(failure) || !owner.current(&state.peek()) {
        return;
    }
    let Ok(request) = protocol::request(&owner.client, &owner.session, channel) else {
        return;
    };
    {
        let mut s = state.write();
        let Some(op) = operation(&mut s, id) else {
            return;
        };
        // One background read per terminal user attempt; subsequent local
        // actions never rewrite the original failure or replay this read.
        if op
            .posting_refresh
            .iter()
            .any(|r| r.generation == generation)
        {
            return;
        }
        op.posting_refresh.push(Refresh {
            generation,
            request: json!({"method":"conversations.info","channel":channel,
                "_x_reason":"posting_permission_send_message_error"}),
            raw: None,
            outcome: "reading",
            finished: false,
            applied: false,
        });
    }
    // Both canonical and posting reads reserve their original order before
    // dispatch. The publisher pairs them with the same actual raw response.
    let read = match PostingRead::capture(state, &owner.session.team_id, channel, &|s| {
        owner.current(s)
    }) {
        Ok(read) => read,
        Err(_) => {
            finish(state, id, generation, "unavailable");
            return;
        }
    };
    let channel = channel.to_owned();
    dioxus::core::spawn_forever(async move {
        if !owner.current(&state.peek()) || !read.current(&state.peek()) {
            finish(state, id, generation, "retired");
            return;
        }
        let result = if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::share::posting::read(state, &request)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else {
            owner
                .transport
                .execute_workflow_posting_refresh_current(request, || {
                    owner.current(&state.peek()) && read.current(&state.peek())
                })
                .await
        };
        // Preserve actual response evidence even when its publication retired.
        if let Some(r) = receipt(&mut state.write(), id, generation) {
            r.raw = result.as_ref().ok().cloned();
        }
        if !owner.current(&state.peek()) || !read.current(&state.peek()) {
            finish(state, id, generation, "retired");
            return;
        }
        let raw = match result {
            Ok(raw) => raw,
            Err(_) => {
                finish(state, id, generation, "failed");
                return;
            }
        };
        let channel = match protocol::channel(&raw, &channel) {
            Ok(channel) => channel,
            Err(_) => {
                finish(state, id, generation, "invalid");
                return;
            }
        };
        if let Some(r) = receipt(&mut state.write(), id, generation) {
            r.outcome = "publishing";
        }
        let published = read.publish(state, channel, |s| owner.current(s)).await;
        finish(
            state,
            id,
            generation,
            if published.is_ok() {
                "applied"
            } else {
                "retired"
            },
        );
    });
}
