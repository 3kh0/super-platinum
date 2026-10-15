//! Closed protected actions. Full facade inventory validation and each private
//! reducer transition share one immediate transaction, including metadata writes.
use super::*;
use crate::drafts::sync::{ConsumeKind, RejectedScheduleChoice, Snapshot};

fn disk<T>(result: Result<T, AppError>) -> Result<T, Error> {
    result.map_err(|_| Error::Storage)
}
fn stream(conn: &Connection, proof: &Stream, scope: &Scope) -> Result<(), Error> {
    if proof.account() != scope.account_id || proof.team() != scope.key.team {
        return Err(Error::Stale);
    }
    let _guard = proof.0.live_guard().map_err(|_| Error::Gap)?;
    let team = disk(storage::team(conn, proof.team()))?;
    if !proof.0.matches(&team) {
        return Err(Error::Gap);
    }
    if team.pending.is_some() {
        return Err(Error::Pending);
    }
    Ok(())
}
fn admitted(conn: &Connection, proof: &Stream, review: &Review) -> Result<(), Error> {
    stream(conn, proof, review.scope())?;
    if review.blocked().is_some() || !review.alternates().is_empty() {
        return Err(Error::ReviewRequired);
    }
    if review.row.lineage().verified_epoch() != proof.epoch()
        || review.row.lineage().basis() == Basis::Unverified
    {
        return Err(Error::Gap);
    }
    disk(facade::admitted(conn, &proof.0, &review.inner))
}
pub(super) fn enroll(
    store: &DraftStore,
    scope: &Scope,
    expected: &LocalInventory,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    if scope != expected.scope() || expected.row().is_some() {
        return Err(Error::Stale);
    }
    if expected.uncommitted().is_some() {
        return Err(Error::Pending);
    }
    let current = disk(views::local(&tx, scope))?;
    if !current.same(expected) {
        return Err(Error::Stale);
    }
    if current.blocked().is_some() || !current.alternates().is_empty() {
        return Err(Error::ReviewRequired);
    }
    disk(facade::enroll_local(store, scope, current.local()))?;
    let result = views::review(&tx, scope)?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn stream_state(
    store: &DraftStore,
    account: &str,
    team: &str,
) -> Result<StreamState, Error> {
    disk(adapter_schema::require(&store.0))?;
    disk(owner(&store.0, account))?;
    let exists: bool = store
        .0
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM composer_draft_continuity WHERE team=?1)",
            [team],
            |r| r.get(0),
        )
        .map_err(|_| Error::Storage)?;
    if !exists {
        return Err(Error::Stale);
    }
    Ok(StreamState {
        account: account.into(),
        team: team.into(),
        inner: disk(storage::team(&store.0, team))?,
    })
}
pub(super) fn open_stream(
    store: &DraftStore,
    account: &str,
    team: &str,
    expected: &StreamState,
) -> Result<Stream, Error> {
    if expected.account != account || expected.team != team {
        return Err(Error::Stale);
    }
    lifecycle::revoke_team(account, team);
    let tx = disk(adapter_atomic::begin(&store.0))?;
    if disk(storage::team(&tx, team))? != expected.inner {
        return Err(Error::Stale);
    }
    let result = disk(storage::open_stream(store, account, team, &expected.inner))?;
    disk(tx.commit())?;
    lifecycle::register(account, team, &result.revoked);
    Ok(Stream(result))
}
pub(super) fn mark_gap(store: &DraftStore, proof: &Stream, reason: GapReason) -> Result<(), Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    // Client revokes synchronously before queueing; this is a disk-only retry.
    let reason = match reason {
        GapReason::Disconnect => "socket disconnected",
        GapReason::SourceReplaced => "socket source replaced",
        GapReason::QueueOverflow => "ordered ingress queue overflow",
        GapReason::InterruptedAdmission => "admission interrupted",
        GapReason::StorageFailure => "ingress storage failed",
        GapReason::ExplicitRecovery => "explicit continuity recovery",
    };
    disk(storage::mark_gap(store, &proof.0, reason))?;
    faults::before_commit(faults::FaultPoint::GapCommit).map_err(|_| Error::Storage)?;
    disk(tx.commit())
}
pub(super) fn verify(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    stream(&tx, proof, current.scope())?;
    disk(facade::verify_local(store, &proof.0, &current.inner))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn consume(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
    kind: ConsumeKind,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    admitted(&tx, proof, &current)?;
    disk(facade::consume(store, &proof.0, &current.inner, kind))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn reviewed_schedule(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
    snapshot: &Snapshot,
    post_at: i64,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    admitted(&tx, proof, &current)?;
    disk(facade::reviewed_schedule(
        store,
        &proof.0,
        &current.inner,
        snapshot,
        post_at,
    ))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn rejected_schedule(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
    choice: &RejectedScheduleChoice,
    now: i64,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    admitted(&tx, proof, &current)?;
    disk(facade::rejected_schedule(
        store,
        &proof.0,
        &current.inner,
        choice,
        now,
    ))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn control(
    store: &DraftStore,
    expected: &Review,
    action: Control,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    let action = match action {
        Control::Clear => facade::Control::Clear,
        Control::RetryConsumption(id) => facade::Control::RetryConsumption(id),
        Control::ReturnConsumption(id) => facade::Control::ReturnConsumption(id),
    };
    disk(facade::control(store, &current.inner, action))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn settle(
    store: &DraftStore,
    expected: &Review,
    choice: &LocalChoice,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    disk(facade::settle_local(store, &current.inner, choice))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn prepare(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
) -> Result<DispatchTicket, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    admitted(&tx, proof, &current)?;
    let (row, operation) = disk(storage::prepare(store, &proof.0, &current.inner))?;
    let receipt = Receipt(facade::ReceiptTarget {
        scope: current.scope().clone(),
        enrollment: row.ownership.enrollment_id,
        operation,
    });
    #[cfg(test)]
    faults::pause_prepared().map_err(|_| Error::Storage)?;
    disk(tx.commit())?;
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    Ok(DispatchTicket {
        stream: proof.clone(),
        receipt,
    })
}
pub(super) fn receipt(
    store: &DraftStore,
    expected: &Row,
    operation: &Operation,
) -> Result<Receipt, Error> {
    let current = disk(storage::read_row(&store.0, expected.scope()))?;
    if !expected.inner.matches(&current) || current.stored.state.pending() != Some(operation) {
        return Err(Error::Stale);
    }
    Ok(Receipt(facade::ReceiptTarget {
        scope: expected.scope().clone(),
        enrollment: current.ownership.enrollment_id,
        operation: operation.clone(),
    }))
}
pub(super) fn claim(store: &DraftStore, ticket: DispatchTicket) -> Result<ClaimedDispatch, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    stream(&tx, &ticket.stream, ticket.receipt.scope())?;
    let _live = ticket.stream.0.live_guard().map_err(|_| Error::Gap)?;
    let current = disk(storage::read_row(&tx, ticket.receipt.scope()))?;
    if current.ownership.enrollment_id != ticket.receipt.enrollment()
        || current.ownership.verified_epoch != ticket.stream.epoch()
        || current.stored.state.pending() != Some(ticket.receipt.operation())
        || !matches!(current.stored.state.workflow(), Workflow::InFlight(_))
    {
        return Err(Error::Stale);
    }
    disk(tx.commit())?;
    if !ticket.stream.is_live() {
        return Err(Error::Gap);
    }
    Ok(ClaimedDispatch(ticket.receipt))
}
pub(super) fn outcome(value: &Outcome) -> facade::Outcome {
    match value {
        Outcome::Active(remote) => facade::Outcome::Active(remote.clone()),
        Outcome::Deleted { draft_id } => facade::Outcome::Deleted {
            draft_id: draft_id.clone(),
        },
        Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        } => facade::Outcome::Sent {
            client_msg_id: client_msg_id.clone(),
            draft_id: draft_id.clone(),
            ts: ts.clone(),
        },
        Outcome::Scheduled { remote, post_at } => facade::Outcome::Scheduled {
            remote: remote.clone(),
            post_at: *post_at,
        },
        Outcome::Rejected => facade::Outcome::Rejected,
        Outcome::Uncertain => facade::Outcome::Uncertain,
    }
}
pub(super) fn record(
    store: &DraftStore,
    receipt: &Receipt,
    value: &Outcome,
    until: u64,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(owner(&tx, &receipt.scope().account_id))?;
    // Retain independent known cooldown even when the operation is stale or
    // supplied evidence fails reducer validation; never infer success from Idle.
    if until > 0 {
        disk(recovery::persist_deadline(&tx, receipt.scope(), until))?;
    }
    let id = format!("mutation:{}", receipt.operation().id);
    let input = journal::Input::Mutation(outcome(value));
    if let Some(previous) = disk(journal::inventory(&tx))?.remove(&id) {
        let result = if journal::matches(&previous, receipt, &input) {
            journal::review(&previous)?.ok_or(Error::Stale)
        } else {
            Err(Error::Stale)
        };
        faults::before_commit(faults::FaultPoint::MutationReceipt).map_err(|_| Error::Storage)?;
        disk(tx.commit())?;
        return result;
    }
    let current = disk(views::local(&tx, receipt.scope()))?;
    let failure = match current.row() {
        Some(row)
            if row.lineage().enrollment() == receipt.enrollment()
                && row.state().pending() == Some(receipt.operation()) =>
        {
            let mut probe = row.state().clone();
            if facade::apply(&mut probe, receipt.operation(), &outcome(value)) {
                None
            } else {
                Some(Error::Unsupported)
            }
        }
        _ => Some(Error::Stale),
    };
    if let Some(error) = failure {
        faults::before_commit(faults::FaultPoint::MutationReceipt).map_err(|_| Error::Storage)?;
        disk(tx.commit())?;
        return Err(error);
    }
    disk(facade::record_with_cooldown(
        store,
        &receipt.0,
        &outcome(value),
        until,
    ))?;
    let result = views::review(&tx, receipt.scope())?;
    disk(journal::save(
        &tx,
        &journal::Entry {
            version: 1,
            id,
            scope: receipt.scope().clone(),
            enrollment: receipt.enrollment().into(),
            operation: receipt.operation().clone(),
            input,
            positive_accepted: journal::positive(value),
            original: Some(
                views::InventoryRecord::capture(&views::local_from_review(&result))
                    .map_err(|_| Error::Storage)?,
            ),
        },
    ))?;

    faults::before_commit(faults::FaultPoint::MutationReceipt).map_err(|_| Error::Storage)?;
    disk(tx.commit())?;
    Ok(result)
}
pub(super) fn release(store: &DraftStore, expected: &Review) -> Result<Release, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    let result = disk(archive::release(store, &current.inner))?;
    faults::before_commit(faults::FaultPoint::ReleaseCommit).map_err(|_| Error::Storage)?;
    disk(tx.commit())?;
    Ok(Release(result))
}
pub(super) fn keep_editing(
    store: &DraftStore,
    expected: &Review,
    intent: &crate::drafts::sync::ConsumeIntent,
) -> Result<(), Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    if current.row.state().local_preservation_intent() != Some(intent) {
        return Err(Error::Stale);
    }
    disk(archive::keep_editing(store, &current.inner))?;
    disk(tx.commit())
}

/// Only the already-consumed Send may prepare beside newer durable local input.
/// The original proof is immutable. Read, full intent/enrollment comparison and
/// the existing Prepare CAS share one immediate transaction. No Review provided
/// by a caller replaces this fresh inventory and no ticket is reconstructed.
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn prepare_consumed_send(
    store: &DraftStore,
    proof: &Stream,
    consumed: &Review,
) -> Result<DispatchTicket, Error> {
    let intent = consumed.row().state().consumption().ok_or(Error::Stale)?;
    if !matches!(intent.kind, ConsumeKind::Send)
        || consumed.row().state().pending().is_some()
        || consumed.blocked().is_some()
        || consumed.uncommitted().is_some()
        || !consumed.alternates().is_empty()
    {
        return Err(Error::Stale);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::review(&tx, consumed.scope())?;
    if !consumed.row().same_enrollment(current.row())
        || current.row().state().consumption() != Some(intent)
        || current.row().state().pending().is_some()
        || current.blocked().is_some()
        || current.uncommitted().is_some()
        || !current.alternates().is_empty()
    {
        return Err(Error::Stale);
    }
    // Uses the unchanged admitted/source/live/SQLite preparation checks inside
    // this outer transaction; its nested savepoint cannot commit independently.
    let prepared = prepare(store, proof, &current)?;
    disk(tx.commit())?;
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    Ok(prepared)
}
