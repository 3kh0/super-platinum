//! Only this original execute path can publish a native definition receipt.
use super::*;
use super_platinum_core::slack::{
    PreparedRequest,
    api::emoji_catalog::Read,
    transport::{
        Health, ObservedReactionFailure as Failure, ReactionRequestKind, ReactionWireEvent,
    },
};
pub(super) struct Attempt {
    owner: Arc<Picker>,
    prepared: Arc<Prepared>,
    account: Option<String>,
    revision: u64,
    generation: u64,
    identity: ReadIdentity,
    accepted: ReadBarrier,
    media: crate::media::MediaRegistry,
    watch: Arc<ReadWatch>,
    request: PreparedRequest,
}
impl Attempt {
    pub(super) fn capture(
        s: &ShellState,
        owner: Arc<Picker>,
        prepared: Arc<Prepared>,
    ) -> Option<Self> {
        if !parent_ready(&owner, s, &prepared) {
            return None;
        }
        let source = &owner.open.scope.source;
        let request = Read::new(&source.session)
            .request(&source.client, &source.session)
            .ok()?;
        let workspace = s.core.workspaces.get(&source.team)?;
        let c = super::super::super::connection(s, &source.team)?;
        let identity = c.read_identity();
        let accepted = s
            .thread_feed_reads
            .reaction_toggle
            .applied
            .get(&source.team)?
            .barrier
            .clone();
        let watch = owner.open.scope.arm(s, &owner.open.key)?;
        Some(Self {
            request,
            account: s.core.active_account.clone(),
            revision: workspace.custom_emoji_revision,
            generation: workspace.rt_generation,
            identity,
            accepted,
            media: s.media.clone(),
            watch,
            owner,
            prepared,
        })
    }
    fn current(&self, s: &ShellState, serial: u64) -> bool {
        let source = &self.owner.open.scope.source;
        parent_ready(&self.owner, s, &self.prepared)
            && self.account == s.core.active_account
            && source.client.same_configuration(&s.core.client)
            && s.core
                .session
                .as_ref()
                .and_then(|v| v.workspaces.get(&source.team))
                .is_some_and(|v| crate::emoji_preferences::same_session(&source.session, v))
            && s.core.workspaces.get(&source.team).is_some_and(|w| {
                w.custom_emoji_revision == self.revision && w.rt_generation == self.generation
            })
            && super::super::super::connection(s, &source.team)
                .is_some_and(|c| self.identity.open() && self.identity.is_for(c))
            && self.media.same_registry(&s.media)
            && s.thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()
                .is_some_and(|v| {
                    v.custom.mode
                        && v.custom.serial == serial
                        && v.custom.loading
                        && v.ready
                            .as_ref()
                            .is_some_and(|p| Arc::ptr_eq(p, &self.prepared))
                })
    }
    fn readable(&self, state: Signal<ShellState>, serial: u64, ledger: &Registry) -> bool {
        state.try_peek().is_ok_and(|s| self.current(&s, serial))
            && self.watch.current()
            && self.owner.open.scope.source.transport.health() != Health::Offline
            && !ledger.cooling(&self.owner.open.scope.physical(&self.owner.open.key).actor)
    }
}
pub(super) async fn load(mut state: Signal<ShellState>, attempt: Attempt, serial: u64) {
    let result = async {
        let open = &attempt.owner.open;
        let ledger = state
            .try_peek()
            .ok()
            .map(|s| crate::reaction_writes::registry(&s))
            .ok_or(())?;
        let current = || attempt.readable(state, serial, &ledger);
        if !current() {
            return Err(());
        }
        let trace = Trace::new(ledger.clone(), open.scope.physical(&open.key).actor);
        // Clone the original factory output, never rebuild with current/new
        // credentials after an await. Advice belongs to the original actor.
        let request = attempt.request.clone();
        #[cfg(test)]
        let raw = {
            let io =
                try_consume_context::<std::rc::Rc<super::super::super::test_io::Io>>().ok_or(())?;
            if !current() {
                return Err(());
            }
            trace.observe(ReactionWireEvent::Attempted(ReactionRequestKind::Read));
            let raw = io.execute(request).await;
            trace.test_result(ReactionRequestKind::Read, &raw);
            raw.map_err(|_| ())?
        };
        #[cfg(not(test))]
        let raw = {
            if crate::fixture::is_fixture() {
                return Err(());
            }
            open.scope
                .source
                .transport
                .execute_root_reaction_catalog_read_observed(request, current, |event| {
                    trace.observe(event)
                })
                .await
                .map_err(|_: Failure| ())?
        };
        if !current() {
            return Err(());
        }
        let complete = Arc::new(Complete::decode(raw).map_err(|_| ())?);
        let policy = attempt.prepared._policy.clone();
        let membership = attempt.prepared._membership.clone();
        let (complete, domain, choices, model) = tokio::task::spawn_blocking(move || {
            let domain = CustomDomain::build(&complete)?;
            let choices = domain.choices(&policy, policy.preferences(), &membership);
            let model = name_search::Model::direct(
                choices
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.name().to_owned(), i)),
            );
            Ok::<_, String>((complete, domain, choices, model))
        })
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
        if !current() {
            return Err(());
        }
        let ready = Arc::new(Ready {
            receipt: Receipt {
                retired: AtomicBool::new(false),
                open: open.clone(),
                account: attempt.account.clone(),
                revision: attempt.revision,
                generation: attempt.generation,
                identity: attempt.identity.clone(),
                _accepted: attempt.accepted.clone(),
                media: attempt.media.clone(),
                _complete: complete,
            },
            domain,
            choices,
            model,
        });
        let work = {
            let mut s = state.try_write().map_err(|_| ())?;
            if !attempt.current(&s, serial) || !ready.current(&s) {
                return Err(());
            }
            let session = s
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_mut()
                .ok_or(())?;
            let next = session.frame.checked_add(1).ok_or(())?;
            attempt
                .watch
                .with_current(|| {
                    session.frame = next;
                    session.cursor = 0;
                    session.matches.clear();
                    session.custom.ready = Some(ready);
                    session.custom.loading = false;
                    session.custom.notice = String::new();
                    picker::QueryWork::for_session(session)
                })
                .ok_or(())?
        };
        // Definition loading has no query argument. Requery against the actual
        // current literal outside the write lock; a newer keystroke wins via
        // the existing frame/literal publication fence, without another fetch.
        if let Some(work) = work {
            let matches = work.compute();
            picker::publish_query(state, work, matches);
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        if let Ok(mut s) = state.try_write() {
            if let Some(session) = s
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_mut()
                .filter(|v| Arc::ptr_eq(&v.owner, &attempt.owner) && v.custom.serial == serial)
            {
                session.custom.loading = false;
                session.custom.ready = None;
                session.matches.clear();
                session.custom.notice =
                    "Workspace emoji could not load. Retry when connected, or use Emoji.".into();
            }
        }
    }
    super::super::super::wake(state);
}
