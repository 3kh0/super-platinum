//! Independent changed settings groups; an ACK never comes from readback.
use super::*;

pub(super) struct Run {
    pub(super) plan: MutationPlan,
    pub(super) reply: Option<Reply>,
    pub(super) followup: Option<protocol::FollowUpPlan>,
    pub(super) follow_reply: Option<Reply>,
}
impl Run {
    pub(super) fn snapshot(&self) -> Value {
        let phase = |o: &Outcome| match o {
            Outcome::Pending => "pending",
            Outcome::InFlight => "in-flight",
            Outcome::NotSent => "not-sent",
            Outcome::Succeeded(_) => "acknowledged",
            Outcome::Rejected(_) => "rejected",
            Outcome::Unknown => "unknown",
        };
        let receipt = |slot: &protocol::Slot, reply: Option<&Reply>| json!({"method":slot.plan().method(),"arguments":slot.plan().arguments(),"outcome":phase(slot.outcome()),"raw":match reply{Some(Reply::Raw(v))=>Some(v),_=>None},"failure":match reply{Some(Reply::Failure(v))=>Some(*v),_=>None}});
        json!({"primary":receipt(&self.plan.slots()[0],self.reply.as_ref()),"complete":self.complete(),
            "followup":self.followup.as_ref().map(|f|json!({"phase":format!("{:?}",f.phase()),"fullySucceeded":f.fully_succeeded(),"restriction":f.restriction(),
                "slots":f.stages().iter().flat_map(|s|s.slots()).map(|s|receipt(s,self.follow_reply.as_ref())).collect::<Vec<_>>()}))})
    }
    pub(super) fn new(editor: &Editor, command: protocol::Command) -> Result<Self, Error> {
        Ok(Self {
            plan: MutationPlan::new(editor, command)?,
            reply: None,
            followup: None,
            follow_reply: None,
        })
    }
    pub(super) fn complete(&self) -> bool {
        self.plan.primary_succeeded() && self.followup.as_ref().is_some_and(|p| p.fully_succeeded())
    }
    pub(super) fn no_send(&self) -> bool {
        self.plan
            .slots()
            .iter()
            .all(|s| matches!(s.outcome(), Outcome::Pending | Outcome::NotSent))
    }
    pub(super) fn restriction(&self) -> Option<&Value> {
        self.followup.as_ref().and_then(|p| p.restriction())
    }
}

pub(super) async fn execute(
    mut state: Signal<ShellState>,
    id: u64,
    original: &Original,
    lease: &read::Lease,
) {
    if !run::attempt(&state.peek(), id).is_some_and(|a| a.run_group.is_some()) {
        return;
    }
    for primary in [true, false] {
        if !managers::dispatch_native(state, id, original).await {
            return;
        }
        let request = {
            let mut s = state.write();
            if !managers::dispatch_current(&s, id) {
                return;
            }
            let Some(group) = run::attempt_mut(&mut s, id).and_then(|a| a.run_group.as_mut())
            else {
                return;
            };
            let plan = if primary {
                if group.plan.mark_sent(0).is_err() {
                    return;
                }
                group.plan.slots()[0].plan()
            } else {
                let Some(follow) = group.followup.as_mut() else {
                    return;
                };
                if follow.mark_sent(0).is_err() {
                    return;
                }
                follow.slots()[0].plan()
            };
            plan.request(&lease.owner.ctx.client, &lease.owner.ctx.session)
        };
        let result = if fixture::enabled(&state.peek()) {
            match fixture::managers::mutation(state, &request).await {
                Some(result) => result,
                None => fixture::run_access::mutation(state, &request)
                    .await
                    .unwrap_or_else(|| {
                        Err(Error::Api("workflow_management_not_sent_fixture".into()))
                    }),
            }
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else if primary {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_run_permission(request)
                .await
        } else {
            lease.owner.ctx.transport.execute(request).await
        };
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        settle(&mut s, id, primary, result);
    }
}
pub(super) fn settle(s: &mut ShellState, id: u64, primary: bool, result: Result<Value, Error>) {
    let Some(group) = run::attempt_mut(s, id).and_then(|a| a.run_group.as_mut()) else {
        return;
    };
    let in_flight = if primary {
        group.reply.is_none() && matches!(group.plan.slots()[0].outcome(), Outcome::InFlight)
    } else {
        group.follow_reply.is_none()
            && group.followup.as_ref().is_some_and(|p| {
                p.slots()
                    .first()
                    .is_some_and(|s| matches!(s.outcome(), Outcome::InFlight))
            })
    };
    if !in_flight {
        return;
    }
    let (outcome, reply) = match result {
        Ok(raw) => {
            let valid = raw.is_object()
                && raw["ok"] == true
                && raw.get("error").is_none_or(Value::is_null)
                && (!primary
                    || protocol::run_settings::acknowledges(group.plan.slots()[0].plan(), &raw));
            let outcome = if valid {
                Outcome::Succeeded(raw.clone())
            } else if raw.is_object() && raw["ok"] == false {
                Outcome::Rejected(raw.clone())
            } else {
                Outcome::Unknown
            };
            (outcome, Reply::Raw(raw))
        }
        Err(Error::Offline(_)) => (Outcome::NotSent, Reply::Failure("offline before dispatch")),
        Err(Error::Api(code)) if code.starts_with("workflow_management_not_sent_") => {
            (Outcome::NotSent, Reply::Failure("refused before dispatch"))
        }
        Err(_) => (Outcome::Unknown, Reply::Failure("delivery unconfirmed")),
    };
    if primary {
        if group.plan.settle(0, outcome).is_err() {
            let _ = group.plan.settle(0, Outcome::Unknown);
        }
        group.reply = Some(reply);
        if group.plan.primary_succeeded() {
            group.followup = protocol::FollowUpPlan::new(&group.plan).ok();
        }
    } else if let Some(follow) = group.followup.as_mut() {
        if follow.settle(0, outcome).is_err() {
            let _ = follow.settle(0, Outcome::Unknown);
        }
        group.follow_reply = Some(reply);
        let _ = follow.advance();
    }
}
