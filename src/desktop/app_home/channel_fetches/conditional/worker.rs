use super::*;
struct Guard {
    state: Signal<ShellState>,
    wake: Wake,
    batch: Option<Batch>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.state;
        let wake = self.wake.clone();
        let batch = self.batch.take();
        let release = move |s: &mut ShellState| {
            if !s
                .app_home
                .channel_fetches
                .conditional
                .slots
                .get(&wake.context.key())
                .is_some_and(|r| {
                    Arc::ptr_eq(&r.identity, &wake.identity)
                        && r.worker
                            .as_ref()
                            .is_some_and(|t| Arc::ptr_eq(t, &wake.token))
                })
            {
                return None;
            }
            let r = s
                .app_home
                .channel_fetches
                .conditional
                .slots
                .get_mut(&wake.context.key())
                .unwrap();
            if let Some(batch) = batch {
                let _ = r.manager.return_unsent(&r.scope, &batch);
            }
            r.worker = None;
            r.active = false;
            let owner = owner(s, &wake.context).ok();
            if let Some(slot) = s.app_home.channel_fetches.slots.get(&wake.context.key()) {
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
        batch: None,
    };
    loop {
        let wait = {
            let s = state.peek();
            if !guard.wake.current(&s) {
                return;
            }
            let r = &s.app_home.channel_fetches.conditional.slots[&guard.wake.context.key()];
            let Some(due) = r.manager.next_wake() else {
                return;
            };
            let Ok(now) = now(&s) else {
                return;
            };
            let wait = Duration::from_millis(due.saturating_sub(now.millis()));
            s.app_home
                .channel_fetches
                .slots
                .get(&guard.wake.context.key())
                .and_then(|slot| slot.retry_at)
                .map_or(wait, |at| {
                    wait.max(at.saturating_duration_since(Instant::now()))
                })
        };
        tokio::time::sleep(wait).await;
        let admission = begin(&mut state.write(), &guard.wake);
        let attempt = match admission {
            Ok(Admission::Ready(a)) => a,
            Ok(Admission::Wait(mut changes)) => {
                if tokio::time::timeout(Duration::from_secs(90), changes.changed())
                    .await
                    .is_err()
                {
                    return;
                }
                continue;
            }
            _ => return,
        };
        guard.batch = Some(attempt.batch.clone());
        let progress = read::Progress::default();
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            read::acquire(state, &attempt, &progress),
        )
        .await;
        match result {
            Ok(Ok(missing)) if missing.is_empty() => {}
            Ok(Err(e)) => failure(&mut state.write(), &attempt, e),
            _ => failure(&mut state.write(), &attempt, unavailable()),
        }
        let attempted = progress.attempted(&attempt.batch);
        let complete = complete(&mut state.write(), &attempt, &attempted);
        if complete.is_err() {
            return;
        }
        guard.batch = None;
        // The renderer sees the accepted queue atomically after receipt settlement.
        drain::kick(state, attempt.owner.clone());
    }
}
pub(super) enum Admission {
    Ready(Attempt),
    Wait(tokio::sync::watch::Receiver<u64>),
    Idle,
}
pub(super) fn begin(shell: &mut ShellState, wake: &Wake) -> Result<Admission, Error> {
    if !wake.current(shell) {
        return Err(unavailable());
    }
    let owner = owner(shell, &wake.context)?;
    let ctx = owner.context.clone();
    let slot = &shell.app_home.channel_fetches.slots[&ctx.key()];
    let changes = slot.changes.subscribe();
    if slot.initial.is_some()
        || slot.drain.is_some()
        || shell.core.workspaces[&ctx.session.team_id]
            .directory_channels
            .has_pending()
    {
        return Ok(Admission::Wait(changes));
    }
    if slot.retry_at.is_some_and(|at| at > Instant::now()) {
        return Err(unavailable());
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
    let observation = facts(shell, &ctx)?;
    let now = now(shell)?;
    let r = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&ctx.key())
        .unwrap();
    let Some(batch) = r
        .manager
        .begin_due(&r.scope, now, observation)
        .map_err(|_| unavailable())?
    else {
        return Ok(Admission::Idle);
    };
    if batch.ids().iter().any(|id| shadow.row(id).is_err()) {
        r.manager
            .return_unsent(&r.scope, &batch)
            .map_err(|_| unavailable())?;
        return Err(unavailable());
    }
    r.active = true;
    Ok(Admission::Ready(Attempt {
        wake: wake.clone(),
        owner,
        batch,
        revision,
        publication: RefCell::new(publication),
        connection: observation.slow_connection_count,
        shadow: RefCell::new(shadow),
    }))
}
pub(super) fn complete(
    shell: &mut ShellState,
    a: &Attempt,
    attempted: &[String],
) -> Result<(), Error> {
    if !a.accounting_current(shell) {
        return Err(unavailable());
    }
    let ctx = &a.owner.context;
    let observation = facts(shell, ctx)?;
    let now = now(shell)?;
    let reconnect = observation
        .slow_connection_count
        .is_some_and(|c| c > 1 && Some(c) != a.connection);
    let values = if reconnect {
        Vec::new()
    } else if a.current(shell) {
        fresh(&a.shadow.borrow(), attempted)?
    } else {
        let r = shell
            .app_home
            .channel_fetches
            .conditional
            .slots
            .get_mut(&ctx.key())
            .unwrap();
        r.manager
            .return_unsent(&r.scope, &a.batch)
            .map_err(|_| unavailable())?;
        r.active = false;
        return Ok(());
    };
    let cooldown = shell.app_home.channel_fetches.slots[&ctx.key()].retry_at;
    let r = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&ctx.key())
        .unwrap();
    let effects = if reconnect || attempted == a.batch.ids() {
        r.manager
            .finish(&r.scope, &a.batch, now, observation, &values)
    } else {
        r.manager
            .finish_admitted(&r.scope, &a.batch, now, observation, &values)
    }
    .map_err(|_| unavailable())?;
    if cooldown.is_some_and(|at| at > Instant::now()) {
        r.manager
            .resume(&r.scope, now, observation)
            .map_err(|_| unavailable())?;
    }
    r.active = false;
    if effects.notify_immediately {
        shell.app_home.channel_fetches.slots[&ctx.key()].notify();
    }
    Ok(())
}
