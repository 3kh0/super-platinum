use super::*;
mod settle;
#[cfg(test)]
pub(super) use settle::settle;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    let original = attempt(&state.peek(), id).unwrap().original.clone();
    let validation = validation::acquire(state, id).await;
    if validation.is_err() {
        if let Some(a) = attempt_mut(&mut state.write(), id) {
            a.validation.status = "failed";
            a.validation.notice = Some(
                "Publication checks could not finish. Retry publication checks; no publication was sent.",
            );
        }
    }
    let validation_only = attempt(&state.peek(), id).is_some_and(|a| a.validation_only);
    if !matches!(validation, Ok(true)) || validation_only {
        let notice = attempt(&state.peek(), id)
            .and_then(|a| a.validation.notice)
            .unwrap_or_else(|| ready_notice(&original.editor));
        finish(state, id, &original, notice).await;
        return;
    }
    for primary in [true, false] {
        if !exact(&state.peek(), id) || !native::current(&original).await {
            break;
        }
        let (lease, request) = {
            let mut s = state.write();
            if !exact(&s, id) {
                break;
            }
            let a = attempt_mut(&mut s, id).unwrap();
            let plan = if primary {
                if a.plan.mark_sent(0).is_err() {
                    break;
                }
                a.plan.slots()[0].plan()
            } else {
                let Some(p) = a.followup.as_mut() else { break };
                if p.phase() == protocol::FollowUpPhase::Complete || p.mark_sent(0).is_err() {
                    break;
                }
                p.slots()[0].plan()
            };
            (
                a.lease.clone(),
                plan.request(&a.lease.owner.ctx.client, &a.lease.owner.ctx.session),
            )
        };
        let result = if fixture::enabled(&state.peek()) {
            fixture::publish::mutation(state, &request)
                .await
                .unwrap_or_else(|| Err(Error::Api("workflow_management_not_sent_fixture".into())))
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else if primary {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_publish(request)
                .await
        } else {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_management(request)
                .await
        };
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        settle::settle(&mut s, id, primary, result);
    }
    finish(state,id,&original,"Publication is not fully confirmed. Each response is retained; no operation will be replayed.").await;
}
async fn finish(mut state: Signal<ShellState>, id: u64, original: &Original, notice: &str) {
    let buffers = native::finish_snapshot(original).await;
    let mut s = state.write();
    let owned = exact(&s, id);
    let Some(a) = attempt_mut(&mut s, id) else {
        return;
    };
    a.finished = true;
    a.complete = a.plan.primary_succeeded()
        && a.followup
            .as_ref()
            .is_some_and(protocol::FollowUpPlan::fully_succeeded);
    a.no_send = a
        .plan
        .slots()
        .iter()
        .all(|s| matches!(s.outcome(), Outcome::Pending | Outcome::NotSent));
    let (complete, no_send, lease, release) =
        (a.complete, a.no_send, a.lease.clone(), a.release.take());
    if let Some(b) = buffers.filter(|_| owned) {
        let d = document_mut(&mut s, original.visit).unwrap();
        d.title = b.title;
        d.description = b.description;
        d.notice=Some(if complete{"Publication and trigger synchronization acknowledged. Check current publication to verify the saved version."}else{notice}.into());
        d.revision = next();
        if no_send {
            d.pending = None;
        }
    }
    lease.finish(&mut s, None);
    if let Some(release) = release {
        release.release();
    }
}
