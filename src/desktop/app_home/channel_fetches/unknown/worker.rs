use super::*;
struct Guard {
    state: Signal<ShellState>,
    wake: Wake,
    flight: Option<Flight>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.state;
        let wake = self.wake.clone();
        let flight = self.flight.take();
        let release = move |s: &mut ShellState| {
            // A retired owner may release only its own worker identity, never
            // publish a marker or clear a replacement account's active task.
            let r = s
                .app_home
                .channel_fetches
                .unknown
                .slots
                .get_mut(&wake.owner.context.key())?;
            if !Arc::ptr_eq(&r.identity, &wake.identity)
                || r.worker
                    .as_ref()
                    .is_none_or(|t| !Arc::ptr_eq(t, &wake.token))
            {
                return None;
            }
            if let Some(f) = flight {
                r.manager.cancel(&r.scope, &f);
            }
            r.worker = None;
            r.active = false;
            let owner = owner(s, &wake.owner.context.session.team_id).ok();
            if let Some(slot) = s
                .app_home
                .channel_fetches
                .slots
                .get(&wake.owner.context.key())
            {
                slot.notify();
            }
            owner
        };
        let drain_state = state;
        if let Ok(mut s) = state.try_write() {
            let owner = release(&mut s);
            drop(s);
            if let Some(owner) = owner {
                drain::kick(drain_state, owner);
            }
        } else {
            dioxus::core::spawn_forever(async move {
                let owner = release(&mut state.write());
                if let Some(owner) = owner {
                    drain::kick(state, owner);
                }
            });
        }
    }
}
pub(super) async fn run(mut state: Signal<ShellState>, wake: Wake) {
    let mut guard = Guard {
        state,
        wake,
        flight: None,
    };
    loop {
        let delay = {
            let s = state.peek();
            if !guard.wake.current(&s) {
                return;
            }
            let r = &s.app_home.channel_fetches.unknown.slots[&guard.wake.owner.context.key()];
            if r.manager.pending().is_empty() {
                return;
            }
            let wait = r.manager.remaining(clock());
            s.app_home.channel_fetches.slots[&guard.wake.owner.context.key()]
                .retry_at
                .map_or(wait, |at| {
                    wait.max(at.saturating_duration_since(Instant::now()))
                })
        };
        tokio::time::sleep(delay).await;
        let mut attempt = loop {
            let admission = begin(&mut state.write(), &guard.wake);
            match admission {
                Ok(Admission::Ready(a)) => break a,
                Ok(Admission::Wait(mut changes)) => {
                    if tokio::time::timeout(Duration::from_secs(90), changes.changed())
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                _ => return,
            }
        };
        guard.flight = Some(attempt.flight.clone());
        loop {
            let result =
                tokio::time::timeout(domain::ATTEMPT_TIMEOUT, read::acquire(state, &attempt)).await;
            if !attempt.current(&state.peek()) || attempt.refused_dispatch.get() {
                return;
            }
            match result {
                Ok(Ok(())) => break,
                Ok(Err(error)) => {
                    // The original wrapper catches acquisition failure. Only a
                    // still-current owner may complete its local terminal state.
                    failure(&mut state.write(), &attempt, error);
                    break;
                }
                Err(_) => {
                    let next = {
                        let mut s = state.write();
                        if !attempt.current(&s) {
                            return;
                        }
                        let r = s
                            .app_home
                            .channel_fetches
                            .unknown
                            .slots
                            .get_mut(&attempt.owner.context.key())
                            .unwrap();
                        r.manager.retry_timeout(&r.scope, &attempt.flight)
                    };
                    match next {
                        Ok(Some(next)) => {
                            attempt.flight = next.clone();
                            guard.flight = Some(next);
                        }
                        Ok(None) => {
                            failure(&mut state.write(), &attempt, unavailable());
                            break;
                        }
                        Err(_) => return,
                    }
                }
            }
        }
        if publish::finish(&mut state.write(), &attempt).is_err() {
            return;
        }
        guard.flight = None;
        drain::kick(state, attempt.owner.clone());
    }
}
pub(super) enum Admission {
    Ready(Attempt),
    Wait(tokio::sync::watch::Receiver<u64>),
    Idle,
}
pub(super) fn begin(shell: &mut ShellState, wake: &Wake) -> Result<Admission, Error> {
    if !wake.current(shell) || !wake.owner.publication_current(shell) {
        return Err(unavailable());
    }
    let ctx = &wake.owner.context;
    let slot = &shell.app_home.channel_fetches.slots[&ctx.key()];
    if slot.initial.is_some()
        || slot.drain.is_some()
        || conditional::busy(shell, &ctx.key())
        || shell.core.workspaces[&ctx.session.team_id]
            .directory_channels
            .has_pending()
    {
        return Ok(Admission::Wait(slot.changes.subscribe()));
    }
    let publication = slot.publication.clone();
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let shadow = publication
        .classification(
            &w.directory_channels,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
        )
        .map_err(|_| unavailable())?;
    let revision = w.channel_lifecycle.revision();
    let r = shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get_mut(&ctx.key())
        .unwrap();
    let Some(flight) = r
        .manager
        .begin(&r.scope, clock())
        .map_err(|_| unavailable())?
    else {
        return Ok(Admission::Idle);
    };
    r.active = true;
    Ok(Admission::Ready(Attempt {
        wake: wake.clone(),
        owner: wake.owner.clone(),
        flight,
        revision,
        publication: RefCell::new(publication),
        shadow: RefCell::new(shadow),
        refused_dispatch: Cell::new(false),
        accepted_ids: RefCell::new(Default::default()),
    }))
}
