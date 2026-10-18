//! DISCONNECTED transactional storage. Activation exists only in candidate tests.
use super::*;

pub(super) fn read_row(conn: &Connection, scope: &Scope) -> Result<ProtectedRow, AppError> {
    let (revision, format, raw): (i64, i64, String) = conn.query_row(
        "SELECT storage_revision,format_version,state FROM composer_draft_sync WHERE destination=?1",
        [serde_json::to_string(&scope.key)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    wire::decode(scope, revision, format, &raw)
}
pub(super) fn write_row(conn: &Connection, row: &ProtectedRow) -> Result<(), AppError> {
    let changed = conn.execute("UPDATE composer_draft_sync SET storage_revision=?2,format_version=7,state=?3 WHERE destination=?1",
        params![serde_json::to_string(&row.stored.state.scope().key)?,sql_number(row.stored.revision)?,wire::encode(row)?])?;
    if changed != 1 {
        return Err(invalid("protected row disappeared"));
    }
    Ok(())
}
pub(super) fn review_row(conn: &Connection, scope: &Scope) -> Result<ProtectedReview, AppError> {
    let row = read_row(conn, scope)?;
    let key = serde_json::to_string(&scope.key)?;
    let local: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let blocked: Option<(String, String)> = conn
        .query_row(
            "SELECT token,content FROM composer_draft_blocked WHERE destination=?1",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let review = LocalReview {
        scope: scope.clone(),
        local: local
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default(),
        stored: Some(row.stored.clone()),
        blocked: blocked
            .map(|(token, raw)| -> Result<_, AppError> {
                Ok(BlockedLocal {
                    token,
                    draft: serde_json::from_str(&raw)?,
                })
            })
            .transpose()?,
        alternates: read_alternates(conn)?
            .remove(&scope.key)
            .unwrap_or_default(),
    };
    Ok(ProtectedReview { review, row })
}
pub(super) fn team(conn: &Connection, key: &str) -> Result<Team, AppError> {
    if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid("invalid continuity team"));
    }
    let raw: String = conn.query_row(
        "SELECT content FROM composer_draft_continuity WHERE team=?1",
        [key],
        |r| r.get(0),
    )?;
    let shape: serde_json::Value = serde_json::from_str(&raw)?;
    for field in ["lease_id", "pending", "last_interrupted_id", "gap_reason"] {
        if shape.get(field).is_none() {
            return Err(invalid("missing continuity guard field"));
        }
    }
    let state: Team = wire::strict(&raw)?;
    sql_number(state.epoch)?;
    sql_number(state.sequence)?;
    sql_number(state.read_retry_at_ms)?;
    if state.epoch == 0
        || (state.mode == Mode::Open) != state.lease_id.is_some()
        || (state.mode == Mode::Gap && state.pending.is_some())
        || state
            .last_interrupted_id
            .as_ref()
            .is_some_and(|id| !valid_uuid(id))
        || state.lease_id.as_ref().is_some_and(|id| !valid_uuid(id))
        || state.pending.as_ref().is_some_and(|p| {
            !valid_uuid(&p.id)
                || !valid_uuid(&p.enrollment_id)
                || p.sequence != state.sequence
                || p.sequence == 0
                || state.last_interrupted_id.as_ref() == Some(&p.id)
        })
    {
        return Err(invalid("invalid durable continuity"));
    }
    inventory::validate_team_reads(conn, key, &state)?;
    Ok(state)
}
pub(super) fn write_team(conn: &Connection, key: &str, value: &Team) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_continuity(team,content) VALUES(?1,?2) ON CONFLICT(team) DO UPDATE SET content=excluded.content",params![key,serde_json::to_string(value)?])?;
    Ok(())
}
pub(super) fn exact(
    conn: &Connection,
    expected: &ProtectedReview,
) -> Result<ProtectedReview, AppError> {
    let current = review_row(conn, &expected.review.scope)?;
    if !expected.matches(&current) {
        return Err(invalid("protected inventory changed"));
    }
    Ok(current)
}
pub(super) fn lease_scope(lease: &LiveLease, scope: &Scope) -> Result<(), AppError> {
    if lease.account != scope.account_id || lease.team != scope.key.team {
        return Err(invalid("continuity owner mismatch"));
    }
    Ok(())
}

