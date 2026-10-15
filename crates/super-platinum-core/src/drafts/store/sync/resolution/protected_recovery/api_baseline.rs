//! Persist completed reads before presenting choices. A response is history;
//! applying it still requires the original live stream and full native inventory.
use super::*;
use crate::drafts::sync::{RemoteDraft, reconciliation::resolve_reviewed_remote};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    version: u32,
    id: String,
    scope: Scope,
    enrollment: String,
    lease: String,
    epoch: u64,
    sequence: u64,
    original: views::InventoryRecord,
    response: BaselineResponse,
    #[serde(deserialize_with = "required_option")]
    remote: Option<RemoteDraft>,
    consumed: bool,
    #[serde(deserialize_with = "required_option")]
    applied: Option<(Choice, views::InventoryRecord)>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
enum Choice {
    Remote,
    Current,
    Blocked(String),
    Alternate(String, u64),
}
fn choice_key(choice: &RemoteChoice) -> Choice {
    match choice {
        RemoteChoice::Remote => Choice::Remote,
        RemoteChoice::Local(LocalChoice::Current) => Choice::Current,
        RemoteChoice::Local(LocalChoice::Blocked { token }) => Choice::Blocked(token.clone()),
        RemoteChoice::Local(LocalChoice::Alternate {
            writer_id,
            revision,
        }) => Choice::Alternate(writer_id.clone(), *revision),
    }
}
fn read(conn: &Connection, id: &str) -> Result<Option<Evidence>, AppError> {
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_baseline_evidence",
    )?;
    let raw: Option<(String, String)> = conn
        .query_row(
            "SELECT team,content FROM composer_draft_baseline_evidence WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    raw.map(|(team, raw)| {
        let shape: serde_json::Value = wire::strict(&raw)?;
        if shape.get("remote").is_none() {
            return Err(invalid("missing retained baseline guard"));
        }
        let evidence: Evidence = wire::strict(&raw)?;
        let original = evidence.original.restore()?;
        if evidence.version != 1
            || evidence.id != id
            || !valid_uuid(id)
            || evidence.scope != *original.scope()
            || evidence.scope.key.team != team
            || evidence.enrollment
                != original
                    .row()
                    .ok_or_else(|| invalid("baseline owner missing"))?
                    .lineage()
                    .enrollment()
            || !valid_uuid(&evidence.lease)
            || evidence.epoch == 0
            || evidence.sequence == 0
            || evidence.consumed && evidence.remote.is_none()
            || evidence.consumed != evidence.applied.is_some()
        {
            return Err(invalid("invalid retained baseline identity"));
        }
        if let Some((_, applied)) = &evidence.applied {
            let applied = applied.restore()?;
            if applied.scope() != &evidence.scope
                || applied
                    .row()
                    .is_none_or(|r| r.lineage().enrollment() != evidence.enrollment)
            {
                return Err(invalid("baseline result owner changed"));
            }
        }
        sql_number(evidence.epoch)?;
        sql_number(evidence.sequence)?;
        if let Some(remote) = &evidence.remote {
            let BaselineResponse::Positive { raw, user } = &evidence.response else {
                return Err(invalid("positive baseline lacks response"));
            };
            let reviewed = RemoteReview::prepare(original.review.clone(), raw.clone(), user)?;
            if reviewed.remote() != remote {
                return Err(invalid("retained baseline response changed"));
            }
        }
        Ok(evidence)
    })
    .transpose()
}
pub(super) fn validate_inventory(conn: &Connection) -> Result<(), AppError> {
    inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(content AS BLOB))+LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))),0) FROM composer_draft_baseline_evidence",
    )?;
    let mut statement = conn.prepare("SELECT id FROM composer_draft_baseline_evidence")?;
    let ids = statement
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for id in ids {
        read(conn, &id)?.ok_or_else(|| invalid("retained baseline disappeared"))?;
    }
    Ok(())
}
fn save(conn: &Connection, evidence: &Evidence) -> Result<(), AppError> {
    conn.execute("INSERT INTO composer_draft_baseline_evidence VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET content=excluded.content",params![evidence.id,evidence.scope.key.team,serde_json::to_string(evidence)?])?;
    validate_inventory(conn)
}
fn same_input(
    evidence: &Evidence,
    completion: &BaselineCompletion,
    response: &BaselineResponse,
) -> bool {
    let admission = &completion.inner.admission;
    evidence.scope == *completion.scope()
        && evidence.enrollment == completion.enrollment()
        && evidence.lease == admission.lease.id
        && evidence.epoch == admission.lease.epoch
        && evidence.sequence == admission.pending.sequence
        && evidence.response == *response
        && evidence
            .original
            .restore()
            .ok()
            .and_then(|v| views::managed(v).ok())
            .is_some_and(|r| r.same(&completion.review))
}
fn result(evidence: &Evidence, completion: &BaselineCompletion) -> BaselineResult {
    match &evidence.remote {
        Some(remote) => BaselineResult::Retained(BaselineEvidence {
            completion: completion.clone(),
            review: completion.review.clone(),
            remote: remote.clone(),
        }),
        None => BaselineResult::Nonpositive,
    }
}
pub(super) fn reserve(
    store: &DraftStore,
    proof: &Stream,
    expected: &Review,
    now: u64,
) -> Result<BaselineTicket, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    validate_inventory(&tx).map_err(|_| Error::Storage)?;
    let current = views::exact(&tx, expected)?;
    if proof.account() != current.scope().account_id || proof.team() != current.scope().key.team {
        return Err(Error::Stale);
    }
    drop(proof.0.live_guard().map_err(|_| Error::Gap)?);
    let team = storage::team(&tx, proof.team()).map_err(|_| Error::Storage)?;
    if !proof.0.matches(&team) {
        return Err(Error::Gap);
    }
    if team.pending.is_some()
        || readback::unresolved(&tx, proof.team()).map_err(|_| Error::Storage)?
        || recovery::unresolved(&tx, proof.team()).map_err(|_| Error::Storage)?
    {
        return Err(Error::Pending);
    }
    let deadline = team
        .read_retry_at_ms
        .max(recovery::deadline(&tx, proof.team()).map_err(|_| Error::Storage)?);
    if now < deadline {
        return Err(Error::Cooldown(deadline));
    }
    let inner =
        readback::reserve(store, &proof.0, &current.inner, now).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    Ok(BaselineTicket {
        inner,
        review: current,
    })
}
pub(super) fn claim(
    store: &DraftStore,
    ticket: BaselineTicket,
    now: u64,
) -> Result<BaselineClaim, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let lease = &ticket.inner.admission.lease;
    let live = lease.live_guard().map_err(|_| Error::Gap)?;
    let team = storage::team(&tx, &lease.team).map_err(|_| Error::Storage)?;
    if !lease.matches(&team) || team.pending.as_ref() != Some(&ticket.inner.admission.pending) {
        return Err(Error::Gap);
    }
    views::exact(&tx, &ticket.review)?;
    let record = readback::record(&tx, &ticket.inner).map_err(|_| Error::Storage)?;
    if record.state != ReadState::Unresolved {
        return Err(Error::Stale);
    }
    let known = adapter_schema::known(&tx, &lease.team).map_err(|_| Error::Storage)?;
    let other = recovery::deadline(&tx, &lease.team).map_err(|_| Error::Storage)?;
    let deadline = known
        .max(if team.read_retry_at_ms > ticket.inner.floor_ms {
            team.read_retry_at_ms
        } else {
            0
        })
        .max(other);
    if now < deadline {
        drop(live);
        tx.commit().map_err(|_| Error::Storage)?;
        if !lease.is_live() {
            return Err(Error::Gap);
        }
        return Ok(BaselineClaim::Deferred {
            ticket,
            until: deadline,
        });
    }
    let remote = ticket
        .review
        .row
        .state()
        .acknowledged()
        .ok_or(Error::Unsupported)?;
    let request = BaselineRequest {
        scope: ticket.review.scope().clone(),
        draft_id: remote.id.clone(),
    };
    drop(live);
    tx.commit().map_err(|_| Error::Storage)?;
    if !lease.is_live() {
        return Err(Error::Gap);
    }
    Ok(BaselineClaim::Ready {
        request,
        completion: BaselineCompletion {
            inner: Arc::new(ticket.inner),
            review: ticket.review,
        },
    })
}
pub(super) fn retain(
    store: &DraftStore,
    completion: &BaselineCompletion,
    response: BaselineResponse,
    known_until: u64,
) -> Result<BaselineResult, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    owner(&tx, &completion.scope().account_id).map_err(|_| Error::Storage)?;
    sql_number(known_until).map_err(|_| Error::Unsupported)?;
    if known_until > 0 {
        recovery::persist_deadline(&tx, completion.scope(), known_until)
            .map_err(|_| Error::Storage)?;
    }
    if let Some(previous) = read(&tx, completion.admission_id()).map_err(|_| Error::Storage)? {
        if !same_input(&previous, completion, &response) {
            faults::before_commit(faults::FaultPoint::BaselineRetention)
                .map_err(|_| Error::Storage)?;
            tx.commit().map_err(|_| Error::Storage)?;
            return Err(Error::Stale);
        }
        let result = result(&previous, completion);
        faults::before_commit(faults::FaultPoint::BaselineRetention).map_err(|_| Error::Storage)?;
        tx.commit().map_err(|_| Error::Storage)?;
        return Ok(result);
    }
    let mut record = readback::record(&tx, &completion.inner).map_err(|_| Error::Storage)?;
    let remote = match &response {
        BaselineResponse::Positive { raw, user } => {
            RemoteReview::prepare(completion.review.inner.review.clone(), raw.clone(), user)
                .ok()
                .map(|r| r.remote().clone())
        }
        BaselineResponse::Nonpositive(_) => None,
    };
    let admission = &completion.inner.admission;
    let mut team = storage::team(&tx, &admission.lease.team).map_err(|_| Error::Storage)?;
    team.read_retry_at_ms = team.read_retry_at_ms.max(record.floor_ms).max(known_until);
    record.retry_at_ms = record.retry_at_ms.max(team.read_retry_at_ms);
    if record.state == ReadState::Unresolved {
        record.state = ReadState::Finished;
        record.completion = Some(if remote.is_some() {
            ReadCompletion::RetainedPositive { version: 1 }
        } else {
            ReadCompletion::Executed {
                version: 1,
                disposition: match &response {
                    BaselineResponse::Nonpositive(issue) => *issue,
                    BaselineResponse::Positive { .. } => KnownReadFailure::Unsupported,
                },
            }
        });
        if admission.lease.matches(&team) && team.pending.as_ref() == Some(&admission.pending) {
            team.pending = None;
        }
    }
    let original = views::InventoryRecord::capture(&views::local_from_review(&completion.review))
        .map_err(|_| Error::Storage)?;
    let evidence = Evidence {
        version: 1,
        id: completion.admission_id().into(),
        scope: completion.scope().clone(),
        enrollment: completion.enrollment().into(),
        lease: admission.lease.id.clone(),
        epoch: admission.lease.epoch,
        sequence: admission.pending.sequence,
        original,
        response,
        remote,
        consumed: false,
        applied: None,
    };
    save(&tx, &evidence).map_err(|_| Error::Storage)?;
    readback::write_record(&tx, completion.admission_id(), &record).map_err(|_| Error::Storage)?;
    storage::write_team(&tx, &admission.lease.team, &team).map_err(|_| Error::Storage)?;
    let result = result(&evidence, completion);
    faults::before_commit(faults::FaultPoint::BaselineRetention).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(result)
}
pub(super) fn finish(
    store: &DraftStore,
    evidence: &BaselineEvidence,
    choice: &RemoteChoice,
) -> Result<Review, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let completion = &evidence.completion;
    let mut retained = read(&tx, evidence.admission_id())
        .map_err(|_| Error::Storage)?
        .ok_or(Error::Stale)?;
    if retained.remote.as_ref() != Some(evidence.remote())
        || !same_input(&retained, completion, &retained.response)
    {
        return Err(Error::Stale);
    }
    let lease = &completion.inner.admission.lease;
    let _live = lease.live_guard().map_err(|_| Error::Gap)?;
    let team = storage::team(&tx, &lease.team).map_err(|_| Error::Storage)?;
    if !lease.matches(&team)
        || team.pending.is_some()
        || team.sequence != completion.inner.admission.pending.sequence
    {
        return Err(Error::Gap);
    }
    if let Some((original_choice, result)) = &retained.applied {
        if *original_choice != choice_key(choice) {
            return Err(Error::Stale);
        }
        // Original receipt only: no stream/basis/composer transition is repeated.
        let result = views::managed(result.restore().map_err(|_| Error::Storage)?)?;
        faults::before_commit(faults::FaultPoint::BaselineChoice).map_err(|_| Error::Storage)?;
        tx.commit().map_err(|_| Error::Storage)?;
        if !lease.is_live() {
            return Err(Error::Gap);
        }
        return Ok(result);
    }
    let current = views::exact(&tx, &evidence.review)?;
    inventory::acknowledged(&tx, &current.inner).map_err(|_| Error::Storage)?;
    let record = inventory::reads(&tx)
        .map_err(|_| Error::Storage)?
        .remove(evidence.admission_id())
        .ok_or(Error::Stale)?;
    if record.state != ReadState::Finished
        || record.completion != Some(ReadCompletion::RetainedPositive { version: 1 })
    {
        return Err(Error::Stale);
    }
    let (local, desired) = readback::selected(&current.inner, evidence.remote(), choice)
        .map_err(|_| Error::Unsupported)?;
    let state = resolve_reviewed_remote(current.row.state(), evidence.remote(), desired)
        .map_err(|_| Error::Unsupported)?;
    let mut row = successor(&current.row.inner, state).map_err(|_| Error::Storage)?;
    row.ownership.verified_epoch = team.epoch;
    row.ownership.basis = super::super::Basis::Remote;
    storage::write_row(&tx, &row).map_err(|_| Error::Storage)?;
    write_local(&tx, &current.scope().key, &local).map_err(|_| Error::Storage)?;
    facade::retire_versions(&tx, &current.inner.review).map_err(|_| Error::Storage)?;
    retained.consumed = true;
    let result = views::review(&tx, current.scope())?;
    retained.applied = Some((
        choice_key(choice),
        views::InventoryRecord::capture(&views::local_from_review(&result))
            .map_err(|_| Error::Storage)?,
    ));
    save(&tx, &retained).map_err(|_| Error::Storage)?;
    faults::before_commit(faults::FaultPoint::BaselineChoice).map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    if !lease.is_live() {
        return Err(Error::Gap);
    }
    Ok(result)
}
