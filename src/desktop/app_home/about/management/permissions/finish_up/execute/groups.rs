use super::*;
pub(super) async fn metadata(mut state: Signal<ShellState>, id: u64, index: usize) -> bool {
    if !authorized(state, id).await {
        return false;
    }
    let plan = {
        let mut s = state.write();
        let Some(a) = attempt_mut(&mut s, id) else {
            return false;
        };
        let Some(p) = a.metadata.as_mut() else {
            return true;
        };
        if p.mark_sent(index).is_err() {
            return false;
        }
        p.slots()[index].plan().clone()
    };
    let (mut outcome, reply) = classify(send(state, id, &plan, "metadata").await);
    let mut s = state.write();
    let Some(a) = attempt_mut(&mut s, id) else {
        return false;
    };
    if let Outcome::Succeeded(raw) = &outcome {
        if if index == 0 {
            workflow_ack(&a.original, raw)
        } else {
            trigger_ack(&a.original, raw)
        } {
            a.acknowledged.push((
                if index == 0 {
                    protocol::ReadKind::Latest
                } else {
                    protocol::ReadKind::TriggerInfo
                },
                raw.clone(),
            ));
        } else {
            outcome = Outcome::Unknown;
        }
    }
    let p = a.metadata.as_mut().unwrap();
    if p.settle(index, outcome).is_err() {
        let _ = p.settle(index, Outcome::Unknown);
    }
    a.metadata_replies[index] = Some(reply);
    matches!(p.slots()[index].outcome(), Outcome::Succeeded(_))
}
pub(super) async fn copy(mut state: Signal<ShellState>, id: u64) -> bool {
    if attempt(&state.peek(), id).is_some_and(|a| a.copy.is_none()) {
        return true;
    }
    if !authorized(state, id).await {
        return false;
    }
    let plan = {
        let mut s = state.write();
        let Some(p) = attempt_mut(&mut s, id).and_then(|a| a.copy.as_mut()) else {
            return false;
        };
        if p.mark_sent(0).is_err() {
            return false;
        }
        p.slots()[0].plan().clone()
    };
    let (mut outcome, reply) = classify(send(state, id, &plan, "copy").await);
    if let Outcome::Succeeded(raw) = &outcome {
        if raw["permission_type"] != plan.arguments()["permission_type"] {
            outcome = Outcome::Unknown
        }
    }
    let mut s = state.write();
    let Some(a) = attempt_mut(&mut s, id) else {
        return false;
    };
    if let Outcome::Succeeded(raw) = &outcome {
        a.acknowledged
            .push((protocol::ReadKind::CopyPermissions, raw.clone()));
    }
    let p = a.copy.as_mut().unwrap();
    if p.settle(0, outcome).is_err() {
        let _ = p.settle(0, Outcome::Unknown);
    }
    a.copy_reply = Some(reply);
    p.primary_succeeded()
}
pub(super) async fn run_access(mut state: Signal<ShellState>, id: u64) -> bool {
    if attempt(&state.peek(), id).is_some_and(|a| a.run.is_none()) {
        return true;
    }
    for primary in [true, false] {
        if !authorized(state, id).await {
            return false;
        }
        let plan = {
            let mut s = state.write();
            let Some(g) = attempt_mut(&mut s, id).and_then(|a| a.run.as_mut()) else {
                return false;
            };
            if primary {
                if g.plan.mark_sent(0).is_err() {
                    return false;
                }
                g.plan.slots()[0].plan().clone()
            } else {
                let Some(f) = g.followup.as_mut() else {
                    return false;
                };
                if f.phase() == protocol::FollowUpPhase::Complete {
                    break;
                }
                if f.mark_sent(0).is_err() {
                    return false;
                }
                f.slots()[0].plan().clone()
            }
        };
        let (mut outcome, reply) = classify(
            send(
                state,
                id,
                &plan,
                if primary { "run" } else { "restriction" },
            )
            .await,
        );
        if primary {
            if let Outcome::Succeeded(raw) = &outcome {
                if !protocol::run_settings::acknowledges(&plan, raw) {
                    outcome = Outcome::Unknown
                }
            }
        }
        if !primary {
            if let Outcome::Succeeded(raw) = &outcome {
                if !raw["is_restricted"].is_boolean()
                    || !raw["restricted_by_connector_steps_only"].is_boolean()
                {
                    outcome = Outcome::Unknown;
                }
            }
        }
        let mut s = state.write();
        let Some(a) = attempt_mut(&mut s, id) else {
            return false;
        };
        if let Outcome::Succeeded(raw) = &outcome {
            a.acknowledged.push((
                if primary {
                    protocol::ReadKind::RunPermissions
                } else {
                    protocol::ReadKind::Restriction
                },
                raw.clone(),
            ));
        }
        let Some(g) = a.run.as_mut() else {
            return false;
        };
        if primary {
            if g.plan.settle(0, outcome).is_err() {
                let _ = g.plan.settle(0, Outcome::Unknown);
            }
            g.reply = Some(reply);
            if !g.plan.primary_succeeded() {
                return false;
            }
            g.followup = protocol::FollowUpPlan::new(&g.plan).ok();
        } else {
            let f = g.followup.as_mut().unwrap();
            if f.settle(0, outcome).is_err() {
                let _ = f.settle(0, Outcome::Unknown);
            }
            g.follow_reply = Some(reply);
            let _ = f.advance();
        }
    }
    attempt(&state.peek(), id)
        .and_then(|a| a.run.as_ref())
        .is_some_and(|g| g.complete())
}
pub(super) async fn managers(mut state: Signal<ShellState>, id: u64) -> bool {
    if attempt(&state.peek(), id).is_some_and(|a| a.manager.is_none()) {
        return true;
    }
    loop {
        let index = attempt(&state.peek(), id)
            .and_then(|a| a.manager.as_ref())
            .and_then(|g| g.plan.next());
        let Some(index) = index else { break };
        if !authorized(state, id).await {
            return false;
        }
        let plan = {
            let mut s = state.write();
            let Some(g) = attempt_mut(&mut s, id).and_then(|a| a.manager.as_mut()) else {
                return false;
            };
            if g.plan.mark_sent(index).is_err() {
                return false;
            }
            g.plan.slots()[index].plan().clone()
        };
        let result = send(state, id, &plan, "managers").await;
        let mut s = state.write();
        let Some(a) = attempt_mut(&mut s, id) else {
            return false;
        };
        let Some(g) = a.manager.as_mut() else {
            return false;
        };
        match result {
            Ok(raw) => {
                let _ = g.plan.settle_response(index, raw);
            }
            Err(error) => {
                let not_sent = matches!(&error, Error::Offline(_))
                    || matches!(&error,Error::Api(c)if c.starts_with("workflow_management_not_sent_"));
                g.failures[index] = Some(if not_sent {
                    "refused before dispatch"
                } else {
                    "delivery unconfirmed"
                });
                let _ = g.plan.settle_failure(index, not_sent);
            }
        }
        if !matches!(g.plan.slots()[index].outcome(), Outcome::Succeeded(_)) {
            return false;
        }
        if let Outcome::Succeeded(raw) = g.plan.slots()[index].outcome() {
            a.acknowledged
                .push((protocol::ReadKind::Latest, raw.clone()));
        }
    }
    attempt(&state.peek(), id)
        .and_then(|a| a.manager.as_ref())
        .is_some_and(|g| g.plan.complete())
}