/// Private format installation used by validated owned migration/shadow inspection.
pub(super) fn activate(store: &DraftStore, account: &str) -> Result<(), AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    tx.execute_batch("CREATE TABLE composer_draft_continuity(team TEXT PRIMARY KEY,content TEXT NOT NULL);
        CREATE TABLE composer_draft_continuity_reads(id TEXT PRIMARY KEY,team TEXT NOT NULL,content TEXT NOT NULL);
        CREATE TABLE composer_draft_continuity_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=7));
        INSERT INTO composer_draft_continuity_format VALUES(1,7);")?;
    let rows: Vec<(String, i64, i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT destination,storage_revision,format_version,state FROM composer_draft_sync",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?
    };
    for (key, revision, format, raw) in rows {
        let scope = Scope {
            account_id: account.into(),
            key: serde_json::from_str(&key)?,
        };
        let previous = super::super::super::decode(&scope, revision, format, raw)?;
        let row = ProtectedRow {
            stored: Stored {
                revision: next_revision(Some(&previous))?,
                state: previous.state,
            },
            ownership: Ownership {
                enrollment_id: uuid::Uuid::new_v4().to_string(),
                verified_epoch: 0,
                basis: Basis::Unverified,
                origin: Origin::Imported,
            },
        };
        write_row(&tx, &row)?;
        // Multiple owners in one team share its continuity fence.
        tx.execute(
            "INSERT OR IGNORE INTO composer_draft_continuity(team,content) VALUES(?1,?2)",
            params![scope.key.team, serde_json::to_string(&Team::gap())?],
        )?;
    }
    tx.execute_batch("CREATE TRIGGER composer_draft_require_v7_insert BEFORE INSERT ON composer_draft_sync WHEN NEW.format_version != 7 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;
        CREATE TRIGGER composer_draft_require_v7_update BEFORE UPDATE ON composer_draft_sync WHEN NEW.format_version != 7 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;")?;
    recovery::schema(&tx)?;
    archive::schema(&tx)?;
    migration::import_checks(&tx, account)?;
    tx.commit()?;
    Ok(())
}

