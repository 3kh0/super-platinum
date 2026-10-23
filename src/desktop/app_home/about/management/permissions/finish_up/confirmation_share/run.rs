//! Destination/preparation errors remain in Share. Message-queue outcomes own
//! the optimistic row independently of whether the original modal still exists.
use super::*;
use crate::app_home::channel_fetches::context::Context;
use super_platinum_core::workflow_share::{
    Attempt as SendAttempt, Correlation, Message, Outcome as SendOutcome,
};

pub(super) struct Operation {
    message: Option<Message>,
    owner: Option<Arc<Context>>,
    locally_cancelled: bool,
    request: Option<Value>,
    attempts: Vec<SendAttempt>,
    raw: Option<Value>,
    outcome: &'static str,
    finished: bool,
    queued: bool,
    pub(super) resends: Vec<resend::Resend>,
    pub(super) cooldown: resend::Cooldown,
    pub(super) posting_refresh: Vec<posting::Refresh>,
}
impl Operation {
    pub(super) fn cancellable(
        &self,
    ) -> Option<(&Arc<Context>, &Message, crate::state::PendingDeliveryStatus)> {
        use crate::state::PendingDeliveryStatus;
        if !self.finished || !self.queued || self.locally_cancelled {
            return None;
        }
        let (finished, outcome) = self
            .resends
            .last()
            .map_or((self.finished, self.outcome), |r| (r.finished, r.outcome));
        if !finished {
            return None;
        }
        let status = match outcome {
            "rejected" => PendingDeliveryStatus::Failed,
            "unknown" => PendingDeliveryStatus::Unconfirmed,
            _ => return None,
        };
        Some((self.owner.as_ref()?, self.message.as_ref()?, status))
    }
    pub(super) fn generation(&self) -> u64 {
        self.resends.last().map_or(0, |r| r.generation)
    }
    pub(super) fn failure(&self) -> Option<&Value> {
        self.resends
            .last()
            .map_or(self.raw.as_ref(), |r| r.raw.as_ref())
    }
    pub(super) fn owns(&self, owner: &Arc<Context>, message: &Message) -> bool {
        self.queued
            && !self.locally_cancelled
            && self.owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, owner))
            && self.message.as_ref() == Some(message)
    }
    pub(super) fn mark_locally_cancelled(&mut self) {
        self.locally_cancelled = true;
    }

    pub(super) fn snapshot(&self) -> Value {
        json!({"message":self.message.as_ref().map(|m|json!({"channel":m.channel(),"clientMsgId":m.client_msg_id(),"localTs":m.local_ts(),"blocks":m.blocks(),"url":m.url(),"text":m.text_fallback()})),
            "request":self.request,"attempts":self.attempts,"raw":self.raw,"outcome":self.outcome,"finished":self.finished,"queued":self.queued,"locallyCancelled":self.locally_cancelled,"resends":self.resends.iter().map(resend::Resend::snapshot).collect::<Vec<_>>(),"retryBlocked":self.cooldown.blocked(),"postingRefresh":self.posting_refresh.iter().map(posting::Refresh::snapshot).collect::<Vec<_>>()})
    }
}
fn operation<'a>(s: &'a mut ShellState, q: &Query) -> Option<&'a mut Operation> {
    owned_mut(s, q)?.operation.as_mut()
}
pub(super) fn send(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let q = Query::capture(&state.peek(), visit, id);
    let Some(q) = q else { return };
    dioxus::core::spawn_forever(async move {
        if !q.current(&state.peek()) || !native::current(&q, Some(&json!({"kind":"send"}))).await {
            return;
        }
        {
            let mut s = state.write();
            if !q.current(&s) {
                return;
            }
            let v = owned_mut(&mut s, &q).unwrap();
            if v.operation.is_some() || v.loading {
                return;
            }
            v.operation = Some(Operation {
                message: None,
                owner: None,
                locally_cancelled: false,
                request: None,
                attempts: vec![],
                raw: None,
                outcome: "preflight",
                finished: false,
                queued: false,
                resends: vec![],
                cooldown: resend::Cooldown::default(),
                posting_refresh: vec![],
            });
        }
        let selected = q.expected["selected"].as_str().unwrap_or_default();
        let is_group = attempt(&state.peek(), id)
            .and_then(|a| a.confirmation_share.as_ref())
            .is_some_and(|v| v.recipients.len() > 1);
        let checked = if is_group {
            group::validate(state, &q, selected).await
        } else {
            channels::validate(state, &q, selected).await
        };
        let choice = match checked {
            Ok(choice) => choice,
            Err(_) => {
                preflight_failed(state, &q);
                return;
            }
        };
        if !q.authorized(state).await || !choice.current(&state.peek()) {
            preflight_failed(state, &q);
            return;
        }
        let prepared = {
            let s = state.peek();
            if !q.current(&s) || !choice.current(&s) {
                None
            } else {
                let a = attempt(&s, id);
                let trigger = a
                    .and_then(|a| a.followup.as_ref())
                    .and_then(|f| f.current_trigger());
                let (_, ts) = ShellState::optimistic_identity("workflow-share");
                let client_id = uuid::Uuid::new_v4().to_string();
                trigger
                    .and_then(|t| {
                        Message::new(
                            t,
                            q.expected["text"].as_str().unwrap_or_default(),
                            selected,
                            &client_id,
                            &ts,
                        )
                        .ok()
                    })
                    .and_then(|message| {
                        Context::capture(&s, &q.ctx().session.team_id)
                            .ok()
                            .map(|ctx| (message, Arc::new(ctx)))
                    })
            }
        };
        let Some((message, ctx)) = prepared else {
            preflight_failed(state, &q);
            return;
        };
        let request = match message.request(&ctx.client, &ctx.session) {
            Ok(request) => request,
            Err(_) => {
                preflight_failed(state, &q);
                return;
            }
        };
        {
            let mut s = state.write();
            if !q.current(&s) || !choice.current(&s) || !ctx.current(&s) {
                return;
            }
            let Some(workspace) = s.core.workspaces.get_mut(&ctx.session.team_id) else {
                return;
            };
            let row = optimistic(&message, &workspace.self_user_id);
            let messages = workspace
                .messages
                .entry(message.channel().to_owned())
                .or_default();
            messages.upsert(row);
            messages.pending.push(message.local_ts().into());
            let op = operation(&mut s, &q).unwrap();
            op.message = Some(message.clone());
            op.owner = Some(ctx.clone());
            op.request = Some(request.inspect());
            op.outcome = "sending";
            op.queued = true;
            s.refresh_from_core();
        }
        let mut observation_state = state;
        let mut observer = |value: SendAttempt| {
            if let Some(op) = operation(&mut observation_state.write(), &q) {
                op.cooldown.observe(&value);
                op.attempts.push(value)
            }
        };
        let result = if crate::fixture::is_fixture() {
            fixture::first_publish::confirmation::share::mutation(state, &request, &mut observer)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else {
            ctx.transport
                .execute_workflow_share_current(
                    &request,
                    || ctx.current(&state.peek()) && choice.current(&state.peek()),
                    &mut observer,
                )
                .await
        };
        // Queue failure is consumed into the retained message row. Cancel or
        // document retirement never redirects its response to a successor pane.
        let raw = result.as_ref().ok().cloned().or_else(|| {
            operation(&mut state.write(), &q)
                .and_then(|op| op.attempts.last())
                .and_then(|a| a.raw.clone())
        });
        let correlation = raw
            .as_ref()
            .map(|raw| message.classify(raw.clone()).correlation())
            .unwrap_or_else(|| {
                let mut s = state.write();
                match operation(&mut s, &q)
                    .and_then(|op| op.attempts.last())
                    .map(|a| a.outcome)
                {
                    Some(SendOutcome::Rejected | SendOutcome::RateLimited) => Correlation::Rejected,
                    _ => Correlation::Unknown,
                }
            });
        {
            let mut s = state.write();
            if let Some(op) = operation(&mut s, &q) {
                op.raw = raw.clone();
                op.finished = true;
                op.outcome = match correlation {
                    Correlation::Acknowledged => "acknowledged",
                    Correlation::Rejected => "rejected",
                    Correlation::Unknown => "unknown",
                };
            }
            if ctx.account_current(&s) {
                apply(
                    &mut s,
                    &ctx.session.team_id,
                    &message,
                    correlation,
                    raw.as_ref(),
                );
            }
        }
        posting::start(state, id, 0, ctx.clone(), message.channel(), raw.as_ref());
        resend::wake_after_cooldown(state, id, 0);
        if q.current(&state.peek()) && native::current(&q, None).await && q.current(&state.peek()) {
            close_current(&mut state.write(), &q);
        }
        if ctx.account_current(&state.peek())
            && correlation == Correlation::Acknowledged
            && !crate::fixture::is_fixture()
        {
            crate::bootstrap::persist_workspace(&state, &ctx.session.team_id);
        }
    });
}
fn preflight_failed(mut state: Signal<ShellState>, q: &Query) {
    let mut s = state.write();
    if !q.current(&s) {
        return;
    }
    if let Some(v) = owned_mut(&mut s, q) {
        // Nothing was queued/dispatched; preserving the editor permits a fresh
        // user action after recovering the actual destination facts.
        v.operation = None;
        v.notice = Some("Sorry, something went wrong. Try again in a moment.".into());
    }
}
pub(super) fn optimistic(
    message: &Message,
    user: &str,
) -> super_platinum_core::slack::models::Message {
    super_platinum_core::slack::models::Message {
        kind: Some("message".into()),
        user: Some(user.into()),
        channel: Some(message.channel().into()),
        ts: Some(message.local_ts().into()),
        client_msg_id: Some(message.client_msg_id().into()),
        text: Some(message.text_fallback().into()),
        blocks: message.blocks().as_array().cloned().unwrap_or_default(),
        ..Default::default()
    }
}
pub(super) fn apply(
    s: &mut ShellState,
    team: &str,
    message: &Message,
    correlation: Correlation,
    raw: Option<&Value>,
) {
    use crate::state::PendingDeliveryStatus;
    match correlation {
        Correlation::Acknowledged => {
            let Some(ts) = raw.and_then(|v| v["ts"].as_str()) else {
                return;
            };
            if let Some(workspace) = s.core.workspaces.get_mut(team) {
                // Preserve the actually sent local content when an ACK omits
                // fields; observed server fields win. This is optimistic row
                // reconciliation, not a claimed conversations.history readback.
                let mut merged =
                    serde_json::to_value(optimistic(message, &workspace.self_user_id)).unwrap();
                if let Some(server) = raw
                    .and_then(|v| v.get("message"))
                    .and_then(Value::as_object)
                {
                    merged.as_object_mut().unwrap().extend(server.clone());
                }
                merged["ts"] = json!(ts);
                merged["channel"] = json!(message.channel());
                if let Ok(row) = serde_json::from_value(merged) {
                    workspace
                        .messages
                        .entry(message.channel().into())
                        .or_default()
                        .confirm(message.client_msg_id(), row);
                    s.pending_delivery.remove(message.local_ts());
                }
            }
        }
        Correlation::Rejected => {
            s.pending_delivery
                .insert(message.local_ts().into(), PendingDeliveryStatus::Failed);
        }
        Correlation::Unknown => {
            s.pending_delivery.insert(
                message.local_ts().into(),
                PendingDeliveryStatus::Unconfirmed,
            );
        }
    }
    s.refresh_from_core();
}
