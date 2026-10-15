//! Exact legacy enum variants, not Result::is_ok as durable success.
use super::*;
use crate::drafts::store::sync;
pub(crate) fn commit(value: &sync::Commit) -> Report {
    match value {
        sync::Commit::Applied(row) => accepted(value).pending(row.state.pending().is_some()),
        sync::Commit::Stale(_) => refused(value),
    }
}
pub(crate) fn prepare(value: &sync::Prepare) -> Report {
    match value {
        sync::Prepare::Prepared { .. } => accepted(value).pending(true),
        sync::Prepare::Blocked(_) | sync::Prepare::Stale(_) => refused(value),
    }
}
pub(crate) fn release(value: &sync::Release) -> Report {
    match value {
        sync::Release::Released(_) => accepted(value),
        sync::Release::Stale(_) | sync::Release::Blocked(_) => refused(value),
    }
}
pub(crate) fn local_settlement(value: &sync::LocalSettlement) -> Report {
    match value {
        sync::LocalSettlement::Applied { stored, .. } => accepted(value).pending(
            stored
                .as_ref()
                .is_some_and(|row| row.state.pending().is_some()),
        ),
        sync::LocalSettlement::Stale | sync::LocalSettlement::Blocked => refused(value),
    }
}
pub(crate) fn remote_settlement(value: &sync::RemoteSettlement) -> Report {
    match value {
        sync::RemoteSettlement::Applied { stored, .. } => {
            accepted(value).pending(stored.state.pending().is_some())
        }
        sync::RemoteSettlement::Stale | sync::RemoteSettlement::Blocked => refused(value),
    }
}
pub(crate) fn reviewed_schedule(value: &sync::ReviewedScheduleCommit) -> Report {
    match value {
        sync::ReviewedScheduleCommit::Applied(row) => {
            accepted(value).pending(row.state.pending().is_some())
        }
        sync::ReviewedScheduleCommit::Stale | sync::ReviewedScheduleCommit::Blocked => {
            refused(value)
        }
    }
}
pub(crate) fn rejected_schedule(value: &sync::RejectedScheduleCommit) -> Report {
    match value {
        sync::RejectedScheduleCommit::Applied { stored, .. } => {
            accepted(value).pending(stored.state.pending().is_some())
        }
        sync::RejectedScheduleCommit::Stale
        | sync::RejectedScheduleCommit::Blocked
        | sync::RejectedScheduleCommit::Invalid(_) => refused(value),
    }
}
pub(crate) fn conditional(value: &bool) -> Report {
    // false also includes a previously reviewed serial or existing conflict;
    // the bool alone does not prove this submitted payload was preserved.
    if *value {
        accepted(value)
    } else {
        refused(value)
    }
}
