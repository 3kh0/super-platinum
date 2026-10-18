//! Closed owned-worker commands. No ordinary Client or caller-selected SQL.
use super::*;
use std::sync::{Arc, mpsc};

pub struct Prepared {
    pub(crate) stream: Stream,
    pub(crate) expected: Review,
    pub(crate) writer: Writer,
    pub(crate) serial: u64,
    pub(crate) input: Input,
}
impl Prepared {
    pub fn new(
        stream: Stream,
        expected: Review,
        writer: Writer,
        serial: u64,
        input: Input,
    ) -> Result<Self, Error> {
        input.paired(&expected)?;
        if serial == 0
            || writer.scope() != expected.scope()
            || writer.enrollment() != expected.row().lineage().enrollment()
            || stream.account() != expected.scope().account_id
            || stream.team() != expected.scope().key.team
            || !stream.is_live()
        {
            return Err(Error::Stale);
        }
        Ok(Self {
            stream,
            expected,
            writer,
            serial,
            input,
        })
    }
    pub fn review(&self) -> &Review {
        &self.expected
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub(crate) fn account(&self) -> &str {
        &self.expected.scope().account_id
    }
}
pub(crate) struct QueuedClaim {
    pub(crate) claim: Claim,
    pub(crate) activity: u64,
}
#[derive(Clone)]
pub struct SaveDraft {
    pub input: Arc<Prepared>,
    pub prior: Option<editor::Draft>,
    pub clear: bool,
}
pub(crate) enum Command {
    Retire {
        input: cleanup::retirement::Retirement,
        reply: mpsc::Sender<Result<cleanup::retirement::Retired, Error>>,
    },
    Cleanup {
        claim: Arc<Claim>,
        stream: Stream,
        reply: mpsc::Sender<Result<Arc<std::sync::Mutex<cleanup::Completed>>, Error>>,
    },
    SaveDraft {
        input: SaveDraft,
        reply: mpsc::Sender<Result<Option<editor::Draft>, Error>>,
    },
    LoadDraft {
        scope: Scope,
        reply: mpsc::Sender<Result<Option<editor::Draft>, Error>>,
    },
    Claim {
        input: Arc<Prepared>,
        reply: mpsc::Sender<Result<QueuedClaim, Error>>,
    },
    Settle {
        claim: Arc<Claim>,
        scope: Scope,
        returned: Arc<Returned>,
        reply: mpsc::Sender<Result<Record, Error>>,
    },
}
impl Command {
    pub(crate) fn scope(&self) -> &Scope {
        match self {
            Self::Retire { input, .. } => input.receipt().record().scope(),
            Self::Cleanup { claim, .. } => claim.record().scope(),
            Self::SaveDraft { input, .. } => input.input.expected.scope(),
            Self::LoadDraft { scope, .. } => scope,
            Self::Claim { input, .. } => input.expected.scope(),
            Self::Settle { scope, .. } => scope,
        }
    }
    pub(crate) fn account(&self) -> &str {
        &self.scope().account_id
    }
    pub(crate) fn is_completion(&self) -> bool {
        matches!(self, Self::Settle { .. } | Self::Cleanup { .. })
    }
    #[inline(never)]
    pub(crate) fn execute(
        self,
        store: Result<&mut DraftStore, Error>,
        pending: bool,
        activity: u64,
    ) -> crate::drafts::store::terminal::Report {
        use crate::drafts::store::terminal;
        match self {
            Self::Retire { input, reply } => {
                // Retirement never touches caption rows, writer state or pending text.
                let result = store.and_then(|store| cleanup::retirement::retire(store, &input));
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Cleanup {
                claim,
                stream,
                reply,
            } => {
                let result = store
                    .and_then(|store| cleanup::complete(store, &claim, &stream))
                    .map(|v| Arc::new(std::sync::Mutex::new(v)));
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::SaveDraft { input, reply } => {
                let result = (|| {
                    if pending {
                        return Err(Error::Pending);
                    }
                    editor::save(store?, &input.input, input.prior.as_ref(), input.clear)
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::LoadDraft { scope, reply } => {
                let result = store.and_then(|store| editor::load(store, &scope));
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Claim { input, reply } => {
                let result = (|| {
                    if pending {
                        return Err(Error::Pending);
                    }
                    let claim = claim_saved(store?, &input)?;
                    Ok(QueuedClaim { claim, activity })
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
            Self::Settle {
                claim,
                scope,
                returned,
                reply,
            } => {
                let result = (|| {
                    if claim.record().scope() != &scope {
                        return Err(Error::Stale);
                    }
                    settle(store?, &claim, returned.as_ref().clone())
                })();
                let report = terminal::protected(&result, terminal::accepted);
                let _ = reply.send(result);
                report
            }
        }
    }
}
