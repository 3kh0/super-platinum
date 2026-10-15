//! Typed private v7 transitions. No caller can commit arbitrary replacement state.
use super::*;
use crate::drafts::sync::{
    ConsumeIntent, ConsumeKind, RejectedScheduleChoice, RemoteDraft, Snapshot,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Outcome {
    Active(RemoteDraft),
    Deleted {
        draft_id: String,
    },
    Sent {
        client_msg_id: String,
        draft_id: Option<String>,
        ts: String,
    },
    Scheduled {
        remote: RemoteDraft,
        post_at: i64,
    },
    Rejected,
    Uncertain,
}
#[derive(Clone)]
pub(super) struct ReceiptTarget {
    pub(super) scope: Scope,
    pub(super) enrollment: String,
    pub(super) operation: Operation,
}
impl ReceiptTarget {
    pub(super) fn for_row(row: &ProtectedRow) -> Result<Self, AppError> {
        Ok(Self {
            scope: row.stored.state.scope().clone(),
            enrollment: row.ownership.enrollment_id.clone(),
            operation: row
                .stored
                .state
                .pending()
                .cloned()
                .ok_or_else(|| invalid("no frozen operation"))?,
        })
    }
}
pub(super) fn apply(state: &mut DraftSync, op: &Operation, outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Active(remote) => state.confirm(&op.id, remote.clone()),
        Outcome::Deleted { draft_id } => state.confirm_deleted(&op.scope, &op.id, draft_id),
        Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        } => state.confirm_sent(&op.scope, &op.id, client_msg_id, draft_id.as_deref(), ts),
        Outcome::Scheduled { remote, post_at } => {
            state.confirm_scheduled(&op.id, remote.clone(), *post_at)
        }
        Outcome::Rejected => state.reject(&op.scope, &op.id),
        Outcome::Uncertain => state.mark_uncertain(&op.scope, &op.id),
    }
}
pub(super) fn clean(review: &ProtectedReview) -> Result<(), AppError> {
    if review.review.blocked.is_some() || !review.review.alternates.is_empty() {
        return Err(invalid("review local versions first"));
    }
    Ok(())
}
pub(super) fn admitted(
    conn: &Connection,
    lease: &LiveLease,
    current: &ProtectedReview,
) -> Result<(), AppError> {
    api::slash_guard(conn, &current.review.scope)?;
    storage::lease_scope(lease, &current.review.scope)?;
    let team = storage::team(conn, &lease.team)?;
    clean(current)?;
    if !lease.matches(&team)
        || team.pending.is_some()
        || current.row.ownership.verified_epoch != team.epoch
        || current.row.ownership.basis == Basis::Unverified
    {
        return Err(invalid("new mutation requires current verified basis"));
    }
    if current.row.stored.state.acknowledged().is_some() {
        if current.row.ownership.basis != Basis::Remote {
            return Err(invalid("remote basis required"));
        }
        inventory::acknowledged(conn, current)?;
    } else {
        if current.row.ownership.basis != Basis::Local {
            return Err(invalid("local origin basis required"));
        }
        inventory::local_identity(conn, current)?;
    }
    Ok(())
}
pub(super) fn original_pair(review: &ProtectedReview) -> Result<(), AppError> {
    let desired = review
        .row
        .stored
        .state
        .desired()
        .ok_or_else(|| invalid("empty draft"))?;
    if !review.review.local.attachments.is_empty()
        || review.review.local.composer.text.trim().is_empty()
        || codec::decode(&review.review.scope.key, desired)
            .and_then(|p| p.snapshot_for(&review.review.local.composer))
            .as_ref()
            .ok()
            != Some(desired)
    {
        return Err(invalid("paired composer changed or unsupported"));
    }
    Ok(())
}

pub(super) fn enroll_local(
    store: &DraftStore,
    scope: &Scope,
    local: &LocalDraft,
) -> Result<ProtectedRow, AppError> {
    // Mint here, not from a caller-supplied DraftSync claiming local origin.
    let snapshot = if local.composer.text.is_empty() && local.attachments.is_empty() {
        None
    } else if local.attachments.is_empty() {
        Some(
            codec::encode_composer(&scope.key, &local.composer)
                .map_err(|_| invalid("unsupported enrollment"))?,
        )
    } else {
        return Err(invalid("unsupported enrollment"));
    };
    let mut state = DraftSync::new(scope.clone());
    state.edit(scope, snapshot);
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &scope.account_id)?;
    let key = serde_json::to_string(&scope.key)?;
    let present: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1) OR EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",[&key],|r|r.get(0))?;
    if present || read_alternates(&tx)?.contains_key(&scope.key) {
        return Err(invalid("enrollment inventory changed"));
    }
    let raw: Option<String> = tx
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let current: LocalDraft = raw
        .map(|r| serde_json::from_str(&r))
        .transpose()?
        .unwrap_or_default();
    if !same_local(&current, local) {
        return Err(invalid("enrollment local changed"));
    }
    archive::reject_claim(&tx, scope, None, Some(state.client_msg_id()))?;
    let row = ProtectedRow {
        ownership: Ownership {
            enrollment_id: uuid::Uuid::new_v4().to_string(),
            verified_epoch: 0,
            basis: Basis::Unverified,
            origin: Origin::LocalIssued {
                client_msg_id: state.client_msg_id().into(),
            },
        },
        stored: Stored { revision: 1, state },
    };
    tx.execute(
        "INSERT INTO composer_draft_sync VALUES(?1,1,7,?2)",
        params![key, wire::encode(&row)?],
    )?;
    tx.execute(
        "DELETE FROM composer_draft_release_head WHERE destination=?1",
        [&key],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO composer_draft_continuity(team,content) VALUES(?1,?2)",
        params![scope.key.team, serde_json::to_string(&Team::gap())?],
    )?;
    tx.commit()?;
    Ok(row)
}