/// The caller explicitly takes over/reconnects. This revokes any persisted lease;
/// it is not evidence that another process died. No loaded identifier is a permit.
pub(super) fn open_stream(
    store: &DraftStore,
    account: &str,
    key: &str,
    expected: &Team,
) -> Result<LiveLease, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    let mut current = team(&tx, key)?;
    if current != *expected {
        return Err(invalid("continuity changed before takeover"));
    }
    current.epoch = increment(current.epoch)?;
    if let Some(pending) = current.pending.take() {
        current.last_interrupted_id = Some(pending.id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    current.lease_id = Some(id.clone());
    current.mode = Mode::Open;
    current.gap_reason = Some("new stream requires per-owner verification".into());
    write_team(&tx, key, &current)?;
    tx.commit()?;
    Ok(LiveLease {
        account: account.into(),
        team: key.into(),
        id,
        epoch: current.epoch,
        live: Arc::new(Mutex::new(true)),
        revoked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

pub(super) fn mark_gap(
    store: &DraftStore,
    lease: &LiveLease,
    reason: &str,
) -> Result<(), AppError> {
    // Revoke the shared atomic latch before fallible SQLite work. An already
    // admitted transaction may commit; the facade withholds its capability after
    // commit if revoked. Revocation never waits on the private admission mutex.
    lease.revoke()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    let mut current = team(&tx, &lease.team)?;
    if !lease.matches(&current) {
        return Err(invalid("superseded continuity lease"));
    }
    current.epoch = increment(current.epoch)?;
    current.mode = Mode::Gap;
    current.lease_id = None;
    current.gap_reason = Some(reason.into());
    if let Some(pending) = current.pending.take() {
        current.last_interrupted_id = Some(pending.id);
    }
    write_team(&tx, &lease.team, &current)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn reserve_observation(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
    event: &crate::drafts::sync::DraftObservation,
) -> Result<Admission, AppError> {
    let _live = lease.live_guard()?;
    lease_scope(lease, &expected.review.scope)?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    exact(&tx, expected)?;
    inventory::route(&tx, expected, event)?;
    let mut current = team(&tx, &lease.team)?;
    if !lease.matches(&current) {
        return Err(invalid("stale source lease"));
    }
    let pending = next_pending(&mut current, PendingKind::Observation, &expected.row)?;
    write_team(&tx, &lease.team, &current)?;
    tx.commit()?;
    Ok(Admission {
        lease: lease.clone(),
        pending,
        expected: expected.clone(),
        observation: Some(event.clone()),
    })
}

pub(super) fn finish_observation(
    store: &DraftStore,
    admission: &Admission,
    event: &crate::drafts::sync::DraftObservation,
) -> Result<ProtectedRow, AppError> {
    let _live = admission.lease.live_guard()?;
    if admission.pending.kind != PendingKind::Observation {
        return Err(invalid("wrong admission kind"));
    }
    if admission.observation.as_ref() != Some(event) {
        return Err(invalid("admitted event changed"));
    }
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &admission.lease.account)?;
    let mut current = team(&tx, &admission.lease.team)?;
    check_finish(&current, admission)?;
    let before = exact(&tx, &admission.expected)?;
    inventory::route(&tx, &before, event)?;
    if before.review.blocked.is_some() || !before.review.alternates.is_empty() {
        return Err(invalid("unreviewed local versions"));
    }
    let mut state = before.row.stored.state.clone();
    state
        .observe_event(event)
        .map_err(|_| invalid("invalid observation"))?;
    if state.desired() != before.row.stored.state.desired() {
        let local = project(&state)?;
        write_local(&tx, &before.review.scope.key, &local)?;
    }
    let after = successor(&before.row, state)?;
    write_row(&tx, &after)?;
    current.pending = None;
    write_team(&tx, &admission.lease.team, &current)?;
    tx.commit()?;
    Ok(after)
}

pub(super) fn project(state: &DraftSync) -> Result<LocalDraft, AppError> {
    match state.desired() {
        None => Ok(LocalDraft::default()),
        Some(snapshot) => Ok(LocalDraft {
            composer: codec::decode(&state.scope().key, snapshot)
                .map_err(|_| invalid("unsupported projection"))?
                .composer()
                .clone(),
            attachments: vec![],
        }),
    }
}

/// Local preservation remains available during Gap. No remote admission, no
/// changed metadata supplied by the caller, no loss of blocked/alternate rows.
pub(super) fn checkpoint(
    store: &DraftStore,
    expected: &ProtectedReview,
    local: &LocalDraft,
    direct: Option<&crate::drafts::sync::ConsumeIntent>,
) -> Result<ProtectedRow, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = exact(&tx, expected)?;
    let desired = if !local.attachments.is_empty() {
        Err(codec::Unsupported::Files)
    } else if direct.is_none() && current.row.stored.state.desired().is_some() {
        let raw = current.row.stored.state.desired().unwrap();
        codec::decode(&expected.review.scope.key, raw).and_then(|projection| {
            if !projection.composer().text.is_empty() && local.composer.text.is_empty() {
                Ok(None)
            } else {
                projection.snapshot_for(&local.composer).map(Some)
            }
        })
    } else if local.composer.text.is_empty() {
        Ok(None)
    } else {
        codec::encode_composer(&expected.review.scope.key, &local.composer).map(Some)
    };
    let mut state = current.row.stored.state.clone();
    if let Some(intent) = direct {
        state
            .checkpoint_direct_local(intent, desired.ok().flatten())
            .map_err(|_| invalid("direct-local checkpoint authority changed"))?;
    } else {
        state.edit(
            &expected.review.scope,
            desired.map_err(|_| invalid("unsupported checkpoint"))?,
        );
    }
    let row = successor(&current.row, state)?;
    write_local(&tx, &expected.review.scope.key, local)?;
    write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}

/// Persist an exact already-admitted Create/Update/Delete receipt during a Gap.
/// The pure reducer still validates identity, frozen operation and version. No
/// arbitrary replacement state and no new immutable operation can enter here.
pub(super) enum Receipt {
    Active(crate::drafts::sync::RemoteDraft),
    Deleted,
    Sent {
        client_msg_id: String,
        remote_id: Option<String>,
        message_ts: String,
    },
    Scheduled {
        remote: crate::drafts::sync::RemoteDraft,
        post_at: i64,
    },
}
pub(super) fn receipt(
    store: &DraftStore,
    expected: &ProtectedReview,
    operation: &Operation,
    receipt: Receipt,
) -> Result<ProtectedRow, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = exact(&tx, expected)?;
    if current.row.stored.state.pending() != Some(operation) {
        return Err(invalid("receipt operation changed"));
    }
    let mut state = current.row.stored.state.clone();
    let applied = match receipt {
        Receipt::Active(remote) => state.confirm(&operation.id, remote),
        Receipt::Deleted => operation
            .base
            .as_ref()
            .is_some_and(|base| state.confirm_deleted(&operation.scope, &operation.id, &base.id)),
        Receipt::Sent {
            client_msg_id,
            remote_id,
            message_ts,
        } => state.confirm_sent(
            &operation.scope,
            &operation.id,
            &client_msg_id,
            remote_id.as_deref(),
            &message_ts,
        ),
        Receipt::Scheduled { remote, post_at } => {
            state.confirm_scheduled(&operation.id, remote, post_at)
        }
    };
    if !applied {
        return Err(invalid("receipt not applicable"));
    }
    let row = successor(&current.row, state)?;
    // A receipt never replaces canonical local input. Reducer retains newer desired.
    write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}

pub(super) fn prepare(
    store: &DraftStore,
    lease: &LiveLease,
    expected: &ProtectedReview,
) -> Result<(ProtectedRow, Operation), AppError> {
    let _live = lease.live_guard()?;
    lease_scope(lease, &expected.review.scope)?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &lease.account)?;
    let current = exact(&tx, expected)?;
    let continuity = team(&tx, &lease.team)?;
    if !lease.matches(&continuity)
        || continuity.pending.is_some()
        || current.row.ownership.verified_epoch != continuity.epoch
        || current.review.blocked.is_some()
        || !current.review.alternates.is_empty()
    {
        return Err(invalid("protected mutation not admitted"));
    }
    facade::admitted(&tx, lease, &current)?;
    let mut state = current.row.stored.state.clone();
    let operation = state
        .begin_operation(&expected.review.scope)
        .ok_or_else(|| invalid("reducer blocks mutation"))?;
    let row = successor(&current.row, state)?;
    write_row(&tx, &row)?;
    tx.commit()?;
    Ok((row, operation))
}

pub(super) fn release(
    store: &DraftStore,
    expected: &ProtectedReview,
) -> Result<LocalDraft, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = exact(&tx, expected)?;
    api::slash_guard(&tx, &current.review.scope)?;
    if current.row.stored.state.has_remote_identity_history()
        || current.review.blocked.is_some()
        || !current.review.alternates.is_empty()
    {
        return Err(invalid(
            "remote history requires a future ownership archive",
        ));
    }
    let continuity = team(&tx, &expected.review.scope.key.team)?;
    if continuity
        .pending
        .as_ref()
        .is_some_and(|p| p.enrollment_id == current.row.ownership.enrollment_id)
    {
        return Err(invalid("owner has unresolved admission"));
    }
    tx.execute(
        "DELETE FROM composer_draft_sync WHERE destination=?1",
        [serde_json::to_string(&expected.review.scope.key)?],
    )?;
    tx.commit()?;
    Ok(current.review.local)
}

