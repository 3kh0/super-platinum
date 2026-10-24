//! Ordered, atomic publication of an accepted source queue.
use super::*;

pub(super) async fn wait(state: Signal<ShellState>, owner: &Owner) -> Result<(), Error> {
    let mut changes = {
        let s = state.peek();
        if !owner.publication_current(&s) {
            return Err(unavailable());
        }
        s.app_home.channel_fetches.slots[&owner.context.key()]
            .changes
            .subscribe()
    };
    kick(state, owner.clone());
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            {
                let s = state.peek();
                if !owner.publication_current(&s) {
                    return Err(unavailable());
                }
                if !s.core.workspaces[&owner.context.session.team_id]
                    .directory_channels
                    .has_pending()
                {
                    return Ok(());
                }
            }
            changes.changed().await.map_err(|_| unavailable())?;
        }
    })
    .await
    .map_err(|_| unavailable())?
}

pub(super) fn kick(mut state: Signal<ShellState>, owner: Owner) {
    let token = {
        let mut s = state.write();
        if !owner.publication_current(&s)
            || conditional::busy(&s, &owner.context.key())
            || unknown::busy(&s, &owner.context.key())
            || conditional::fixture::drain_held(&s)
            || !s.core.workspaces[&owner.context.session.team_id]
                .directory_channels
                .has_pending()
        {
            return;
        }
        let slot = s
            .app_home
            .channel_fetches
            .slots
            .get_mut(&owner.context.key())
            .unwrap();
        if slot.drain.is_some() || slot.retry.is_some() {
            return;
        }
        let token = Arc::new(());
        slot.drain = Some(token.clone());
        token
    };
    dioxus::core::spawn_forever(async move {
        let result =
            tokio::time::timeout(Duration::from_secs(90), run(state, &owner, &token)).await;
        let rearm = {
            let mut s = state.write();
            let active = release(&mut s, &owner, &token);
            if active && !matches!(result, Ok(Ok(()))) {
                failed(&mut s, &owner, unavailable());
            }
            active
                && owner.publication_current(&s)
                && s.core.workspaces[&owner.context.session.team_id]
                    .directory_channels
                    .has_pending()
        };
        if rearm {
            retry(state, owner);
        }
    });
}
fn active(s: &ShellState, owner: &Owner, token: &Arc<()>) -> bool {
    owner.publication_current(s)
        && s.app_home.channel_fetches.slots[&owner.context.key()]
            .drain
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, token))
}
fn release(s: &mut ShellState, owner: &Owner, token: &Arc<()>) -> bool {
    if !owner.current(s) {
        return false;
    }
    let slot = s
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    if !slot
        .drain
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, token))
    {
        return false;
    }
    slot.drain = None;
    slot.notify();
    true
}
fn retry(mut state: Signal<ShellState>, owner: Owner) {
    let (token, deadline) = {
        let mut s = state.write();
        if !owner.publication_current(&s) {
            return;
        }
        let slot = s
            .app_home
            .channel_fetches
            .slots
            .get_mut(&owner.context.key())
            .unwrap();
        if slot.retry.is_some() {
            return;
        }
        let token = Arc::new(());
        let deadline = slot
            .retry_at
            .unwrap_or_else(Instant::now)
            .max(Instant::now() + Duration::from_secs(90));
        slot.retry = Some(token.clone());
        (token, deadline)
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep_until(deadline.into()).await;
        {
            let mut s = state.write();
            if !owner.current(&s) {
                return;
            }
            let slot = s
                .app_home
                .channel_fetches
                .slots
                .get_mut(&owner.context.key())
                .unwrap();
            if !slot
                .retry
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &token))
            {
                return;
            }
            slot.retry = None;
        }
        kick(state, owner);
    });
}
async fn run(mut state: Signal<ShellState>, owner: &Owner, token: &Arc<()>) -> Result<(), Error> {
    let ctx = &owner.context;
    // A new accepted sibling/socket publication may extend the same queue while
    // JS computes lowercase. Restart only through the owned publication chain.
    loop {
        let (observed, mut store, mut native) = {
            let s = state.peek();
            if !active(&s, owner, token) {
                return Err(unavailable());
            }
            let w = &s.core.workspaces[&ctx.session.team_id];
            (
                observe(&s, ctx)?,
                w.directory_channels.clone(),
                w.channels.clone(),
            )
        };
        let mut superseded = false;
        for batch in 0..64 {
            if !store.has_pending() {
                break;
            }
            let current = store
                .observation(&ctx.account, &ctx.session, ctx.epoch, GENERATION, &native)
                .map_err(|_| unavailable())?;
            if let Some(commit) = store
                .drain_source_action(
                    &current,
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    GENERATION,
                    &native,
                )
                .map_err(|_| unavailable())?
            {
                (store, native) = commit.into_parts();
                continue;
            }
            let facts = if let Some(facts) = current.receipt_facts() {
                facts.clone()
            } else {
                members::receipt(&state.peek(), owner, current.incoming())?.facts
            };
            let generation = format!("channel-drain-{}-{batch}", ctx.epoch);
            let pending = store
                .prepare(
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    &generation,
                    &native,
                    facts.clone(),
                )
                .map_err(|_| unavailable())?;
            let values = render::fields_guarded(
                state,
                |s| active(s, owner, token),
                &generation,
                pending.fields(),
            )
            .await?;
            if !observed_current(&state.peek(), ctx, &observed) {
                superseded = true;
                break;
            }
            let reply = RendererReply {
                generation: generation.clone(),
                fields: pending.fields().to_vec(),
                values,
            };
            (store, native) = store
                .finish(
                    pending,
                    &facts,
                    reply,
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    &generation,
                    &native,
                )
                .map_err(|_| unavailable())?
                .into_parts();
        }
        if superseded {
            continue;
        }
        if store.has_pending() {
            return Err(unavailable());
        }
        if !publish(&mut state.write(), owner, token, &observed, store, native)? {
            continue;
        }
        crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
        return Ok(());
    }
}

/// Final atomic sink, shared with cancellation/restart ownership regressions.
pub(super) fn publish(
    shell: &mut ShellState,
    owner: &Owner,
    token: &Arc<()>,
    observed: &Observation,
    store: super_platinum_core::state::directory_channels::Store,
    native: BTreeMap<String, super_platinum_core::slack::models::Channel>,
) -> Result<bool, Error> {
    if !active(shell, owner, token) {
        return Err(unavailable());
    }
    if !observed_current(shell, &owner.context, observed) {
        return Ok(false);
    }
    let posting = search::posting::prepare(shell, owner, &native);
    let opened = search::open::prepare(shell, owner);
    let w = shell
        .core
        .workspaces
        .get_mut(&owner.context.session.team_id)
        .unwrap();
    w.directory_channels = store;
    w.channels = native;
    search::posting::apply(shell, posting);
    search::open::apply(shell, opened);
    advance(shell, owner)?;
    conditional::continuation::clear(shell, owner);
    shell.refresh_from_core();
    Ok(true)
}
