use super::*;

struct Guard {
    state: Signal<ShellState>,
    wake: Wake,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.state;
        let wake = self.wake.clone();
        let release = move |shell: &mut ShellState| {
            let Some(slot) = shell
                .group_freshness
                .slots
                .get_mut(&key(&wake.owner))
                .filter(|slot| {
                    Arc::ptr_eq(&slot.identity, &wake.identity)
                        && slot
                            .worker
                            .as_ref()
                            .is_some_and(|v| Arc::ptr_eq(v, &wake.worker))
                })
            else {
                return;
            };
            // An externally dropped task cannot certify a completed request.
            if let Some(batch) = slot.active.take() {
                let _ = slot.manager.return_unsent(&slot.scope, &batch);
                slot.finished = slot.finished.saturating_add(1);
            }
            slot.worker = None;
        };
        if let Ok(mut shell) = state.try_write() {
            release(&mut shell);
        } else {
            dioxus::core::spawn_forever(async move {
                release(&mut state.write());
            });
        }
    }
}
pub(super) async fn run(mut state: Signal<ShellState>, wake: Wake) {
    let guard = Guard { state, wake };
    loop {
        let wait = {
            let shell = state.peek();
            if !guard.wake.current(&shell) {
                return;
            }
            let slot = &shell.group_freshness.slots[&key(&guard.wake.owner)];
            let Some(due) = slot.manager.next_wake() else {
                return;
            };
            let Ok(now) = clock() else {
                return;
            };
            Duration::from_millis(due.saturating_sub(now.millis()))
        };
        tokio::time::sleep(wait).await;
        let admitted = begin(&mut state.write(), &guard.wake);
        let Ok(Some((owner, batch))) = admitted else {
            return;
        };
        let progress = super::super::recovery::RefreshProgress::default();
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            super::super::recovery::refresh(
                state,
                owner.clone(),
                batch.ids(),
                &progress,
                |shell| guard.wake.batch_current(shell, &batch),
            ),
        )
        .await;
        let (adopted, errors, timed_out) = match result {
            Ok(Ok((adopted, errors))) => (adopted, errors, false),
            Ok(Err(_)) => (false, 1, false),
            Err(_) => (false, 0, true),
        };
        if complete(
            &mut state.write(),
            &guard.wake,
            &batch,
            &progress.ids(batch.ids()),
            adopted,
            errors,
            timed_out,
        )
        .is_err()
        {
            return;
        }
    }
}
fn begin(shell: &mut ShellState, wake: &Wake) -> Result<Option<(Arc<Context>, Batch)>, Error> {
    if !wake.current(shell) {
        return Err(unavailable());
    }
    // The worker belongs to the account; each actual attempt captures the
    // current socket. A successor cannot lend that socket to a late response.
    let owner = Arc::new(Context::capture(shell, &wake.owner.session.team_id)?);
    let observation = facts(shell, &owner.session.team_id)?;
    let blocked = super::super::recovery::rate_status(shell, &owner.session.team_id).blocked;
    let slot = shell
        .group_freshness
        .slots
        .get_mut(&key(&owner))
        .ok_or_else(unavailable)?;
    let Some(batch) = slot
        .manager
        .begin_due(&slot.scope, clock()?, observation)
        .map_err(|_| unavailable())?
    else {
        return Ok(None);
    };
    // Endpoint debt is a local admission refusal, not a source fetch failure.
    if blocked {
        slot.manager
            .return_unsent(&slot.scope, &batch)
            .map_err(|_| unavailable())?;
        return Ok(None);
    }
    slot.active = Some(batch.clone());
    slot.started = slot.started.saturating_add(1);
    slot.observation = observation;
    Ok(Some((owner, batch)))
}
fn complete(
    shell: &mut ShellState,
    wake: &Wake,
    batch: &Batch,
    attempted: &[String],
    adopted: bool,
    errors: usize,
    timed_out: bool,
) -> Result<(), Error> {
    if !wake.batch_current(shell, batch) {
        return Err(unavailable());
    }
    let observation = facts(shell, &wake.owner.session.team_id)?;
    let slot = shell
        .group_freshness
        .slots
        .get_mut(&key(&wake.owner))
        .ok_or_else(unavailable)?;
    let now = clock()?;
    if attempted == batch.ids() {
        slot.manager
            .finish(&slot.scope, now, observation, batch)
            .map_err(|_| unavailable())?;
    } else {
        slot.manager
            .finish_admitted(&slot.scope, now, observation, batch, attempted)
            .map_err(|_| unavailable())?;
    }
    slot.active = None;
    slot.finished = slot.finished.saturating_add(1);
    slot.observation = observation;
    slot.last = json!({"ids":batch.ids(),"attempted":attempted,"adopted":adopted,
        "errors":errors,"timedOut":timed_out});
    Ok(())
}