/// Candidate test-only reenrollment, not a production migration/adoption API.
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn enroll_identity_free(
    store: &DraftStore,
    scope: &Scope,
    expected_local: &LocalDraft,
    state: &DraftSync,
) -> Result<ProtectedRow, AppError> {
    validate(scope, state)?;
    if state.has_remote_identity_history() {
        return Err(invalid("test enrollment is identity-free only"));
    }
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &scope.account_id)?;
    let key = serde_json::to_string(&scope.key)?;
    let present: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_sync WHERE destination=?1)",
        [&key],
        |r| r.get(0),
    )?;
    let blocked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_blocked WHERE destination=?1)",
        [&key],
        |r| r.get(0),
    )?;
    if present || blocked || read_alternates(&tx)?.contains_key(&scope.key) {
        return Err(invalid("enrollment inventory changed"));
    }
    let raw: Option<String> = tx
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let local: LocalDraft = raw
        .map(|v| serde_json::from_str(&v))
        .transpose()?
        .unwrap_or_default();
    if !same_local(&local, expected_local) || !same_local(&project(state)?, &local) {
        return Err(invalid("enrollment local changed"));
    }
    let row = ProtectedRow {
        stored: Stored {
            revision: 1,
            state: state.clone(),
        },
        ownership: Ownership {
            enrollment_id: uuid::Uuid::new_v4().to_string(),
            verified_epoch: 0,
            basis: Basis::Unverified,
            origin: Origin::Imported,
        },
    };
    tx.execute("INSERT INTO composer_draft_sync(destination,storage_revision,format_version,state) VALUES(?1,1,7,?2)",params![key,wire::encode(&row)?])?;
    tx.commit()?;
    Ok(row)
}
