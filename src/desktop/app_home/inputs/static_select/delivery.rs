use super::*;
use std::future::Future;

pub(super) async fn deliver(mut state: Signal<ShellState>, q: Queued) {
    if !q.current(&state.peek()) {
        fixture::refused(&mut state.write(), &q, "owner");
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let static_lease = loop {
        if !q.current(&state.peek()) || Instant::now() >= deadline {
            fixture::refused(&mut state.write(), &q, "select-busy-retired");
            return;
        }
        match bridge::lock(state, &q).await {
            bridge::Lock::Acquired(lease) => break lease,
            bridge::Lock::Refused => {
                fixture::refused(&mut state.write(), &q, "select-custody");
                return;
            }
            bridge::Lock::Busy => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    };
    let Some(siblings) = super::super::text::button::lock_snapshot(state, q.binding.visit).await
    else {
        static_lease.release();
        fixture::refused(&mut state.write(), &q, "sibling-custody");
        return;
    };
    fixture::before_dispatch(state).await;
    let prepared = {
        let mut s = state.write();
        if !q.current(&s)
            || !static_lease.current(&s, &q)
            || !siblings.current(&s, q.binding.visit)
            || !super::ready(&s)
            || !super::super::ready(&s)
        {
            None
        } else {
            let current = s
                .app_home
                .surface
                .as_ref()
                .unwrap()
                .view
                .as_ref()
                .unwrap()
                .clone();
            let form = form(&s).unwrap().clone();
            let token = crate::app_modal::register_action_with_context(
                &mut s,
                q.action.app_id(),
                None,
                None,
            );
            token.and_then(|token| {
                match q.action.prepare_with_context(
                    &q.binding.owner.ctx.client,
                    &q.binding.owner.ctx.session,
                    &current,
                    &form,
                    &token.token,
                    q.route.team_id(),
                ) {
                    Ok(request) => {
                        s.app_home
                            .correlations
                            .push((token.token.clone(), q.binding.owner.clone()));
                        if s.app_home.correlations.len() > 10 {
                            s.app_home.correlations.remove(0);
                        }
                        s.app_home.static_select.activity.insert(
                            (q.binding.form.clone(), q.binding.key.block_id.clone()),
                            Activity {
                                binding: q.binding.clone(),
                                route: q.route.clone(),
                                loading: true,
                                error: None,
                            },
                        );
                        Some((token, request))
                    }
                    Err(_) => {
                        crate::app_modal::reject_action(&mut s, &token);
                        None
                    }
                }
            })
        }
    };
    let Some((token, request)) = prepared else {
        fixture::refused(&mut state.write(), &q, "final-current");
        static_lease.release();
        siblings.release();
        return;
    };
    let intercepted = fixture::request(&mut state.write(), &q, &request);
    let result = if let Some(reply) = intercepted {
        // Admission and the actual closed fixture request record precede unlock.
        static_lease.release();
        siblings.release();
        reply
            .await
            .unwrap_or_else(|_| Err(Error::Offline("Home static fixture retired".into())))
    } else {
        let transport = q.binding.owner.ctx.transport.clone();
        let future = transport.execute_block_interaction(request);
        tokio::pin!(future);
        let mut leases = Some((static_lease, siblings));
        // Hand the immutable request into the real transport once before release.
        // Network readiness may still await; we do not hold controls to response.
        std::future::poll_fn(|cx| {
            let polled = future.as_mut().poll(cx);
            if let Some((text, calendar)) = leases.take() {
                text.release();
                calendar.release();
            }
            polled
        })
        .await
    }
    .and_then(|raw| api::decode_block_action_response(&raw));
    let mut s = state.write();
    fixture::response(&mut s, q.id, &result);
    if !q.binding.current(&s) || !q.route.current(&s) {
        return;
    }
    if let Err(error) = &result {
        failure(&mut s, &q.binding.owner.ctx, error);
    }
    let unsent = matches!(&result, Err(Error::RateLimited { .. }))
        || matches!(&result,Err(Error::Api(v)) if v=="block_interaction_not_sent_offline");
    let error = match &result {
        Ok(api::BlockActionResponse::Accepted) => None,
        Ok(api::BlockActionResponse::Rejected(rejected)) => {
            crate::app_modal::reject_action(&mut s, &token);
            Some(
                rejected
                    .message
                    .clone()
                    .unwrap_or_else(|| "The app declined this selection.".into()),
            )
        }
        Err(_) if unsent => {
            crate::app_modal::reject_action(&mut s, &token);
            Some(
                "The selection was not sent. Try again when the connection or rate limit permits."
                    .into(),
            )
        }
        Err(_) => Some(
            "The app has not confirmed this selection. It will not be replayed automatically."
                .into(),
        ),
    };
    // Original loading state is a per-block Set. An earlier completion clears it
    // even if a later request remains pending; the native static UI follows it.
    s.app_home.static_select.activity.insert(
        (q.binding.form.clone(), q.binding.key.block_id.clone()),
        Activity {
            binding: q.binding,
            route: q.route,
            loading: false,
            error,
        },
    );
}