pub(super) fn verify_local(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
) -> Result<ProtectedRow, AppError> {
    let _live = lease.live_guard()?;
    storage::lease_scope(lease, &expected.review.scope)?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    let current = storage::exact(&tx, expected)?;
    let team = storage::team(&tx, &lease.team)?;
    clean(&current)?;
    let state = &current.row.stored.state;
    if !lease.matches(&team)
        || team.pending.is_some()
        || !matches!(&current.row.ownership.origin, Origin::LocalIssued{client_msg_id} if client_msg_id == state.client_msg_id())
        || state.acknowledged().is_some()
        || state.pending().is_some()
        || state.consumption().is_some()
        || state.blocked_observation().is_some()
        || !matches!(state.workflow(), Workflow::Idle)
        || state.protected_current_identity_has_history()
    {
        return Err(invalid("cannot establish local-origin basis"));
    }
    inventory::local_identity(&tx, &current)?;
    let mut row = successor(&current.row, state.clone())?;
    row.ownership.verified_epoch = team.epoch;
    row.ownership.basis = Basis::Local;
    storage::write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}

pub(super) fn consume(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
    kind: ConsumeKind,
) -> Result<ProtectedRow, AppError> {
    let _live = lease.live_guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    admitted(&tx, lease, &current)?;
    original_pair(&current)?;
    let mut state = current.row.stored.state.clone();
    state
        .request_consume(&current.review.scope, state.local_revision(), kind)
        .map_err(|_| invalid("consume blocked"))?;
    let row = successor(&current.row, state)?;
    write_local(&tx, &current.review.scope.key, &LocalDraft::default())?;
    storage::write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}

pub(super) fn reviewed_schedule(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
    reviewed: &Snapshot,
    post_at: i64,
) -> Result<ProtectedRow, AppError> {
    let _live = lease.live_guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    admitted(&tx, lease, &current)?;
    original_pair(&current)?;
    let mut state = current.row.stored.state.clone();
    state
        .request_reviewed_schedule(&current.row.stored.state, reviewed.clone(), post_at)
        .map_err(|_| invalid("reviewed Schedule blocked"))?;
    let row = successor(&current.row, state)?;
    write_local(&tx, &current.review.scope.key, &LocalDraft::default())?;
    storage::write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}
pub(super) fn rejected_schedule(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
    choice: &RejectedScheduleChoice,
    now_secs: i64,
) -> Result<ProtectedRow, AppError> {
    let _live = lease.live_guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    admitted(&tx, lease, &current)?;
    let mut state = current.row.stored.state.clone();
    state
        .review_rejected_schedule(&current.row.stored.state, choice, now_secs)
        .map_err(|_| invalid("rejected Schedule review refused"))?;
    let row = successor(&current.row, state)?;
    storage::write_row(&tx, &row)?; // canonical bytes untouched, including opaque newer input
    tx.commit()?;
    Ok(row)
}

pub(super) enum Control {
    Clear,
    RetryConsumption(String),
    ReturnConsumption(String),
}
pub(super) fn control(
    store: &DraftStore,
    expected: &ProtectedReview,
    control: Control,
) -> Result<ProtectedRow, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    clean(&current)?;
    let mut state = current.row.stored.state.clone();
    let local = match control {
        Control::Clear => {
            state.edit(&current.review.scope, None);
            Some(LocalDraft::default())
        }
        Control::RetryConsumption(id) => {
            if !state.retry_consumption(&current.review.scope, &id) {
                return Err(invalid("not definitively rejected"));
            }
            None
        }
        Control::ReturnConsumption(id) => {
            state
                .cancel_consumption(&current.review.scope, &id)
                .map_err(|_| invalid("cannot restore over newer or uncertain input"))?;
            Some(storage::project(&state)?)
        }
    };
    let row = successor(&current.row, state)?;
    if let Some(local) = local {
        write_local(&tx, &current.review.scope.key, &local)?;
    }
    storage::write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}
