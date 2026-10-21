use super::*;
mod finish;
#[cfg(test)]
pub(super) use finish::finish_owned;
mod settle;
pub(super) use settle::settle;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    let (lease, original) = {
        let s = state.peek();
        let Some(a) = attempt(&s, id) else { return };
        (a.lease.clone(), a.original.clone())
    };
    let preflight = async {
        if !exact(&state.peek(), id) {
            return Err(unavailable());
        }
        if !native::current(&original).await || !exact(&state.peek(), id) {
            return Err(unavailable());
        }
        let policy = policy::acquire(state, &lease).await?;
        let fresh = read::load_editor(state, &lease, original.editor.workflow_id()).await?;
        if !exact(&state.peek(), id)
            || !policy.current(&state.peek())
            || !policy.same(&original.policy)
            || !policy.can_create()
            || !supported(&fresh)
            || !allowed_editor(&fresh, &lease.owner.ctx.session.user_id)
            || !original.editor.same_revision(&fresh)
        {
            return Err(unavailable());
        }
        if !native::current(&original).await || !exact(&state.peek(), id) {
            return Err(unavailable());
        }
        Ok(())
    }
    .await;
    if preflight.is_err() {
        finish::finish(
            state,
            id,
            &original,
            "Workflow or permissions changed. No unpublication was sent; your draft is retained.",
        )
        .await;
        return;
    }
    // Exactly one primary followed by at most one source-derived trigger slot.
    for primary in [true, false] {
        if !exact(&state.peek(), id) {
            diagnostic(primary, "source-before-native");
            break;
        }
        if !native::current(&original).await {
            diagnostic(primary, "native-current");
            break;
        }
        let request = {
            let mut s = state.write();
            if !exact(&s, id) {
                diagnostic(primary, "source-after-native");
                break;
            }
            let a = attempt_mut(&mut s, id).unwrap();
            let plan = if primary {
                if a.plan.mark_sent(0).is_err() {
                    diagnostic(primary, "mark-sent");
                    break;
                }
                a.plan.slots()[0].plan()
            } else {
                let Some(next) = a.followup.as_mut() else {
                    diagnostic(primary, "missing-followup");
                    break;
                };
                if next.phase() == protocol::FollowUpPhase::Complete {
                    diagnostic(primary, "followup-complete");
                    break;
                }
                if next.mark_sent(0).is_err() {
                    diagnostic(primary, "mark-sent");
                    break;
                }
                next.slots()[0].plan()
            };
            plan.request(&lease.owner.ctx.client, &lease.owner.ctx.session)
        };
        let fixture_enabled = fixture::enabled(&state.peek());
        let result = if fixture_enabled {
            fixture::unpublish::mutation(state, &request)
                .await
                .unwrap_or_else(|| Err(Error::Api("workflow_management_not_sent_fixture".into())))
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else if primary {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_unpublish(request)
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
        settle(&mut s, id, primary, result);
    }
    finish::finish(state,id,&original,"Unpublication is not fully confirmed. Your draft and each response are retained. No operation will be replayed.").await;
}
fn diagnostic(primary: bool, boundary: &'static str) {
    if fixture::unpublish::requested() {
        eprintln!("workflow-unpublish-run-guard: primary={primary} boundary={boundary}");
    }
}
pub(super) fn attempt(s: &ShellState, id: u64) -> Option<&Attempt> {
    s.app_home
        .about
        .management
        .unpublishing
        .attempts
        .iter()
        .find(|a| a.original.id == id)
}
pub(super) fn attempt_mut(s: &mut ShellState, id: u64) -> Option<&mut Attempt> {
    s.app_home
        .about
        .management
        .unpublishing
        .attempts
        .iter_mut()
        .find(|a| a.original.id == id)
}
