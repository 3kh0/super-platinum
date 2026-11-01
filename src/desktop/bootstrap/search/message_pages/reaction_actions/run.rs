//! Root-owned bounded IO. Retirement fences precede dispatch, not receipt drain.
use super::*;
use super_platinum_core::{
    search_reaction::Membership,
    slack::{
        PreparedRequest, api,
        transport::{SearchReactionFailure as Failure, SearchReactionRead as Kind},
    },
};

fn wake(mut state: Signal<ShellState>) {
    drop(state.try_write());
}
fn read_current(state: Signal<ShellState>, p: &Picker, watch: &ReadWatch) -> bool {
    let s = state.peek();
    p.current(&s)
        && p.owner.transport.health() != super_platinum_core::slack::transport::Health::Offline
        && watch.current()
        && !s
            .search_reaction_actions
            .registry
            .cooling(&p.owner.key.actor)
}
async fn read(
    state: Signal<ShellState>,
    p: &Picker,
    watch: &ReadWatch,
    kind: Kind,
    request: PreparedRequest,
) -> Result<Value, Failure> {
    if !read_current(state, p, watch) {
        return Err(Failure::NotRequested(unavailable()));
    }
    #[cfg(test)]
    let io = state.peek().search_reaction_actions.io.clone();
    #[cfg(test)]
    if let Some(io) = io {
        let result = io
            .execute(request, false)
            .await
            .map_err(Failure::NotRequested);
        if let Err(Failure::NotRequested(e) | Failure::Unknown(e)) = &result {
            p.ledger.cooldown(&p.owner.key.actor, retry_after(e));
        }
        if !read_current(state, p, watch) {
            return Err(Failure::NotRequested(unavailable()));
        }
        return result;
    }
    if crate::fixture::is_fixture() {
        return Err(Failure::NotRequested(unavailable()));
    }
    let result = p
        .owner
        .transport
        .execute_search_reaction_read(request, kind, &p.owner.key.channel, &p.owner.key.ts, || {
            read_current(state, p, watch)
        })
        .await;
    if let Err(Failure::NotRequested(e) | Failure::Unknown(e)) = &result {
        p.ledger.cooldown(&p.owner.key.actor, retry_after(e));
    }
    if !read_current(state, p, watch) {
        return Err(Failure::NotRequested(unavailable()));
    }
    result
}
fn read_request(p: &Picker, kind: Kind) -> PreparedRequest {
    let o = &p.owner;
    match kind {
        Kind::History => api::conversations_history(
            &o.client,
            &o.session,
            api::HistoryArgs {
                channel: o.key.channel.clone(),
                latest: Some(o.key.ts.clone()),
                limit: Some(1),
                inclusive: true,
                ..Default::default()
            },
        ),
        Kind::Channel => api::conversations_info(&o.client, &o.session, o.key.channel.clone()),
        Kind::Membership => api::reactions_get(
            &o.client,
            &o.session,
            o.key.channel.clone(),
            o.key.ts.clone(),
        ),
    }
}
fn cooldown(reservation: &registry::Reservation, error: &Error) {
    reservation.cooldown(retry_after(error));
}
fn retry_after(error: &Error) -> Option<u64> {
    match error {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
        Error::HttpStatus {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => None,
    }
}
pub(super) async fn prepare(state: Signal<ShellState>, p: Arc<Picker>) {
    let result = async {
        let history = read(
            state,
            &p,
            &p.watch,
            Kind::History,
            read_request(&p, Kind::History),
        )
        .await
        .map_err(|_| ())?;
        let info = read(
            state,
            &p,
            &p.watch,
            Kind::Channel,
            read_request(&p, Kind::Channel),
        )
        .await
        .map_err(|_| ())?;
        FreshPolicy::capture(&p.owner.key.channel, &p.owner.key.ts, &history, &info).map_err(|_| ())
    }
    .await;
    if p.current(&state.peek()) {
        if let Ok(policy) = result {
            let published = p.watch.with_current(|| {
                let Ok(mut value) = p.policy.lock() else {
                    return false;
                };
                *value = Some(policy);
                p.phase.store(1, Ordering::Release);
                p.notice("Choose an emoji to add or remove your reaction.");
                true
            });
            if published != Some(true) {
                p.done("This message changed. Close and reopen the picker.");
            }
        } else if !p.watch.current() {
            p.done("This message changed. Close and reopen the picker.");
        } else {
            p.done(
                "Reaction details could not be verified. Close and reopen the picker to try again.",
            );
        }
    }
    wake(state);
}
fn classify(raw: &Value) -> registry::Outcome {
    if raw.get("ok") == Some(&Value::Bool(true)) && raw.get("error").is_none_or(Value::is_null) {
        registry::Outcome::Acknowledged
    } else if raw.get("ok") == Some(&Value::Bool(false))
        && raw.get("error").and_then(Value::as_str).is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 256
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
    {
        registry::Outcome::Rejected
    } else {
        registry::Outcome::Unknown
    }
}
pub(super) async fn send(
    state: Signal<ShellState>,
    p: Arc<Picker>,
    observation: Observation,
    reservation: registry::Reservation,
) {
    let mut name = observation.requested.clone();
    let mut adding = false;
    let result = async {
        let raw = read(
            state,
            &p,
            &p.watch,
            Kind::Membership,
            read_request(&p, Kind::Membership),
        )
        .await
        .map_err(|f| match f {
            Failure::NotRequested(e) | Failure::Unknown(e) => Failure::NotRequested(e),
        })?;
        let membership = Membership::decode(&p.owner.key.channel, &p.owner.key.ts, &raw)
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        let selected = membership
            .selection(
                &observation.loaded.catalog,
                &observation.preferences,
                &observation.requested,
                &p.owner.key.actor.user,
            )
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        name = selected.name().into();
        adding = selected.adding();
        let request = if adding {
            api::reactions_add(
                &p.owner.client,
                &p.owner.session,
                p.owner.key.channel.clone(),
                p.owner.key.ts.clone(),
                name.clone(),
            )
        } else {
            api::reactions_remove(
                &p.owner.client,
                &p.owner.session,
                p.owner.key.channel.clone(),
                p.owner.key.ts.clone(),
                name.clone(),
            )
        };
        {
            let s = state.peek();
            if !p.readable(&s)
                || !p.owner.prewrite(&s, &p.watch)
                || !reservation.held()
                || !observation.current(&s, &p)
                || !selected.current(
                    &observation.loaded.catalog,
                    &observation.preferences,
                    &membership,
                    &p.owner.key.actor.user,
                )
                || !p.policy.lock().is_ok_and(|policy| {
                    policy.as_ref().is_some_and(|v| {
                        v.channel() == p.owner.key.channel && v.timestamp() == p.owner.key.ts
                    })
                })
            {
                return Err(Failure::NotRequested(unavailable()));
            }
            // Only bounded synchronous consumption is linearized with ingress.
            if p.watch.with_current(|| {
                p.phase
                    .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            }) != Some(true)
            {
                return Err(Failure::NotRequested(unavailable()));
            }
        }
        p.notice("Updating reaction…");
        wake(state);
        let current = || {
            let s = state.peek();
            p.current(&s)
                && p.owner.prewrite(&s, &p.watch)
                && observation.current(&s, &p)
                && reservation.held()
                && p.phase.load(Ordering::Acquire) == 3
        };
        #[cfg(test)]
        let io = state.peek().search_reaction_actions.io.clone();
        #[cfg(test)]
        if let Some(io) = io {
            if !current() {
                return Err(Failure::NotRequested(unavailable()));
            }
            return io.execute(request, true).await.map_err(Failure::Unknown);
        }
        if crate::fixture::is_fixture() {
            return Err(Failure::NotRequested(unavailable()));
        }
        p.owner
            .transport
            .execute_search_reaction_once(
                request,
                &p.owner.key.channel,
                &p.owner.key.ts,
                &name,
                adding,
                current,
            )
            .await
    }
    .await;
    let outcome = match &result {
        Ok(raw) => classify(raw),
        Err(Failure::NotRequested(e)) => {
            cooldown(&reservation, e);
            registry::Outcome::NotSent
        }
        Err(Failure::Unknown(e)) => {
            cooldown(&reservation, e);
            registry::Outcome::Unknown
        }
    };
    // This is independent of UI publication and the prewrite watch. In
    // particular our own reaction RT frame may already have retired the watch.
    reservation.finish(registry::Receipt {
        id: p.id,
        name,
        adding,
        outcome,
    });
    p.done(match outcome{
        registry::Outcome::Acknowledged=>if adding{"Reaction added. Check to read the current reactions."}else{"Reaction removed. Check to read the current reactions."},
        registry::Outcome::Rejected=>"Slack rejected this reaction. No local reaction was changed.",
        registry::Outcome::NotSent=>"The reaction was not sent. Close and reopen the picker to try again.",
        registry::Outcome::Unknown=>"The reaction could not be confirmed. Check only reads current reactions; it does not resend.",
    });
    wake(state);
}
pub(super) async fn check(mut state: Signal<ShellState>, p: Arc<Picker>) {
    let result = async {
        let watch = p.owner.arm(&state.peek()).ok_or(())?;
        let raw = read(
            state,
            &p,
            &watch,
            Kind::Membership,
            read_request(&p, Kind::Membership),
        )
        .await
        .map_err(|_| ())?;
        let membership =
            Membership::decode(&p.owner.key.channel, &p.owner.key.ts, &raw).map_err(|_| ())?;
        let mut s = state.write();
        if !p.current(&s) || !watch.current() {
            return Err(());
        }
        let reactions = membership.reactions().to_vec();
        watch
            .with_current(|| {
                s.search_reaction_actions.display = Some(Display {
                    owner: p.clone(),
                    watch: watch.clone(),
                    reactions,
                })
            })
            .ok_or(())?;
        Ok(())
    }
    .await;
    p.checking.store(false, Ordering::Release);
    let unknown = p
        .ledger
        .receipt(&p.owner.key)
        .is_some_and(|r| r.outcome == registry::Outcome::Unknown);
    p.notice(if result.is_ok(){if unknown{"Current reactions loaded. The earlier reaction is still unconfirmed; no reaction was resent."}else{"Current reactions loaded. This check did not send a reaction."}}else{"Current reactions could not be verified. The previous outcome is unchanged."});
    wake(state);
}
