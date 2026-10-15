//! Executor observations only. Not admission, durability of all input, or drain.
use crate::{drafts::store::protected, error::AppError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Execution {
    ReadObserved,
    // The exact storage API accepted its action, including idempotent success.
    // This does not assert that a new transaction or remote mutation occurred.
    StorageAccepted,
    PreservedConflict,
    // A refused typed result can still commit an independent cooldown. This is
    // not a no-side-effects or rolled-back-transaction assertion.
    PreconditionRefused,
    StorageUnresolved,
    Deferred,
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Inventory {
    NotAssessed,
    OperationPending,
    ConflictPreserved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub execution: Execution,
    // Describes only the returned evidence, which can be an idempotent historical
    // receipt. Never a fresh/full account inventory or a "settled" flag.
    pub inventory: Inventory,
    // Always false: C2.1 does not observe all before-enqueue/runtime side effects.
    pub capability_effects_assessed: bool,
}
impl Report {
    pub(crate) const fn new(execution: Execution) -> Self {
        Self {
            execution,
            inventory: Inventory::NotAssessed,
            capability_effects_assessed: false,
        }
    }
    pub(crate) const fn preserved() -> Self {
        Self {
            inventory: Inventory::ConflictPreserved,
            ..Self::new(Execution::PreservedConflict)
        }
    }
    pub(crate) fn pending(mut self, pending: bool) -> Self {
        if pending {
            self.inventory = Inventory::OperationPending;
        }
        self
    }
}
// Borrow the existing result. Never pass a large command/result through a new
// generic stack frame, clone its payload, or derive a report from sender success.
pub(crate) fn app<T>(result: &Result<T, AppError>, ok: impl FnOnce(&T) -> Report) -> Report {
    match result {
        Ok(value) => ok(value),
        Err(_) => Report::new(Execution::StorageUnresolved),
    }
}
pub(crate) fn protected<T>(
    result: &Result<T, protected::Error>,
    ok: impl FnOnce(&T) -> Report,
) -> Report {
    use protected::Error;
    match result {
        Ok(value) => ok(value),
        Err(Error::Storage) => Report::new(Execution::StorageUnresolved),
        Err(Error::Unsupported) => Report::new(Execution::Unsupported),
        Err(Error::Cooldown(_)) => Report::new(Execution::Deferred),
        Err(
            Error::Stale | Error::Pending | Error::Gap | Error::ReviewRequired | Error::WrongFormat,
        ) => Report::new(Execution::PreconditionRefused),
    }
}
pub(crate) fn read<T>(_: &T) -> Report {
    Report::new(Execution::ReadObserved)
}
pub(crate) fn accepted<T>(_: &T) -> Report {
    Report::new(Execution::StorageAccepted)
}
pub(crate) fn unsupported<T>(_: &T) -> Report {
    Report::new(Execution::Unsupported)
}
pub(crate) fn refused<T>(_: &T) -> Report {
    Report::new(Execution::PreconditionRefused)
}
pub(crate) fn review(value: &protected::Review) -> Report {
    accepted(value).pending(value.row().state().pending().is_some())
}
pub(crate) fn writer(value: &protected::WriteResult) -> Report {
    match value {
        protected::WriteResult::Applied(value) => review(value),
        protected::WriteResult::Preserved(_) => Report::preserved(),
        protected::WriteResult::Retired => refused(value),
    }
}
pub(crate) fn local_writer(value: &protected::LocalWriteResult) -> Report {
    match value {
        protected::LocalWriteResult::Applied(_) => accepted(value),
        protected::LocalWriteResult::Preserved(_) => Report::preserved(),
        protected::LocalWriteResult::Retired => refused(value),
    }
}
#[path = "terminal_legacy.rs"]
pub(crate) mod legacy;
