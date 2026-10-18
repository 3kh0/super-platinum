//! Closed queue commands; no Client method or ordinary Draft operation mapping.
use super::*;
use crate::slash_commands::controller::{ClaimPlan, ManagedTerminal};
use std::sync::{Arc, mpsc};

pub(crate) struct Input {
    pub(crate) stream: Stream,
    pub(crate) expected: Review,
    pub(crate) writer: Writer,
    pub(crate) serial: u64,
    pub(crate) plan: Arc<ClaimPlan>,
}
impl Input {
    pub(crate) fn account(&self) -> &str {
        &self.expected.scope().account_id
    }
}
pub(crate) struct QueuedClaim {
    pub(crate) claim: Claim,
    pub(crate) activity: u64,
}
/// Current accepted native checkpoint, bound by the closed owned facade.
pub(crate) struct ArchiveInput {
    pub(crate) stream: Stream,
    pub(crate) expected: Review,
    pub(crate) writer: Writer,
    pub(crate) serial: u64,
    pub(crate) intent: String,
}
#[derive(Clone)]
pub(crate) struct Archived {
    pub(crate) review: Review,
    pub(crate) intent: String,
}
fn archive_error(error: archive::ArchiveError) -> Error {
    match error {
        archive::ArchiveError::Storage => Error::Storage,
        archive::ArchiveError::Stale => Error::Stale,
        archive::ArchiveError::Pending => Error::Pending,
        archive::ArchiveError::NeedsRemoteReview | archive::ArchiveError::ChoiceRequired => {
            Error::ReviewRequired
        }
    }
}
pub(crate) enum Command {
    Archive {
        input: Arc<ArchiveInput>,
        reply: mpsc::Sender<Result<Archived, Error>>,
    },
    Reserve {
        input: Arc<Input>,
        reply: mpsc::Sender<Result<(), Error>>,
    },
    Claim {
        input: Arc<Input>,
        reply: mpsc::Sender<Result<QueuedClaim, Error>>,
    },
    Complete {
        target: Arc<completion::Target>,
        outcome: Arc<ManagedTerminal>,
        reply: mpsc::Sender<Result<completion::Settlement, Error>>,
    },
}
impl Command {
    pub(crate) fn account(&self) -> &str {
        match self {
            Self::Archive { input, .. } => &input.expected.scope().account_id,
            Self::Reserve { input, .. } | Self::Claim { input, .. } => input.account(),
            Self::Complete { target, .. } => &target.scope().account_id,
        }
    }
    pub(crate) fn scope(&self) -> &Scope {
        match self {
            Self::Archive { input, .. } => input.expected.scope(),
            Self::Reserve { input, .. } | Self::Claim { input, .. } => input.expected.scope(),
            Self::Complete { target, .. } => target.scope(),
        }
    }
    #[inline(never)]
    pub(crate) fn execute(
        self,
        store: Result<&mut DraftStore, Error>,
        pending_input: bool,
        activity: u64,
    ) -> crate::drafts::store::terminal::Report {
        use crate::drafts::store::terminal;
        match self {
            Self::Archive { input, reply } => {
                let result = (|| {
                    if pending_input {
                        return Err(Error::Pending);
                    }
                    let store = store?;
                    let reviewed = archive::inspect(
                        store,
                        &input.stream,
                        &input.expected,
                        &input.writer,
                        input.serial,
                        &input.intent,
                    )
                    .map_err(archive_error)?;
                    let archived =
                        archive::archive_known(store, &reviewed).map_err(archive_error)?;
                    Ok(Archived {
                        review: archived.historical_review().map_err(archive_error)?,
                        intent: archived.original_intent().to_owned(),
                    })
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Reserve { input, reply } => {
                let result = (|| {
                    if pending_input {
                        return Err(Error::Pending);
                    }
                    if !input.plan.current()
                        || input.plan.prior().original() != &original(&input.expected)
                    {
                        return Err(Error::Stale);
                    }
                    let store = store?;
                    // Schemas must already be enrolled by the named migration.
                    disk(bindings::require(&store.0, input.account()))?;
                    disk(history::require(&store.0, input.account()))?;
                    views::exact(&store.0, &input.expected)?;
                    slash_journal::compare(&mut store.0, input.account(), None, input.plan.prior())
                        .map_err(|error| {
                            use crate::slash_commands::Failure;
                            match error {
                                Failure::Persistence => Error::Storage,
                                Failure::RateLimited => Error::Pending,
                                Failure::Unsupported | Failure::AdmissionUnavailable => {
                                    Error::Unsupported
                                }
                                Failure::Invalid | Failure::Stale | Failure::NoReplay => {
                                    Error::Stale
                                }
                            }
                        })
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Claim { input, reply } => {
                let result = (|| {
                    if pending_input {
                        return Err(Error::Pending);
                    }
                    if !input.plan.current() {
                        return Err(Error::Stale);
                    }
                    claim(
                        store?,
                        &input.stream,
                        &input.expected,
                        &input.writer,
                        input.serial,
                        input.plan.prior(),
                        input.plan.next(),
                    )
                    .map(|claim| QueuedClaim { claim, activity })
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Complete {
                target,
                outcome,
                reply,
            } => {
                let result =
                    store.and_then(|store| completion::complete_attested(store, &target, &outcome));
                let report =
                    terminal::protected(&result, |v| terminal::accepted(v).pending(v.pending()));
                let _ = reply.send(result);
                report
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn fixture_install(store: &mut DraftStore, account: &str) {
    slash_journal::install(&mut store.0, account).unwrap();
    install(store, account).unwrap();
    completion::install(store, account).unwrap();
    history::migrate(store, account).unwrap();
}

#[cfg(test)]
pub(crate) fn fixture_review(store: &DraftStore, scope: &Scope) -> Review {
    views::review(&store.0, scope).unwrap()
}

#[cfg(any(test, feature = "protected-test-support"))]
pub(crate) fn seed_fresh_fixture(store: &mut DraftStore, account: &str) -> Result<(), Error> {
    disk(storage::activate(store, account))?;
    disk(adapter_schema::install(store, account))?;
    disk(slash_journal::install(&mut store.0, account))?;
    install(store, account)?;
    completion::install(store, account)?;
    history::migrate(store, account)?;
    Ok(())
}
#[cfg(test)]
pub(crate) fn fixture_seed(store: &mut DraftStore, account: &str) {
    seed_fresh_fixture(store, account).unwrap();
}
