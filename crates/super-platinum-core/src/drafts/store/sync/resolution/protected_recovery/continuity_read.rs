//! Explicit reconciliation admission/completion, never an automatic read loop.
use super::*;
use crate::drafts::sync::reconciliation::resolve_reviewed_remote;
use serde_json::Value;

pub(super) fn record(conn: &Connection, ticket: &ReadTicket) -> Result<ReadRecord, AppError> {
    let value = inventory::reads(conn)?
        .remove(&ticket.admission.pending.id)
        .ok_or_else(|| invalid("read admission missing"))?;
    let lease = &ticket.admission.lease;
    if value.team != lease.team
        || value.lease_id != lease.id
        || value.epoch != lease.epoch
        || value.floor_ms != ticket.floor_ms
        || !matches!(
            value.state,
            ReadState::Unresolved | ReadState::SupersededUnknown
        )
        || value.enrollment_id != ticket.admission.pending.enrollment_id
        || value.sequence != ticket.admission.pending.sequence
    {
        return Err(invalid("read admission completed or mismatched"));
    }
    Ok(value)
}
pub(super) fn write_record(
    conn: &Connection,
    id: &str,
    value: &ReadRecord,
) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_continuity_reads(id,team,content) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content",params![id,value.team,serde_json::to_string(value)?])?;
    Ok(())
}
pub(super) fn unresolved(conn: &Connection, team: &str) -> Result<bool, AppError> {
    Ok(inventory::reads(conn)?
        .values()
        .any(|r| r.team == team && r.state == ReadState::Unresolved))
}

pub(super) fn reserve(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
    now_ms: u64,
) -> Result<ReadTicket, AppError> {
    let _live = lease.live_guard()?;
    storage::lease_scope(lease, &expected.review.scope)?;
    if expected.row.stored.state.acknowledged().is_none() {
        return Err(invalid(
            "positive reconciliation requires a known remote identity",
        ));
    }
    let state = &expected.row.stored.state;
    if state.pending().is_some()
        || state.consumption().is_some()
        || state.blocked_observation().is_some()
        || !matches!(
            state.workflow(),
            Workflow::Idle | Workflow::Conflict { pending: None, .. }
        )
        || codec::decode(
            &expected.review.scope.key,
            &state.acknowledged().unwrap().snapshot,
        )
        .is_err()
        || state
            .desired()
            .is_some_and(|raw| codec::decode(&expected.review.scope.key, raw).is_err())
    {
        return Err(invalid(
            "owner requires another recovery before reconciliation",
        ));
    }
    // This is eligibility checking against an ALREADY retained baseline, not
    // invented network evidence. A stale baseline may be reconciled by a newer
    // response; unsupported clocks, identities or exhausted revisions may not.
    if !matches!(
        resolve_reviewed_remote(
            state,
            state.acknowledged().unwrap(),
            state.desired().cloned()
        ),
        Ok(_) | Err(crate::drafts::sync::reconciliation::ReconcileError::StaleRemote)
    ) {
        return Err(invalid("retained baseline cannot be reconciled"));
    }
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    storage::exact(&tx, expected)?;
    inventory::acknowledged(&tx, expected)?;
    let mut current = storage::team(&tx, &lease.team)?;
    if !lease.matches(&current)
        || expected.row.ownership.verified_epoch == current.epoch
        || now_ms
            < current
                .read_retry_at_ms
                .max(recovery::deadline(&tx, &lease.team)?)
        || recovery::unresolved(&tx, &lease.team)?
        || unresolved(&tx, &lease.team)?
    {
        return Err(invalid(
            "reconciliation unavailable, already verified, or cooling down",
        ));
    }
    let floor = now_ms
        .checked_add(READ_FLOOR_MS)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(|| invalid("read deadline exhausted"))?;
    let pending = next_pending(&mut current, PendingKind::Reconcile, &expected.row)?;
    let record = ReadRecord {
        team: lease.team.clone(),
        lease_id: lease.id.clone(),
        epoch: lease.epoch,
        enrollment_id: pending.enrollment_id.clone(),
        sequence: pending.sequence,
        floor_ms: floor,
        state: ReadState::Unresolved,
        retry_at_ms: floor,
        completion: None,
    };
    current.read_retry_at_ms = current.read_retry_at_ms.max(floor);
    write_record(&tx, &pending.id, &record)?;
    storage::write_team(&tx, &lease.team, &current)?;
    tx.commit()?;
    Ok(ReadTicket {
        admission: Admission {
            lease: lease.clone(),
            pending,
            expected: expected.clone(),
            observation: None,
        },
        floor_ms: floor,
    })
}

