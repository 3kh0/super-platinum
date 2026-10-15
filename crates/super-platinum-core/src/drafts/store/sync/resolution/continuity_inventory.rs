//! Bounded complete transactional inventories for the disconnected first slice.
//! Not the eventual cached/invalidated production index or an ingress queue.
use super::*;
use crate::drafts::store::sync::Loaded;
use crate::drafts::sync::{
    DraftObservation, ObservationKind,
    owned_index::{Identity, IdentityKind, OwnedDraftIndex},
};
use std::collections::BTreeMap;

const MAX_ROWS: i64 = 4096;
const MAX_BYTES: i64 = 16 * 1024 * 1024;
fn bound(conn: &Connection, query: &str) -> Result<(), AppError> {
    let (count, bytes): (i64, i64) = conn.query_row(query, [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if count > MAX_ROWS || bytes > MAX_BYTES {
        return Err(invalid("protected inventory exceeds candidate bound"));
    }
    Ok(())
}

pub(super) fn route(
    conn: &Connection,
    expected: &ProtectedReview,
    event: &DraftObservation,
) -> Result<(), AppError> {
    if event.scope != expected.review.scope {
        return Err(invalid("event owner mismatch"));
    }
    let kind = if matches!(event.kind, ObservationKind::Deleted) {
        IdentityKind::Deleted
    } else {
        IdentityKind::Active
    };
    resolve(
        conn,
        expected,
        Identity {
            draft_id: &event.draft_id,
            client_msg_id: Some(&event.client_msg_id),
            kind,
        },
    )
}
pub(super) fn acknowledged(conn: &Connection, expected: &ProtectedReview) -> Result<(), AppError> {
    let base = expected
        .row
        .stored
        .state
        .acknowledged()
        .ok_or_else(|| invalid("no acknowledged owner"))?;
    resolve(
        conn,
        expected,
        Identity {
            draft_id: &base.id,
            client_msg_id: Some(&base.client_msg_id),
            kind: IdentityKind::Active,
        },
    )
}
fn resolve(
    conn: &Connection,
    expected: &ProtectedReview,
    identity: Identity<'_>,
) -> Result<(), AppError> {
    bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(destination AS BLOB))+LENGTH(CAST(state AS BLOB))),0) FROM composer_draft_sync",
    )?;
    let mut stmt = conn.prepare(
        "SELECT destination,storage_revision,format_version,state FROM composer_draft_sync",
    )?;
    let mut synced = BTreeMap::new();
    for entry in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (key, revision, format, raw) = entry?;
        let key: DraftKey = serde_json::from_str(&key)?;
        let scope = Scope {
            account_id: expected.review.scope.account_id.clone(),
            key: key.clone(),
        };
        let row = wire::decode(&scope, revision, format, &raw)?;
        if key.team == expected.review.scope.key.team
            && key != expected.review.scope.key
            && row
                .stored
                .state
                .continuity_history_claims(identity.draft_id, identity.client_msg_id)
        {
            return Err(invalid(
                "another protected destination retains this historical identity",
            ));
        }
        if key == expected.review.scope.key && !row.matches(&expected.row) {
            return Err(invalid("protected routed row changed"));
        }
        if synced.insert(key, row.stored).is_some() {
            return Err(invalid("duplicate semantic destination"));
        }
    }
    let loaded = Loaded {
        local: Vec::new(),
        synced,
        blocked_local: BTreeMap::new(),
        uncommitted_local: BTreeMap::new(),
        local_alternates: BTreeMap::new(),
    };
    let index = OwnedDraftIndex::from_loaded(&expected.review.scope.account_id, &loaded)
        .map_err(|_| invalid("invalid owned inventory"))?;
    let candidate = index
        .resolve(
            &expected.review.scope.account_id,
            &expected.review.scope.key.team,
            identity,
        )
        .map_err(|_| invalid("ambiguous or unsupported owned identity"))?
        .ok_or_else(|| invalid("unowned event"))?;
    if candidate.scope() != &expected.review.scope {
        return Err(invalid("routed destination changed"));
    }
    candidate
        .revalidate(&index, Some(&expected.row.stored))
        .map_err(|_| invalid("routed row changed"))?;
    Ok(())
}

pub(super) fn reads(conn: &Connection) -> Result<BTreeMap<String, ReadRecord>, AppError> {
    bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_continuity_reads",
    )?;
    let mut stmt = conn.prepare("SELECT id,team,content FROM composer_draft_continuity_reads")?;
    let mut records = BTreeMap::new();
    for entry in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = entry?;
        let shape: serde_json::Value = serde_json::from_str(&raw)?;
        if shape.get("completion").is_none() {
            return Err(invalid("missing read completion guard"));
        }
        let record: ReadRecord = serde_json::from_str(&raw)?;
        for number in [
            record.epoch,
            record.sequence,
            record.floor_ms,
            record.retry_at_ms,
        ] {
            sql_number(number)?;
        }
        if !valid_uuid(&id)
            || !valid_uuid(&record.lease_id)
            || !valid_uuid(&record.enrollment_id)
            || record.team != team
            || team.is_empty()
            || team.chars().any(|c| c.is_whitespace() || c.is_control())
            || record.epoch == 0
            || record.sequence == 0
            || record.floor_ms < READ_FLOOR_MS
            || record.retry_at_ms < record.floor_ms
            || match record.state {
                ReadState::Unresolved => {
                    record.completion.is_some() || record.retry_at_ms != record.floor_ms
                }
                ReadState::Finished => record.completion.is_none(),
            }
        {
            return Err(invalid("invalid durable read record"));
        }
        let owner_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM composer_draft_continuity WHERE team=?1)",
            [&team],
            |r| r.get(0),
        )?;
        if !owner_exists {
            return Err(invalid("read record has no team continuity owner"));
        }
        records.insert(id, record);
    }
    Ok(records)
}

pub(super) fn validate_team_reads(
    conn: &Connection,
    key: &str,
    state: &Team,
) -> Result<(), AppError> {
    let records = reads(conn)?;
    for (id, record) in records.iter().filter(|(_, r)| r.team == key) {
        if record.retry_at_ms > state.read_retry_at_ms
            || record.epoch > state.epoch
            || record.sequence > state.sequence
        {
            return Err(invalid("read continuity counters disagree"));
        }
        if record.state == ReadState::Unresolved
            && state.lease_id.as_ref() == Some(&record.lease_id)
        {
            if state.epoch != record.epoch
                || state.pending.as_ref().is_none_or(|p| {
                    p.id != *id
                        || p.kind != PendingKind::Reconcile
                        || p.enrollment_id != record.enrollment_id
                        || p.sequence != record.sequence
                })
            {
                return Err(invalid("live unresolved read lacks its admission"));
            }
        }
    }
    if let Some(pending) = state
        .pending
        .as_ref()
        .filter(|p| p.kind == PendingKind::Reconcile)
    {
        let record = records
            .get(&pending.id)
            .ok_or_else(|| invalid("read admission record missing"))?;
        if record.team != key
            || record.state != ReadState::Unresolved
            || state.lease_id.as_ref() != Some(&record.lease_id)
            || record.epoch != state.epoch
            || record.enrollment_id != pending.enrollment_id
            || record.sequence != pending.sequence
        {
            return Err(invalid("read admission record disagrees"));
        }
    }
    if state
        .pending
        .as_ref()
        .is_some_and(|p| p.kind == PendingKind::Observation && records.contains_key(&p.id))
    {
        return Err(invalid("observation reuses a read ticket identity"));
    }
    Ok(())
}
