use super::*;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    let (lease, original, has_run) = {
        let s = state.peek();
        let Some(a) = attempt(&s, id) else { return };
        (
            a.lease.clone(),
            a.original.clone(),
            a.run_group.is_some() || a.manager_group.is_some(),
        )
    };
    let preflight = async {
        if !exact(&state.peek(), id) || !native::current_for(&original, has_run).await {
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
            || !native::current_for(&original, has_run).await
            || !exact(&state.peek(), id)
        {
            return Err(unavailable());
        }
        Ok(())
    }
    .await;
    if preflight.is_err() {
        finish::finish(state,id,&original,if has_run {"Workflow or permissions changed. No permissions change was sent; your draft is retained."} else {"Workflow or permissions changed. No copy-access change was sent; your draft is retained."}).await;
        return;
    }
    if let Some(a) = attempt_mut(&mut state.write(), id) {
        a.departure.armed = a
            .manager_group
            .as_ref()
            .is_some_and(|g| g.plan.self_removal());
    }
    // Changed copy and run are independent source Promise.all groups. Neither
    // rejected group may silently cancel or replay its sibling.
    tokio::join!(
        async {
            copy(state, id, &lease).await;
            managers::maybe_depart(state, id, &original).await;
        },
        async {
            groups::execute(state, id, &original, &lease).await;
            managers::maybe_depart(state, id, &original).await;
        },
        async {
            managers::execute(state, id, &original, &lease).await;
            managers::maybe_depart(state, id, &original).await;
        }
    );
    finish::finish(state,id,&original,if has_run {"Permissions were rejected or remain unconfirmed. Check current permissions; no request will be replayed. Your draft is retained."} else {"Copy-access change was rejected or remains unconfirmed. Check current permissions; no request will be replayed. Your draft is retained."}).await;
}
async fn copy(mut state: Signal<ShellState>, id: u64, lease: &read::Lease) {
    if !attempt(&state.peek(), id).is_some_and(|a| a.copy_changed) {
        return;
    }
    let request = {
        let mut s = state.write();
        if !managers::dispatch_current(&s, id) {
            return;
        }
        let a = attempt_mut(&mut s, id).unwrap();
        if a.plan.mark_sent(0).is_err() {
            return;
        }
        a.plan.slots()[0]
            .plan()
            .request(&lease.owner.ctx.client, &lease.owner.ctx.session)
    };
    let result = if fixture::enabled(&state.peek()) {
        match fixture::managers::mutation(state, &request).await {
            Some(result) => result,
            None => match fixture::run_access::mutation(state, &request).await {
                Some(result) => result,
                None => fixture::permissions::mutation(state, &request)
                    .await
                    .unwrap_or_else(|| {
                        Err(Error::Api("workflow_management_not_sent_fixture".into()))
                    }),
            },
        }
    } else if crate::fixture::is_fixture() {
        Err(Error::Api("workflow_management_not_sent_fixture".into()))
    } else {
        lease
            .owner
            .ctx
            .transport
            .execute_workflow_copy_permission(request)
            .await
    };
    {
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        settle(&mut s, id, result);
    }
}

pub(super) fn attempt(s: &ShellState, id: u64) -> Option<&Attempt> {
    s.app_home
        .about
        .management
        .permissions
        .attempts
        .iter()
        .find(|a| a.original.id == id)
}
pub(super) fn attempt_mut(s: &mut ShellState, id: u64) -> Option<&mut Attempt> {
    s.app_home
        .about
        .management
        .permissions
        .attempts
        .iter_mut()
        .find(|a| a.original.id == id)
}
pub(super) fn settle(s: &mut ShellState, id: u64, result: Result<Value, Error>) {
    let Some(a) = attempt_mut(s, id) else { return };
    if a.replies[0].is_some() || !matches!(a.plan.slots()[0].outcome(), Outcome::InFlight) {
        return;
    }
    let (outcome, reply) = match result {
        Ok(raw) => {
            let outcome = if raw.is_object()
                && raw["ok"] == true
                && raw.get("error").is_none_or(Value::is_null)
                && raw["permission_type"] == a.desired
            {
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
    if a.plan.settle(0, outcome).is_err() {
        let _ = a.plan.settle(0, Outcome::Unknown);
    }
    a.replies[0] = Some(reply);
}
