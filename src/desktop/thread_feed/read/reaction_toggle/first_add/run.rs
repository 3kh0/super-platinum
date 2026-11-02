use super::*;
use super_platinum_core::slack::{
    PreparedRequest,
    transport::{
        ObservedReactionFailure as Failure, ReactionRequestKind, ReactionWireEvent,
        RootReactionRead as Kind,
    },
};
fn unavailable() -> Error {
    Error::Api("root_first_add_unavailable".into())
}
fn loaded_current(s: &ShellState, p: &crate::emoji_catalog::Loaded) -> bool {
    p.current(s) && crate::emoji_catalog::current(s).is_some_and(|c| std::ptr::eq(c.as_ref(), p))
}
fn request(open: &Open, kind: Kind) -> PreparedRequest {
    let source = &open.scope.source;
    let (c, t) = &open.key;
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
fn readable(
    open: &Open,
    ledger: &Registry,
    watch: &ReadWatch,
    current: &impl Fn() -> bool,
) -> bool {
    current()
        && watch.current()
        && open.scope.source.transport.health()
            != super_platinum_core::slack::transport::Health::Offline
        && !ledger.cooling(&open.scope.physical(&open.key).actor)
}
async fn read(
    open: &Open,
    ledger: &Registry,
    watch: &ReadWatch,
    kind: Kind,
    current: &impl Fn() -> bool,
) -> Result<Value, Failure> {
    if !readable(open, ledger, watch, current) {
        return Err(Failure::NotRequested(unavailable()));
    }
    let request = request(open, kind);
    let trace = Trace::new(ledger.clone(), open.scope.physical(&open.key).actor);
    #[cfg(test)]
    {
        let io = try_consume_context::<std::rc::Rc<super::super::test_io::Io>>()
            .ok_or_else(|| Failure::NotRequested(unavailable()))?;
        trace.observe(ReactionWireEvent::Attempted(ReactionRequestKind::Read));
        let raw = io.execute(request).await;
        trace.test_result(ReactionRequestKind::Read, &raw);
        if !readable(open, ledger, watch, current) {
            return Err(Failure::NotRequested(unavailable()));
        }
        return raw.map_err(Failure::NotRequested);
    }
    #[cfg(not(test))]
    {
        if crate::fixture::is_fixture() {
            return Err(Failure::NotRequested(unavailable()));
        }
        let result = open
            .scope
            .source
            .transport
            .execute_root_reaction_read_observed(
                request,
                kind,
                &open.key.0,
                &open.key.1,
                || readable(open, ledger, watch, current),
                |event| trace.observe(event),
            )
            .await;
        if !readable(open, ledger, watch, current) {
            return Err(Failure::NotRequested(unavailable()));
        }
        result
    }
}
async fn data(
    open: &Open,
    ledger: &Registry,
    watch: &ReadWatch,
    current: &impl Fn() -> bool,
) -> Result<(RootPolicy, RootMembership), Failure> {
    let history = read(open, ledger, watch, Kind::History, current).await?;
    let info = read(open, ledger, watch, Kind::Channel, current).await?;
    let prefs = read(open, ledger, watch, Kind::Preferences, current).await?;
    let row = open
        .scope
        .rows
        .get(&open.key)
        .ok_or_else(|| Failure::NotRequested(unavailable()))?;
    let policy = RootPolicy::capture(&row.original, &history, &info, &prefs).map_err(|_| {
        row.retired.store(true, Ordering::Release);
        Failure::NotRequested(unavailable())
    })?;
    let full = read(open, ledger, watch, Kind::Membership, current).await?;
    let membership = RootMembership::decode(&row.original, &full).map_err(|_| {
        row.retired.store(true, Ordering::Release);
        Failure::NotRequested(unavailable())
    })?;
    Ok((policy, membership))
}
pub(super) async fn prepare(mut state: Signal<ShellState>, p: Arc<Picker>, attempt: u64) {
    let result = async {
        if !p.preparing(&state.peek(), attempt) {
            return Err(());
        }
        let loaded = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            crate::emoji_catalog::ensure_complete(state),
        )
        .await
        .ok()
        .flatten()
        .ok_or(())?;
        if !p.preparing(&state.peek(), attempt) || !loaded_current(&state.peek(), &loaded) {
            return Err(());
        }
        let ledger = crate::reaction_writes::registry(&state.peek());
        let watch = {
            let s = state.peek();
            if !p.preparing(&s, attempt) || !loaded_current(&s, &loaded) {
                return Err(());
            }
            p.open.scope.arm(&s, &p.open.key).ok_or(())?
        };
        let current = || {
            let s = state.peek();
            p.preparing(&s, attempt) && loaded_current(&s, &loaded)
        };
        let (policy, membership) = data(&p.open, &ledger, &watch, &current)
            .await
            .map_err(|_| ())?;
        let catalog = loaded.catalog.clone();
        // Immutable domains and projection run off render after the SAME fresh
        // policy/full membership. No original native owner is moved to a worker.
        let (domain, alias_domain, choices, name_model, policy, membership) =
            tokio::task::spawn_blocking(move || {
                static DOMAIN: std::sync::OnceLock<
                    std::result::Result<(PreferredDomain, AliasDomain), String>,
                > = std::sync::OnceLock::new();
                let (domain, alias_domain) = DOMAIN
                    .get_or_init(|| Ok((PreferredDomain::build()?, AliasDomain::build()?)))
                    .clone()?;
                let canonical = domain.choices(&catalog, policy.preferences());
                let admitted = canonical
                    .iter()
                    .map(|c| c.base().to_owned())
                    .collect::<std::collections::BTreeSet<_>>();
                let aliases =
                    alias_domain.choices(&policy, &catalog, policy.preferences(), &membership);
                let choices = canonical
                    .into_iter()
                    .map(Selected::Canonical)
                    .chain(
                        aliases
                            .into_iter()
                            .filter(|c| admitted.contains(c.canonical_group()))
                            .map(Selected::Alias),
                    )
                    .collect::<Vec<_>>();
                let name_model = name_search::Model::build(&catalog, &choices);
                Ok::<_, String>((
                    domain,
                    alias_domain,
                    choices,
                    name_model,
                    policy,
                    membership,
                ))
            })
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        let prepared = Arc::new(Prepared {
            loaded,
            domain,
            alias_domain,
            choices,
            name_model,
            _policy: policy,
            _membership: Arc::new(membership),
        });
        let (literal, query_frame) = {
            let s = state.peek();
            if !p.preparing(&s, attempt)
                || !watch.current()
                || !loaded_current(&s, &prepared.loaded)
            {
                return Err(());
            }
            let session = s
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()
                .ok_or(())?;
            (session.query.clone(), session.frame)
        };
        let matches = picker::matching(&prepared, &literal);
        let mut s = state.write();
        if !p.preparing(&s, attempt) || !watch.current() || !loaded_current(&s, &prepared.loaded) {
            return Err(());
        }
        let session = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_mut()
            .filter(|p| p.frame == query_frame && p.query == literal)
            .ok_or(())?;
        let frame = session.frame.checked_add(1).ok_or(())?;
        watch
            .with_current(|| {
                session.frame = frame;
                session.cursor = 0;
                session.matches = matches;
                session.ready = Some(prepared);
                session.loading = false;
                session.notice = String::new();
            })
            .ok_or(())?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let mut s = state.write();
        // Completion may retire its own row on a newly observed denial. It
        // still clears this exact attempt's local busy state, but can never
        // publish data or touch a replacement picker/attempt.
        if let Some(session) = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_mut()
            .filter(|session| {
                Arc::ptr_eq(&session.owner, &p) && p.attempt.load(Ordering::Acquire) == attempt
            })
        {
            session.loading = false;
            session.ready = None;
            session.matches.clear();
            session.notice="Could not verify reaction choices. Retry when connected, or close and refresh Threads.".into();
        }
    }
    super::super::wake(state);
}
pub(super) async fn toggle(state: Signal<ShellState>, p: Arc<Intent>, reservation: Reservation) {
    let trace = Trace::new(p.ledger.clone(), p.open.scope.physical(&p.open.key).actor);
    let name = p.selected.wire_name().to_owned();
    let mut adding = false;
    let result = async {
        let watch = {
            let s = state.peek();
            if !p.current(&s) || !loaded_current(&s, &p.prepared.loaded) {
                return Err(Failure::NotRequested(unavailable()));
            }
            p.open
                .scope
                .arm(&s, &p.open.key)
                .ok_or_else(|| Failure::NotRequested(unavailable()))?
        };
        let current = || {
            let s = state.peek();
            p.current(&s) && loaded_current(&s, &p.prepared.loaded)
        };
        let (policy, membership) = data(&p.open, &p.ledger, &watch, &current).await?;
        let selected = p
            .selected
            .select(
                &policy,
                &p.prepared,
                &membership,
                &p.open.scope.source.session.user_id,
            )
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        if selected.name() != name {
            return Err(Failure::NotRequested(unavailable()));
        }
        adding = selected.adding();
        let source = &p.open.scope.source;
        let request = if adding {
            api::reactions_add(
                &source.client,
                &source.session,
                p.open.key.0.clone(),
                p.open.key.1.clone(),
                name.clone(),
            )
        } else {
            api::reactions_remove(
                &source.client,
                &source.session,
                p.open.key.0.clone(),
                p.open.key.1.clone(),
                name.clone(),
            )
        };
        if !readable(&p.open, &p.ledger, &watch, &current)
            || !reservation.held()
            || !selected.current(&policy, &p.prepared, &membership, &source.session.user_id)
        {
            return Err(Failure::NotRequested(unavailable()));
        }
        if watch.with_current(|| {
            p.phase
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        }) != Some(true)
        {
            return Err(Failure::NotRequested(unavailable()));
        }
        p.notice("Updating reaction…");
        super::super::wake(state);
        let check = || {
            readable(&p.open, &p.ledger, &watch, &current)
                && reservation.held()
                && p.phase.load(Ordering::Acquire) == 1
        };
        #[cfg(test)]
        {
            let io = try_consume_context::<std::rc::Rc<super::super::test_io::Io>>()
                .ok_or_else(|| Failure::NotRequested(unavailable()))?;
            if !check() {
                return Err(Failure::NotRequested(unavailable()));
            }
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
                    &p.open.key.0,
                    &p.open.key.1,
                    &name,
                    adding,
                    check,
                    |event| trace.observe(event),
                )
                .await
        }
    }
    .await;
    let outcome = trace.outcome(&result);
    reservation.finish(crate::reaction_writes::Receipt {
        id: 0,
        name,
        adding,
        outcome,
    });
    if trace.attempted() {
        if let Some(row) = p.open.scope.rows.get(&p.open.key) {
            row.retired.store(true, Ordering::Release);
        }
    }
    p.notice(match outcome{
        Outcome::Acknowledged=>if adding{"Reaction added. Refresh Threads to read current reactions."}else{"Reaction removed. Refresh Threads to read current reactions."},
        Outcome::Rejected=>"Slack rejected the reaction. Refresh before trying again.",
        Outcome::NotSent=>"The reaction was not sent. You can refresh Threads to try again.",
        Outcome::Unknown=>"The reaction could not be confirmed. Refresh only reads; it never resends or unblocks the operation.",
    });
    p.phase.store(2, Ordering::Release);
    super::super::wake(state);
}
