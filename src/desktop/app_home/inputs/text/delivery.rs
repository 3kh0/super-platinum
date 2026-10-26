use super::*;
use std::future::Future;

pub(super) async fn deliver(mut state: Signal<ShellState>, index: (FormOwner, InputKey), id: u64) {
    let q = {
        let mut s = state.write();
        let Some(q) = s
            .app_home
            .text
            .queued
            .get(&index)
            .filter(|q| q.id == id)
            .cloned()
        else {
            return;
        };
        s.app_home.text.queued.remove(&index);
        if !q.current(&s) {
            return;
        }
        q
    };
    deliver_queued(state, q).await;
}

pub(super) async fn deliver_queued(mut state: Signal<ShellState>, q: Queued) {
    if !q.current(&state.peek()) {
        return;
    }
    let id = q.id;
    let Some((text_lease, calendar_lease)) = acquire(state, &q).await else {
        return;
    };
    fixture::before_dispatch(state).await;
    let prepared = {
        let mut s = state.write();
        if !q.current(&s)
            || !text_lease.current(&s, &q)
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
                        s.app_home.text.activity.insert(
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
        text_lease.release();
        calendar_lease.release();
        return;
    };
    let intercepted = fixture::request(&mut state.write(), &q, &request);
    let result = if let Some(reply) = intercepted {
        // Admission and the actual closed fixture request record precede unlock.
        text_lease.release();
        calendar_lease.release();
        reply
            .await
            .unwrap_or_else(|_| Err(Error::Offline("Home text fixture retired".into())))
    } else {
        let transport = q.binding.owner.ctx.transport.clone();
        let future = transport.execute_block_interaction(request);
        tokio::pin!(future);
        let mut leases = Some((text_lease, calendar_lease));
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
    fixture::response(&mut s, id, &result);
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
                    .unwrap_or_else(|| "The app declined this action.".into()),
            )
        }
        Err(_) if unsent => {
            crate::app_modal::reject_action(&mut s, &token);
            Some(
                "The action was not sent. Try again when the connection or rate limit permits."
                    .into(),
            )
        }
        Err(_) => Some(
            "The app has not confirmed this action. It will not be replayed automatically.".into(),
        ),
    };
    // Original loading state is a per-block Set. An earlier completion clears it
    // even if a later request remains pending; new explicit events stay possible.
    s.app_home.text.activity.insert(
        (q.binding.form.clone(), q.binding.key.block_id.clone()),
        Activity {
            binding: q.binding,
            route: q.route,
            loading: false,
            error,
        },
    );
}

async fn acquire(
    state: Signal<ShellState>,
    q: &Queued,
) -> Option<(bridge::TextLease, super::super::native::Release)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if !q.current(&state.peek()) || Instant::now() >= deadline {
            return None;
        }
        let attempted = button::RetrySnapshot::capture(&state.peek(), q.binding.visit)?;
        match bridge::lock(state, q).await {
            bridge::Lock::Refused => {
                if !attempted.progressed(&state.peek()) {
                    return None;
                }
            }
            bridge::Lock::Busy => {
                attempted.progress(&state.peek())?;
            }
            bridge::Lock::Acquired(text) => {
                if let Some(calendar) =
                    super::super::native::lock_acknowledged(state, q.binding.visit).await
                {
                    return Some((text, calendar));
                }
                let busy = super::super::native::lease_busy(state, q.binding.visit).await;
                text.release();
                let progress = attempted.progress(&state.peek())?;
                if !busy && !progress {
                    return None;
                }
            }
        }
        // Retry only compatible busy custody or newer received progress. Each
        // attempt still requires a fresh exact DOM lease before invocation.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
