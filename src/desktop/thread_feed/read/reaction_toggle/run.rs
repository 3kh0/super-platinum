use super::*;
use super_platinum_core::{
    slack::{
        PreparedRequest,
        transport::{
            ObservedReactionFailure as Failure, ReactionRequestKind, ReactionWireEvent,
            RootReactionRead as Kind,
        },
    },
    thread_reaction::{RootMembership, RootPolicy},
};

pub(super) async fn get_view(
    transport: Arc<Transport>,
    request: PreparedRequest,
) -> Result<Value, Error> {
    #[cfg(test)]
    {
        let io = try_consume_context::<std::rc::Rc<test_io::Io>>()
            .ok_or_else(|| Error::Offline("No scoped Root reaction test IO".into()))?;
        return io.execute(request).await;
    }
    #[cfg(not(test))]
    {
        if crate::fixture::is_fixture() {
            return Err(unavailable());
        }
        transport.execute(request).await
    }
}
fn unavailable() -> Error {
    Error::Api("root_reaction_unavailable".into())
}
fn current(state: Signal<ShellState>, p: &ReadTask, watch: &ReadWatch) -> bool {
    let s = state.peek();
    p.current(&s)
        && watch.current()
        && p.scope.source.transport.health()
            != super_platinum_core::slack::transport::Health::Offline
        && !p.ledger.cooling(&p.physical().actor)
}
fn request(p: &ReadTask, kind: Kind) -> PreparedRequest {
    let source = &p.scope.source;
    let (c, t) = &p.key;
    match kind {
        Kind::History => api::conversations_history(
            &source.client,
            &source.session,
            api::HistoryArgs {
                channel: c.clone(),
                latest: Some(t.clone()),
                limit: Some(1),
                inclusive: true,
                ..Default::default()
            },
        ),
        Kind::Channel => api::conversations_info(&source.client, &source.session, c.clone()),
        Kind::Membership => {
            api::reactions_get(&source.client, &source.session, c.clone(), t.clone())
        }
        Kind::Preferences => source
            .client
            .rest_form(&source.session, "users.prefs.get", vec![]),
    }
}
async fn read(
    state: Signal<ShellState>,
    p: &ReadTask,
    watch: &ReadWatch,
    kind: Kind,
) -> Result<Value, Failure> {
    if !current(state, p, watch) {
        return Err(Failure::NotRequested(unavailable()));
    }
    let request = request(p, kind);
    // Separate READ trace: it may preserve original actor advice but can never
    // turn a failed metadata read into mutation issuance/Unknown.
    let mut trace = Trace::new(p.ledger.clone(), p.physical().actor);
    #[cfg(test)]
    {
        let io = try_consume_context::<std::rc::Rc<test_io::Io>>()
            .ok_or_else(|| Failure::NotRequested(unavailable()))?;
        trace.observe(ReactionWireEvent::Attempted(ReactionRequestKind::Read));
        let result = io.execute(request).await;
        trace.test_result(ReactionRequestKind::Read, &result);
        if !current(state, p, watch) {
            return Err(Failure::NotRequested(unavailable()));
        }
        return result.map_err(Failure::NotRequested);
    }
    #[cfg(not(test))]
    {
        if crate::fixture::is_fixture() {
            return Err(Failure::NotRequested(unavailable()));
        }
        let result = p
            .scope
            .source
            .transport
            .execute_root_reaction_read_observed(
                request,
                kind,
                &p.key.0,
                &p.key.1,
                || current(state, p, watch),
                |event| trace.observe(event),
            )
            .await;
        // The real observer recorded headers before any stale-data refusal.
        if !current(state, p, watch) {
            return Err(Failure::NotRequested(unavailable()));
        }
        result
    }
}
async fn data(
    state: Signal<ShellState>,
    p: &ReadTask,
    watch: &ReadWatch,
) -> Result<(RootPolicy, RootMembership), Failure> {
    let history = read(state, p, watch, Kind::History).await?;
    let info = read(state, p, watch, Kind::Channel).await?;
    let prefs = read(state, p, watch, Kind::Preferences).await?;
    let original = &p
        .scope
        .rows
        .get(&p.key)
        .ok_or_else(|| Failure::NotRequested(unavailable()))?
        .original;
    let policy = RootPolicy::capture(original, &history, &info, &prefs)
        .map_err(|_| Failure::NotRequested(unavailable()))?;
    let full = read(state, p, watch, Kind::Membership).await?;
    let membership = RootMembership::decode(original, &full)
        .map_err(|_| Failure::NotRequested(unavailable()))?;
    Ok((policy, membership))
}
pub(super) async fn toggle(state: Signal<ShellState>, p: Arc<Intent>, reservation: Reservation) {
    let mut trace = Trace::new(p.ledger.clone(), p.action.physical().actor);
    let mut name = p.action.name.clone();
    let mut adding = false;
    let reader = ReadTask {
        id: p.id,
        scope: p.action.scope.clone(),
        key: p.action.key.clone(),
        ledger: p.ledger.clone(),
        mutation: Some(p.clone()),
    };
    let result = async {
        if !p.current(&state.peek()) {
            return Err(Failure::NotRequested(unavailable()));
        }
        let loaded = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            crate::emoji_catalog::ensure_complete(state),
        )
        .await
        .ok()
        .flatten()
        .ok_or_else(|| Failure::NotRequested(unavailable()))?;
        let watch = {
            let s = state.peek();
            if !p.current(&s) || !loaded.current(&s) {
                return Err(Failure::NotRequested(unavailable()));
            }
            p.action
                .scope
                .arm(&s, &p.action.key)
                .ok_or_else(|| Failure::NotRequested(unavailable()))?
        };
        let (policy, membership) = data(state, &reader, &watch).await?;
        let selected = membership
            .selection(
                &policy,
                &loaded.catalog,
                policy.preferences(),
                &p.action.name,
                &p.action.scope.source.session.user_id,
            )
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        name = selected.name().into();
        adding = selected.adding();
        let source = &p.action.scope.source;
        let request = if adding {
            api::reactions_add(
                &source.client,
                &source.session,
                p.action.key.0.clone(),
                p.action.key.1.clone(),
                name.clone(),
            )
        } else {
            api::reactions_remove(
                &source.client,
                &source.session,
                p.action.key.0.clone(),
                p.action.key.1.clone(),
                name.clone(),
            )
        };
        {
            let s = state.peek();
            if !current(state, &reader, &watch)
                || !reservation.held()
                || !loaded.current(&s)
                || crate::emoji_catalog::current(&s).is_none_or(|v| !Arc::ptr_eq(&v, &loaded))
                || !selected.current(
                    &policy,
                    &loaded.catalog,
                    policy.preferences(),
                    &membership,
                    &source.session.user_id,
                )
            {
                return Err(Failure::NotRequested(unavailable()));
            }
            if watch.with_current(|| {
                p.phase
                    .compare_exchange(0, 3, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            }) != Some(true)
            {
                return Err(Failure::NotRequested(unavailable()));
            }
        }
        p.notice("Updating reaction…");
        wake(state);
        let check = || {
            current(state, &reader, &watch)
                && reservation.held()
                && loaded.current(&state.peek())
                && crate::emoji_catalog::current(&state.peek())
                    .is_some_and(|v| Arc::ptr_eq(&v, &loaded))
                && p.phase.load(Ordering::Acquire) == 3
        };
        #[cfg(test)]
        {
            let io = try_consume_context::<std::rc::Rc<test_io::Io>>()
                .ok_or_else(|| Failure::NotRequested(unavailable()))?;
            if !check() {
                return Err(Failure::NotRequested(unavailable()));
            }
            // Scoped IO models the actual write boundary, never a NativeAck.
            trace.observe(ReactionWireEvent::Attempted(ReactionRequestKind::Write));
            let raw = io.execute(request).await;
            trace.test_result(ReactionRequestKind::Write, &raw);
            return raw.map_err(Failure::Unknown);
        }
        #[cfg(not(test))]
        {
            if crate::fixture::is_fixture() {
                return Err(Failure::NotRequested(unavailable()));
            }
            source
                .transport
                .execute_root_reaction_once_observed(
                    request,
                    &p.action.key.0,
                    &p.action.key.1,
                    &name,
                    adding,
                    check,
                    |event| trace.observe(event),
                )
                .await
        }
    }
    .await;
    // Always finish original physical evidence/advice before looking at any
    // current UI/read owner. Own RT can already have retired the write watch.
    let outcome = trace.outcome(&result);
    reservation.finish(crate::reaction_writes::Receipt {
        id: 0,
        name,
        adding,
        outcome,
    });
    // One actual issued mutation spends this original write roster even when
    // its RT echo has not arrived. Only a successful new getView can replace
    // it; read-only Check never creates a successor writer.
    if trace.attempted() {
        if let Some(row) = p.action.scope.rows.get(&p.action.key) {
            row.retired.store(true, Ordering::Release);
        }
    }
    p.notice(match outcome {
        Outcome::Acknowledged=>if adding{"Reaction added. Refresh before another toggle, or Check current reactions."}else{"Reaction removed. Refresh before another toggle, or Check current reactions."},
        Outcome::Rejected=>"Slack rejected the reaction. Refresh before trying again.",
        Outcome::NotSent=>"The reaction was not sent. You can refresh to try again.",
        Outcome::Unknown=>"The reaction could not be confirmed. Check only reads; it never resends or unblocks it.",
    });
    p.phase.store(4, Ordering::Release);
    wake(state);
}
pub(super) async fn check(mut state: Signal<ShellState>, p: Arc<ReadTask>) {
    let result = async {
        let watch = p.scope.arm(&state.peek(), &p.key).ok_or(())?;
        let (_, membership) = data(state, &p, &watch).await.map_err(|_| ())?;
        let mut s = state.write();
        if !p.current(&s) || !watch.current() {
            return Err(());
        }
        let display = Display {
            scope: p.scope.clone(),
            key: p.key.clone(),
            reactions: membership.reactions().to_vec(),
        };
        watch
            .with_current(|| s.thread_feed_reads.reaction_toggle.display = Some(display))
            .ok_or(())?;
        Ok(())
    }
    .await;
    {
        let mut s = state.write();
        if s.thread_feed_reads
            .reaction_toggle
            .checking
            .as_ref()
            .is_some_and(|q| q.id == p.id)
        {
            if p.current(&s) {
                s.thread_feed_reads.reaction_toggle.check_notice=Some((p.scope.id,p.key.clone(),if result.is_ok(){"Current reactions loaded. This read did not resend, confirm or unblock the earlier operation."}
                else{"Current reactions could not be verified. The original outcome is unchanged."}.into()));
            }
            s.thread_feed_reads.reaction_toggle.checking = None;
        }
    }
    wake(state);
}