pub(super) fn record(
    store: &DraftStore,
    target: &ReceiptTarget,
    outcome: &Outcome,
) -> Result<ProtectedRow, AppError> {
    record_with_cooldown(store, target, outcome, 0)
}
pub(super) fn record_with_cooldown(
    store: &DraftStore,
    target: &ReceiptTarget,
    outcome: &Outcome,
    until: u64,
) -> Result<ProtectedRow, AppError> {
    sql_number(until)?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &target.scope.account_id)?;
    let current = storage::review_row(&tx, &target.scope)?;
    if current.row.ownership.enrollment_id != target.enrollment
        || current.row.stored.state.pending() != Some(&target.operation)
    {
        return Err(invalid("receipt owner/operation replaced"));
    }
    let mut state = current.row.stored.state.clone();
    if !apply(&mut state, &target.operation, outcome) {
        return Err(invalid("invalid receipt"));
    }
    if until != 0 {
        // Unsupported status-check kinds still retain the shared known deadline.
        state.defer_check(&target.operation, until);
        recovery::persist_deadline(&tx, &target.scope, until)?;
    }
    let row = successor(&current.row, state)?;
    storage::write_row(&tx, &row)?; // preserve latest canonical/local version inventory
    tx.commit()?;
    Ok(row)
}

pub(super) fn preserve(
    store: &DraftStore,
    expected: &ProtectedReview,
    local: &LocalDraft,
    intent: Option<&ConsumeIntent>,
) -> Result<ProtectedRow, AppError> {
    storage::checkpoint(store, expected, local, intent)
}
pub(super) fn preserve_alternate(
    store: &DraftStore,
    scope: &Scope,
    local: &LocalDraft,
    writer: &LocalWriter,
    serial: u64,
) -> Result<(), AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &scope.account_id)?;
    // Preservation cannot claim current enrollment authority. Even a stale
    // writer keeps its version; future settlement reviews the exact inventory.
    preserve_conflict(&tx, scope, local)?;
    preserve_owned_alternate(&tx, scope, writer, serial, local)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn settle_local(
    store: &DraftStore,
    expected: &ProtectedReview,
    choice: &LocalChoice,
) -> Result<ProtectedRow, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    let local = current
        .review
        .chosen_local(choice)
        .cloned()
        .ok_or_else(|| invalid("local choice stale"))?;
    let mut state = current.row.stored.state.clone();
    if !same_local(&local, &current.review.local) {
        let projected = retained_local(&current.row, &local);
        if let Some(intent) = state.local_preservation_intent().cloned() {
            state
                .checkpoint_direct_local(&intent, projected.ok().flatten())
                .map_err(|_| invalid("direct local settlement refused"))?;
        } else {
            state.edit(&current.review.scope, projected?);
        }
    }
    let row = successor(&current.row, state)?;
    storage::write_row(&tx, &row)?;
    write_local(&tx, &current.review.scope.key, &local)?;
    retire_versions(&tx, &current.review)?;
    tx.commit()?;
    Ok(row)
}
fn retained_local(row: &ProtectedRow, local: &LocalDraft) -> Result<Option<Snapshot>, AppError> {
    if !local.attachments.is_empty() {
        return Err(invalid("unsupported ordinary local content"));
    }
    if let Some(raw) = row.stored.state.desired() {
        let projection = codec::decode(&row.stored.state.scope().key, raw)
            .map_err(|_| invalid("unsupported retained content"))?;
        if !projection.composer().text.is_empty() && local.composer.text.is_empty() {
            return Ok(None);
        }
        return projection
            .snapshot_for(&local.composer)
            .map(Some)
            .map_err(|_| invalid("unsupported local content"));
    }
    if local.composer.text.is_empty() {
        Ok(None)
    } else {
        codec::encode_composer(&row.stored.state.scope().key, &local.composer)
            .map(Some)
            .map_err(|_| invalid("unsupported local content"))
    }
}
pub(super) fn retire_versions(conn: &Connection, review: &LocalReview) -> Result<(), AppError> {
    let key = serde_json::to_string(&review.scope.key)?;
    for a in &review.alternates {
        conn.execute("INSERT INTO composer_draft_reviewed_writers VALUES(?1,?2,?3) ON CONFLICT(destination,writer_id) DO UPDATE SET revision=MAX(revision,excluded.revision)",params![key,a.writer_id,sql_number(a.revision)?])?;
    }
    conn.execute(
        "DELETE FROM composer_draft_blocked WHERE destination=?1",
        [&key],
    )?;
    conn.execute(
        "DELETE FROM composer_draft_alternates WHERE destination=?1",
        [&key],
    )?;
    Ok(())
}
