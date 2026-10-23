//! Feature owns an independent outcome after the original bookmark ACK. Native
//! modal retirement caused by Add navigation is not account/source retirement.
use super::*;
use crate::app_home::channel_fetches::{context::Context, search::direct};
use super_platinum_core::channel_workflows::feature::{Attempt, acknowledged};

pub(super) struct Operation {
    generation: u64,
    request: Option<Value>,
    attempts: Vec<Attempt>,
    outcome: &'static str,
    raw: Option<Value>,
    readback: Option<Value>,
    readback_status: &'static str,
}
impl Operation {
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"request":self.request,"attempts":self.attempts,
            "outcome":self.outcome,"raw":self.raw,"readback":self.readback,"readbackStatus":self.readback_status})
    }
}
fn operation<'a>(s: &'a mut ShellState, q: &Query) -> Option<&'a mut Operation> {
    attempt_mut(s, q.id)?
        .confirmation_add
        .as_mut()?
        .feature
        .operation
        .as_mut()
        .filter(|op| op.generation == q.generation)
}
pub(in super::super) async fn prepare(
    mut state: Signal<ShellState>,
    q: &Query,
    choice: &channels::Choice,
) -> Option<Add> {
    if q.expected.pointer("/feature/checked") != Some(&Value::Bool(true)) {
        return None;
    }
    {
        let mut s = state.write();
        if !q.current(&s) {
            return None;
        }
        attempt_mut(&mut s, q.id)?
            .confirmation_add
            .as_mut()?
            .feature
            .operation = Some(Operation {
            generation: q.generation,
            request: None,
            attempts: vec![],
            outcome: "preflight",
            raw: None,
            readback: None,
            readback_status: "not-started",
        });
    }
    match acquire(state, q, choice).await {
        Ok(add) => Some(add),
        Err(_) => {
            if let Some(op) = operation(&mut state.write(), q) {
                op.outcome = "not-sent";
            }
            crate::channel_workflows::feature_notice(
                &mut state.write(),
                &q.ctx().session.team_id,
                choice.id(),
                "Featuring could not be verified; no Feature request was sent.",
            );
            None
        }
    }
}
pub(in super::super) fn start(
    mut state: Signal<ShellState>,
    q: &Query,
    choice: channels::Choice,
    add: Add,
) {
    if !q.current(&state.peek()) || !choice.current(&state.peek()) {
        return;
    }
    let Ok(ctx) = Context::capture(&state.peek(), &q.ctx().session.team_id) else {
        return;
    };
    let Ok(request) = add.request(&ctx.client, &ctx.session) else {
        return;
    };
    let q = q.clone();
    if let Some(op) = operation(&mut state.write(), &q) {
        op.request = Some(request.inspect());
        op.outcome = "in-flight";
    } else {
        return;
    }
    crate::channel_workflows::feature_notice(
        &mut state.write(),
        &ctx.session.team_id,
        choice.id(),
        "Featuring workflow\u{2026}",
    );
    dioxus::core::spawn_forever(async move {
        let owner = |s: &ShellState| {
            ctx.current(s)
                && attempt(s, q.id)
                    .and_then(|a| a.confirmation_add.as_ref())
                    .and_then(|a| a.feature.operation.as_ref())
                    .is_some_and(|op| op.generation == q.generation)
        };
        let current = |s: &ShellState| owner(s) && choice.current(s);
        let mut observer_state = state;
        let mut observe = |value: Attempt| {
            if let Some(op) = operation(&mut observer_state.write(), &q) {
                op.attempts.push(value);
            }
        };
        let result = if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::feature::mutation(state, &request, &mut observe)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else {
            ctx.transport
                .execute_workflow_feature_current(&request, || current(&state.peek()), &mut observe)
                .await
        };
        let accepted = result.as_ref().is_ok_and(acknowledged);
        if let Some(op) = operation(&mut state.write(), &q) {
            op.outcome = match &result {
                Ok(_) if accepted => "acknowledged",
                Ok(raw) if raw["ok"] == false => "rejected",
                Err(Error::Offline(_)) => "not-sent",
                Err(Error::Api(code)) if code.starts_with("workflow_feature_not_sent_") => {
                    "not-sent"
                }
                _ => match op.attempts.last().map(|a| a.outcome) {
                    Some(super_platinum_core::channel_workflows::feature::Outcome::Rejected) => {
                        "rejected"
                    }
                    Some(super_platinum_core::channel_workflows::feature::Outcome::RateLimited) => {
                        "rate-limited"
                    }
                    _ => "unknown",
                },
            };
            op.raw = result.ok();
        }
        if !accepted {
            if ctx.current(&state.peek()) {
                crate::channel_workflows::feature_notice(
                    &mut state.write(),
                    &ctx.session.team_id,
                    choice.id(),
                    "Workflow added. Featuring could not be confirmed.",
                );
            }
            return;
        }
        // Feature dispatch does not delay Add navigation. Our own metadata
        // publication must also not invalidate the guard while it navigates.
        let settled = tokio::time::timeout(Duration::from_secs(15), async {
            while ctx.current(&state.peek())
                && attempt(&state.peek(), q.id).is_some_and(|a| !a.closed)
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .is_ok();
        if !settled {
            if let Some(op) = operation(&mut state.write(), &q) {
                op.readback_status = "unavailable";
            }
            return;
        }
        if !owner(&state.peek()) {
            if let Some(op) = operation(&mut state.write(), &q) {
                op.readback_status = "retired";
            }
            return;
        }
        // Capture the ordered shared source before the actual read. A concurrent
        // socket/update retires this read instead of overwriting newer properties.
        // A readback is a fresh read, not another mutation using the earlier
        // posting grant. Its own direct receipt guards the current source.
        let readback = readback(state, &q, &ctx, &add, &owner).await;
        if let Some(op) = operation(&mut state.write(), &q) {
            op.readback_status = match &readback {
                Ok(true) => "confirmed",
                Ok(false) => "absent",
                Err(_) => "unavailable",
            };
        }
        // Shared publication intentionally changes choice's original source.
        // Only the exact account and destination can receive user feedback now.
        if ctx.current(&state.peek()) {
            let mut s = state.write();
            if let Some(channel) = add.channel()["id"].as_str() {
                crate::channel_workflows::feature_notice(
                    &mut s,
                    &ctx.session.team_id,
                    channel,
                    match readback {
                        Ok(true) => "Workflow featured in this channel.",
                        Ok(false) => {
                            "Workflow added. Featuring was acknowledged but is not yet visible in channel details."
                        }
                        Err(_) => {
                            "Workflow added. Featuring was acknowledged, but channel details could not be verified."
                        }
                    },
                );
            }
        }
    });
}
async fn readback(
    mut state: Signal<ShellState>,
    q: &Query,
    ctx: &Context,
    add: &Add,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<bool, Error> {
    let channel = add.channel()["id"].as_str().ok_or_else(unavailable)?;
    let read = direct::Read::capture(state, &ctx.session.team_id, channel, current)?;
    let request = api::conversations_info(&ctx.client, &ctx.session, channel.into());
    let response = if crate::fixture::is_fixture() {
        fixture::first_publish::confirmation::feature::read(state, &request)
            .await
            .unwrap_or_else(|| Err(unavailable()))?
    } else {
        ctx.transport.execute(request).await?
    };
    if !current(&state.peek())
        || !read.current(&state.peek())
        || response["ok"] != true
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(unavailable());
    }
    let observation = add.observe_channel(response.get("channel").ok_or_else(unavailable)?)?;
    if let Some(op) = operation(&mut state.write(), q) {
        op.readback = Some(response);
    }
    // After ingestion choice.current naturally retires. Account/receipt ownership
    // alone is appropriate for waiting for that already accepted shared publish.
    let canonical = read
        .publish(state, observation.raw().clone(), |s| ctx.current(s))
        .await?;
    if !canonical.current(&state.peek()) {
        return Err(unavailable());
    }
    crate::channel_workflows::metadata_published(&mut state.write(), &ctx.session.team_id, channel);
    Ok(observation.present())
}
