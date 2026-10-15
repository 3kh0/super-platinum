//! Typed integration with the existing DraftStore FIFO; no second writer.
use super::*;
use crate::drafts::store::terminal;
use tokio::sync::oneshot;
pub struct Client {
    account: String,
}
impl Client {
    pub fn new(account: String) -> Result<Self, Failure> {
        if !valid_account(&account) {
            return Err(Failure::Invalid);
        }
        Ok(Self { account })
    }
    pub async fn load(&self) -> Result<Vec<Record>, Failure> {
        let (reply, receive) = oneshot::channel();
        crate::drafts::store::enqueue_slash_journal(Command::Load {
            account: self.account.clone(),
            reply,
        });
        receive.await.map_err(storage)?
    }
}
impl Journal for Client {
    async fn compare_exchange(
        &mut self,
        old: Option<&Record>,
        next: &Record,
    ) -> Result<(), Failure> {
        let (reply, receive) = oneshot::channel();
        // Enqueue before the first await. The existing worker owns completion
        // even if the UI/controller future is cancelled after submission.
        crate::drafts::store::enqueue_slash_journal(Command::Compare {
            account: self.account.clone(),
            old: old.cloned(),
            next: next.clone(),
            reply,
        });
        receive.await.map_err(storage)?
    }
}
pub(crate) enum Command {
    Compare {
        account: String,
        old: Option<Record>,
        next: Record,
        reply: oneshot::Sender<Result<(), Failure>>,
    },
    Load {
        account: String,
        reply: oneshot::Sender<Result<Vec<Record>, Failure>>,
    },
}
impl Command {
    pub(crate) fn account(&self) -> &str {
        match self {
            Self::Compare { account, .. } | Self::Load { account, .. } => account,
        }
    }
    pub(crate) fn execute(
        self,
        store: Result<&mut Connection, Failure>,
        pending_input: bool,
    ) -> terminal::Report {
        match self {
            Self::Load { account, reply } => {
                let result = store.and_then(|conn| load(conn, &account));
                let report = match &result {
                    Ok(value) => terminal::read(value),
                    Err(Failure::Persistence) => {
                        terminal::Report::new(terminal::Execution::StorageUnresolved)
                    }
                    Err(Failure::Unsupported | Failure::AdmissionUnavailable) => {
                        terminal::Report::new(terminal::Execution::Unsupported)
                    }
                    Err(Failure::RateLimited) => {
                        terminal::Report::new(terminal::Execution::Deferred)
                    }
                    Err(Failure::Invalid | Failure::Stale | Failure::NoReplay) => {
                        terminal::Report::new(terminal::Execution::PreconditionRefused)
                    }
                };
                let _ = reply.send(result);
                report
            }
            Self::Compare {
                account,
                old,
                next,
                reply,
            } => {
                let admission = old.is_none() || matches!(next.phase(), Phase::DispatchPossible);
                let result = if pending_input && admission {
                    Err(Failure::Stale)
                } else {
                    store.and_then(|conn| compare(conn, &account, old.as_ref(), &next))
                };
                let report = match &result {
                    Ok(value) => terminal::accepted(value),
                    Err(Failure::Persistence) => {
                        terminal::Report::new(terminal::Execution::StorageUnresolved)
                    }
                    Err(Failure::Unsupported | Failure::AdmissionUnavailable) => {
                        terminal::Report::new(terminal::Execution::Unsupported)
                    }
                    Err(Failure::RateLimited) => {
                        terminal::Report::new(terminal::Execution::Deferred)
                    }
                    Err(Failure::Invalid | Failure::Stale | Failure::NoReplay) => {
                        terminal::Report::new(terminal::Execution::PreconditionRefused)
                    }
                };
                let _ = reply.send(result);
                report
            }
        }
    }
}