/// Only a known completed response may call this. A missing response, timeout,
/// cancellation, process restart or failed storage is NOT a completion proof.
/// An original process-retained ticket can persist a late Retry-After even after
/// replacement of its stream. It cannot release verification for any owner.
pub(super) fn finish_nonpositive(
    store: &DraftStore,
    ticket: &ReadTicket,
    failure: KnownReadFailure,
    retry_at_ms: u64,
) -> Result<(), AppError> {
    sql_number(retry_at_ms)?;
    let tx = adapter_atomic::begin(&store.0)?;
    let lease = &ticket.admission.lease;
    owner(&tx, &lease.account)?;
    adapter_schema::known_if_installed(&tx, &lease.team, retry_at_ms)?;
    let mut record = record(&tx, ticket)?;
    let mut current = storage::team(&tx, &lease.team)?;
    current.read_retry_at_ms = current
        .read_retry_at_ms
        .max(record.floor_ms)
        .max(retry_at_ms);
    if record.state == ReadState::SupersededUnknown {
        recovery::late_baseline(
            &tx,
            &ticket.admission.pending.id,
            &record,
            failure,
            retry_at_ms,
        )?;
        record.retry_at_ms = record.retry_at_ms.max(retry_at_ms);
        write_record(&tx, &ticket.admission.pending.id, &record)?;
        storage::write_team(&tx, &lease.team, &current)?;
        tx.commit()?;
        return Ok(());
    }
    record.state = ReadState::Finished;
    record.completion = Some(ReadCompletion::Nonpositive(failure));
    record.retry_at_ms = current.read_retry_at_ms;
    // A late response is never allowed to clear a replacement stream's work.
    if lease.matches(&current) && current.pending.as_ref() == Some(&ticket.admission.pending) {
        current.pending = None;
    }
    write_record(&tx, &ticket.admission.pending.id, &record)?;
    storage::write_team(&tx, &lease.team, &current)?;
    tx.commit()?;
    Ok(())
}

/// Raw response/user provenance is supplied by the future captured transport
/// adapter. This private storage method alone makes no network provenance claim.
pub(super) fn finish_positive(
    store: &DraftStore,
    ticket: &ReadTicket,
    raw: Value,
    user: &str,
    choice: &RemoteChoice,
) -> Result<(ProtectedRow, LocalDraft), AppError> {
    let lease = &ticket.admission.lease;
    let _live = lease.live_guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    let mut continuity = storage::team(&tx, &lease.team)?;
    check_finish(&continuity, &ticket.admission)?;
    if ticket.admission.pending.kind != PendingKind::Reconcile {
        return Err(invalid("wrong read admission kind"));
    }
    let mut record = record(&tx, ticket)?;
    let current = storage::exact(&tx, &ticket.admission.expected)?;
    inventory::acknowledged(&tx, &current)?;
    // Plain RemoteReview does not carry incarnation/epoch; exact above is
    // mandatory and is kept in the SAME transaction as semantic settlement.
    let reviewed = RemoteReview::prepare(current.review.clone(), raw, user)?;
    let (local, desired) = selected(&current, reviewed.remote(), choice)?;
    let scope = &current.review.scope;
    let state = resolve_reviewed_remote(&current.row.stored.state, reviewed.remote(), desired)
        .map_err(|_| invalid("positive reconciliation refused"))?;
    let mut row = successor(&current.row, state)?;
    row.ownership.verified_epoch = continuity.epoch;
    row.ownership.basis = Basis::Remote;
    storage::write_row(&tx, &row)?;
    write_local(&tx, &scope.key, &local)?;
    let key = serde_json::to_string(&scope.key)?;
    for alternate in &current.review.alternates {
        tx.execute("INSERT INTO composer_draft_reviewed_writers(destination,writer_id,revision) VALUES(?1,?2,?3) ON CONFLICT(destination,writer_id) DO UPDATE SET revision=MAX(revision,excluded.revision)",params![key,alternate.writer_id,sql_number(alternate.revision)?])?;
    }
    tx.execute(
        "DELETE FROM composer_draft_blocked WHERE destination=?1",
        [&key],
    )?;
    tx.execute(
        "DELETE FROM composer_draft_alternates WHERE destination=?1",
        [&key],
    )?;
    continuity.pending = None;
    continuity.read_retry_at_ms = continuity.read_retry_at_ms.max(record.floor_ms);
    record.state = ReadState::Finished;
    record.completion = Some(ReadCompletion::Positive);
    record.retry_at_ms = continuity.read_retry_at_ms;
    write_record(&tx, &ticket.admission.pending.id, &record)?;
    storage::write_team(&tx, &lease.team, &continuity)?;
    tx.commit()?;
    Ok((row, local))
}

/// Shared human-choice projection; storage authority belongs to the caller's
/// exact protected transaction, never to the plain RemoteReview alone.
pub(super) fn selected(
    current: &ProtectedReview,
    remote: &crate::drafts::sync::RemoteDraft,
    choice: &RemoteChoice,
) -> Result<(LocalDraft, Option<crate::drafts::sync::Snapshot>), AppError> {
    let scope = &current.review.scope;
    let (local, desired) = match choice {
        RemoteChoice::Remote => {
            let projection = codec::decode(&scope.key, &remote.snapshot)
                .map_err(|_| invalid("unsupported remote content"))?;
            (
                LocalDraft {
                    composer: projection.composer().clone(),
                    attachments: vec![],
                },
                Some(remote.snapshot.clone()),
            )
        }
        RemoteChoice::Local(choice) => {
            let local = current
                .review
                .chosen_local(choice)
                .cloned()
                .ok_or_else(|| invalid("reviewed local version vanished"))?;
            if !local.attachments.is_empty() {
                return Err(invalid("unsupported local content"));
            }
            let retained = current
                .row
                .stored
                .state
                .desired()
                .and_then(|s| codec::decode(&scope.key, s).ok());
            let desired = if let Some(projection) = retained.filter(|p| {
                p.composer().text == local.composer.text
                    && p.composer().reply_broadcast == local.composer.reply_broadcast
            }) {
                Some(projection.raw_snapshot().clone())
            } else if local.composer.text.is_empty() {
                None
            } else {
                Some(
                    codec::encode_composer(&scope.key, &local.composer)
                        .map_err(|_| invalid("unsupported local content"))?,
                )
            };
            (local, desired)
        }
    };
    Ok((local, desired))
}
