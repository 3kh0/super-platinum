//! Closed completion adapters; actual protected executor still owns all guards.
use super::*;
use p::commands::{Action, Command};
pub(super) enum Receiver {
    Mutation(mpsc::Receiver<Result<p::Review, p::Error>>),
    Check(mpsc::Receiver<Result<p::CheckReceipt, p::Error>>),
    Baseline(mpsc::Receiver<Result<p::BaselineResult, p::Error>>),
    Observation(mpsc::Receiver<Result<p::ObservationResult, p::Error>>),
    Local(mpsc::Receiver<Result<p::LocalWriterLease, p::Error>>),
}
impl Receiver {
    pub(super) fn wait(self) -> Result<Finished, ()> {
        match self {
            Self::Mutation(r) => r.recv().map(Finished::Mutation).map_err(|_| ()),
            Self::Check(r) => r.recv().map(Finished::Check).map_err(|_| ()),
            Self::Baseline(r) => r.recv().map(Finished::Baseline).map_err(|_| ()),
            Self::Observation(r) => r.recv().map(Finished::Observation).map_err(|_| ()),
            Self::Local(r) => r.recv().map(Finished::Local).map_err(|_| ()),
        }
    }
    pub(super) fn poll(&self) -> Result<Option<Finished>, ()> {
        fn poll<T>(r: &mpsc::Receiver<T>) -> Result<Option<T>, ()> {
            match r.try_recv() {
                Ok(v) => Ok(Some(v)),
                Err(mpsc::TryRecvError::Empty) => Ok(None),
                Err(_) => Err(()),
            }
        }
        match self {
            Self::Mutation(r) => poll(r).map(|v| v.map(Finished::Mutation)),
            Self::Check(r) => poll(r).map(|v| v.map(Finished::Check)),
            Self::Baseline(r) => poll(r).map(|v| v.map(Finished::Baseline)),
            Self::Observation(r) => poll(r).map(|v| v.map(Finished::Observation)),
            Self::Local(r) => poll(r).map(|v| v.map(Finished::Local)),
        }
    }
}
pub(super) fn build(
    account: &str,
    binding: &Binding,
    raw: &Raw,
) -> Result<(Request, Receiver), Reason> {
    let (action, receiver) = match (binding, raw) {
        (
            Binding::Mutation(receipt),
            Raw::Mutation {
                outcome,
                known_until,
            },
        ) => {
            let (reply, receiver) = mpsc::channel();
            (
                Action::Record {
                    receipt: receipt.clone(),
                    outcome: outcome.clone(),
                    known_until: *known_until,
                    reply,
                },
                Receiver::Mutation(receiver),
            )
        }
        (
            Binding::Check(completion),
            Raw::Check {
                result,
                known_until,
            },
        ) => {
            let (reply, receiver) = mpsc::channel();
            (
                Action::CompleteCheck {
                    completion: completion.clone(),
                    result: result.clone(),
                    known_until: *known_until,
                    reply,
                },
                Receiver::Check(receiver),
            )
        }
        (
            Binding::Baseline(completion),
            Raw::Baseline {
                response,
                known_until,
            },
        ) => {
            let (reply, receiver) = mpsc::channel();
            (
                Action::RetainBaseline {
                    completion: completion.clone(),
                    response: response.clone(),
                    known_until: *known_until,
                    reply,
                },
                Receiver::Baseline(receiver),
            )
        }
        (Binding::Observation(ticket), Raw::Observation) => {
            let (reply, receiver) = mpsc::channel();
            (
                Action::FinishRetainedObservation {
                    ticket: ticket.clone(),
                    reply,
                },
                Receiver::Observation(receiver),
            )
        }
        (Binding::Local(release, writer), Raw::Local) => {
            let (reply, receiver) = mpsc::channel();
            (
                Action::StartRetainedLocal {
                    release: release.clone(),
                    writer: writer.clone(),
                    reply,
                },
                Receiver::Local(receiver),
            )
        }
        _ => return Err(Reason::Stale),
    };
    Ok((
        Request::Protected(Box::new(Command {
            account: account.into(),
            action,
        })),
        receiver,
    ))
}
