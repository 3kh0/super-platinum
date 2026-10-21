use super::*;
pub(in crate::app_home::about::management::permissions) async fn execute(
    mut state: Signal<ShellState>,
    id: u64,
    o: &Original,
    lease: &read::Lease,
) {
    loop {
        let index = {
            let s = state.peek();
            run::attempt(&s, id)
                .and_then(|a| a.manager_group.as_ref())
                .and_then(|g| g.plan.next())
        };
        let Some(index) = index else { return };
        if !dispatch_native(state, id, o).await {
            return;
        }
        let request = {
            let mut s = state.write();
            if !dispatch_current(&s, id) {
                return;
            }
            let g = run::attempt_mut(&mut s, id)
                .unwrap()
                .manager_group
                .as_mut()
                .unwrap();
            if g.plan.mark_sent(index).is_err() {
                return;
            }
            g.plan.slots()[index]
                .plan()
                .request(&lease.owner.ctx.client, &lease.owner.ctx.session)
        };
        let result = if fixture::enabled(&state.peek()) {
            fixture::managers::mutation(state, &request)
                .await
                .unwrap_or_else(|| Err(Error::Api("workflow_management_not_sent_fixture".into())))
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else {
            lease
                .owner
                .ctx
                .transport
                .execute_workflow_collaborators(request)
                .await
        };
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &lease.owner.ctx, *retry_after_secs);
        }
        let Some(g) = run::attempt_mut(&mut s, id).and_then(|a| a.manager_group.as_mut()) else {
            return;
        };
        match result {
            Ok(raw) => {
                let _ = g.plan.settle_response(index, raw);
            }
            Err(error) => {
                let not_sent = matches!(&error, Error::Offline(_))
                    || matches!(&error,Error::Api(code) if code.starts_with("workflow_management_not_sent_"));
                g.failures[index] = Some(if not_sent {
                    "refused before dispatch"
                } else {
                    "delivery unconfirmed"
                });
                let _ = g.plan.settle_failure(index, not_sent);
            }
        }
        if !matches!(g.plan.slots()[index].outcome(), Outcome::Succeeded(_)) {
            return;
        }
    }
}
