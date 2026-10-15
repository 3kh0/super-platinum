//! Explicit recovery reads, independent of a live realtime socket. Never HTTP.
use super::*;
#[path = "recovery_model.rs"]
mod model;
use crate::drafts::sync::CheckTicket;
pub(super) use model::*;

pub(super) struct Permit {
    account: String,
    team: String,
    generation: String,
    epoch: u64,
    live: Arc<Mutex<bool>>,
    pub(super) revoked: Arc<std::sync::atomic::AtomicBool>,
}
pub(super) struct Ticket {
    pub(super) attempt: Attempt,
}
#[derive(Clone)]
pub(super) struct Completion {
    pub(super) attempt: Attempt,
}
pub(super) enum Claim {
    Ready {
        operation: Operation,
        completion: Completion,
    },
    Deferred {
        ticket: Ticket,
        until: u64,
    },
}
impl Permit {
    pub(super) fn is_live(&self) -> bool {
        !self.live.is_poisoned() && !self.revoked.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(super) fn current(&self, conn: &Connection) -> Result<RecoveryOwner, AppError> {
        if !self.is_live() {
            return Err(invalid("recovery capability revoked"));
        }
        let o = owner_record(conn, &self.team)?.ok_or_else(|| invalid("recovery owner missing"))?;
        if o.account != self.account || o.generation != self.generation || o.epoch != self.epoch {
            return Err(invalid("recovery owner replaced"));
        }
        Ok(o)
    }
    pub(super) fn guard(&self) -> Result<std::sync::MutexGuard<'_, bool>, AppError> {
        let g = self
            .live
            .lock()
            .map_err(|_| invalid("recovery capability poisoned"))?;
        if !*g || !self.is_live() {
            return Err(invalid("recovery capability revoked"));
        }
        Ok(g)
    }
}
/// Explicit takeover revokes the durable stream and previous recovery issuer.
/// It proves exclusive new authority, not that the previous process is dead.
pub(super) fn take_over(
    store: &DraftStore,
    account: &str,
    team: &str,
    expected: Option<&RecoveryOwner>,
) -> Result<Permit, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, account)?;
    let previous = owner_record(&tx, team)?;
    if previous.as_ref() != expected {
        return Err(invalid("recovery takeover stale"));
    }
    attempts(&tx)?;
    let mut stream = storage::team(&tx, team)?;
    stream.epoch = increment(stream.epoch)?;
    stream.mode = Mode::Gap;
    stream.lease_id = None;
    stream.gap_reason = Some("explicit recovery takeover".into());
    if let Some(p) = stream.pending.take() {
        stream.last_interrupted_id = Some(p.id);
    }
    storage::write_team(&tx, team, &stream)?;
    let o = RecoveryOwner {
        version: 1,
        account: account.into(),
        team: team.into(),
        generation: uuid::Uuid::new_v4().to_string(),
        epoch: previous.as_ref().map_or(Ok(1), |o| increment(o.epoch))?,
        deadline: stream
            .read_retry_at_ms
            .max(previous.as_ref().map_or(0, |o| o.deadline)),
        unknown_ordinal: previous.as_ref().map_or(0, |o| o.unknown_ordinal),
    };
    write_owner(&tx, &o)?;
    tx.commit()?;
    Ok(Permit {
        account: account.into(),
        team: team.into(),
        generation: o.generation,
        epoch: o.epoch,
        live: Arc::new(Mutex::new(true)),
        revoked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}
pub(super) fn reserve(
    store: &DraftStore,
    permit: &Permit,
    expected: &ProtectedReview,
    op: &Operation,
    now: u64,
) -> Result<Ticket, AppError> {
    let _live = permit.guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &permit.account)?;
    let mut o = permit.current(&tx)?;
    let current = storage::exact(&tx, expected)?;
    if current.review.scope != op.scope
        || op.scope.account_id != permit.account
        || op.scope.key.team != permit.team
    {
        return Err(invalid("check owner mismatch"));
    }
    let stream = storage::team(&tx, &permit.team)?;
    if now < o.deadline.max(stream.read_retry_at_ms)
        || stream.pending.is_some()
        || unresolved(&tx, &permit.team)?
        || readback::unresolved(&tx, &permit.team)?
    {
        return Err(invalid("another read pending or cooling down"));
    }
    let mut state = current.row.stored.state.clone();
    let ticket = state
        .reserve_check(op, now)
        .map_err(|_| invalid("check not eligible"))?;
    let floor = now
        .checked_add(READ_FLOOR_MS)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| invalid("check clock exhausted"))?;
    o.deadline = o.deadline.max(floor);
    let predecessor = attempts(&tx)?
        .values()
        .filter(|a| {
            a.scope == op.scope
                && a.enrollment == current.row.ownership.enrollment_id
                && a.ticket.operation() == op
        })
        .max_by_key(|a| a.floor)
        .map(|a| a.ticket.id().to_owned());
    let a = Attempt {
        version: 1,
        scope: op.scope.clone(),
        enrollment: current.row.ownership.enrollment_id.clone(),
        generation: o.generation.clone(),
        owner_epoch: o.epoch,
        ticket,
        floor,
        deadline: floor,
        state: State::Pending,
        result: None,
        late: None,
        predecessor,
    };
    write_owner(&tx, &o)?;
    write_attempt(&tx, &a)?;
    storage::write_row(&tx, &successor(&current.row, state)?)?;
    tx.commit()?;
    Ok(Ticket { attempt: a })
}
/// Consumes a process-only ticket. Completion clones cannot call this function.
/// Future transport adapter must also check its captured Binding immediately.
pub(super) fn claim(
    store: &DraftStore,
    permit: &Permit,
    ticket: Ticket,
    now: u64,
) -> Result<Claim, AppError> {
    let _live = permit.guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &permit.account)?;
    let o = permit.current(&tx)?;
    let a = attempts(&tx)?
        .remove(ticket.attempt.ticket.id())
        .ok_or_else(|| invalid("check missing"))?;
    if a != ticket.attempt || a.state != State::Pending || a.generation != o.generation {
        return Err(invalid("check superseded"));
    }
    // The reservation's own spacing floor applies to future attempts, not itself.
    // Any *additional* late cooldown extension blocks dispatch of this ticket.
    let deadline = storage::team(&tx, &permit.team)?
        .read_retry_at_ms
        .max(if o.deadline > a.floor { o.deadline } else { 0 });
    if now < deadline {
        tx.commit()?;
        return Ok(Claim::Deferred {
            ticket,
            until: deadline,
        });
    }
    let row = storage::read_row(&tx, &a.scope)?;
    if row.ownership.enrollment_id != a.enrollment
        || row.stored.state.pending_check(a.ticket.operation()) != Some(&a.ticket)
    {
        return Err(invalid("check owner changed"));
    }
    tx.commit()?;
    Ok(Claim::Ready {
        operation: a.ticket.operation().clone(),
        completion: Completion { attempt: a },
    })
}
pub(super) fn complete(
    store: &DraftStore,
    completion: &Completion,
    result: ResultKind,
    known_until: u64,
) -> Result<Option<ProtectedRow>, AppError> {
    sql_number(known_until)?;
    if matches!(
        &result,
        ResultKind::Positive(facade::Outcome::Rejected | facade::Outcome::Uncertain)
    ) {
        return Err(invalid("read cannot manufacture mutation failure"));
    }
    let tx = adapter_atomic::begin(&store.0)?;
    let before = &completion.attempt;
    owner(&tx, &before.scope.account_id)?;
    let mut a = attempts(&tx)?
        .remove(before.ticket.id())
        .ok_or_else(|| invalid("attempt vanished"))?;
    if a.ticket != before.ticket
        || a.scope != before.scope
        || a.enrollment != before.enrollment
        || a.generation != before.generation
        || a.floor != before.floor
        || a.owner_epoch != before.owner_epoch
        || a.predecessor != before.predecessor
    {
        return Err(invalid("completion identity changed"));
    }
    if known_until > 0 {
        persist_deadline(&tx, &a.scope, known_until)?;
    }
    let mut o =
        owner_record(&tx, &a.scope.key.team)?.ok_or_else(|| invalid("recovery owner missing"))?;
    o.deadline = o.deadline.max(known_until).max(a.floor);
    a.deadline = a.deadline.max(known_until);
    if a.state == State::SupersededUnknown || a.state == State::Historical {
        // Retain evidence and known rate limits only; never finish a replacement.
        if a.late.as_ref().is_some_and(|r| r != &result) {
            return Err(invalid("late completion changed"));
        }
        a.late = Some(result);
        if matches!(a.late, Some(ResultKind::Executed { .. })) {
            a.version = 2;
        }
        write_owner(&tx, &o)?;
        write_attempt(&tx, &a)?;
        tx.commit()?;
        return Ok(None);
    }
    if a.state == State::Completed {
        if a.result.as_ref() != Some(&result) {
            return Err(invalid("completion changed"));
        }
        write_owner(&tx, &o)?;
        write_attempt(&tx, &a)?;
        tx.commit()?;
        return Ok(None);
    }
    let current = storage::review_row(&tx, &a.scope)?;
    if current.row.ownership.enrollment_id != a.enrollment {
        return Err(invalid("completion owner replaced"));
    }
    let mut state = current.row.stored.state.clone();
    // Positive mutation may already have resolved this exact op while read ran.
    let still_pending = state.pending() == Some(a.ticket.operation())
        && matches!(state.workflow(), Workflow::Uncertain(_));
    if still_pending {
        if !state.finish_check(&a.ticket, o.deadline) {
            return Err(invalid("check ticket replaced"));
        }
        if let ResultKind::Positive(outcome) = &result {
            if !facade::apply(&mut state, a.ticket.operation(), outcome) {
                return Err(invalid("positive status proof refused"));
            }
        }
    }
    let row = if state != current.row.stored.state {
        let row = successor(&current.row, state)?;
        storage::write_row(&tx, &row)?;
        Some(row)
    } else {
        None
    };
    a.state = State::Completed;
    a.result = Some(result);
    if matches!(a.result, Some(ResultKind::Executed { .. })) {
        a.version = 2;
    }
    write_owner(&tx, &o)?;
    write_attempt(&tx, &a)?;
    tx.commit()?;
    Ok(row)
}
pub(super) fn delay(ordinal: u32) -> u64 {
    READ_FLOOR_MS
        .saturating_mul(1u64 << ordinal.min(5))
        .min(900_000)
}
/// Optional explicit recovery action for an old positive response retained after
/// supersession. This consumes no read ticket and never verifies a new baseline.
pub(super) fn accept_late_positive(
    store: &DraftStore,
    expected: &ProtectedReview,
    id: &str,
) -> Result<ProtectedRow, AppError> {
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &expected.review.scope.account_id)?;
    let current = storage::exact(&tx, expected)?;
    let attempt = attempts(&tx)?
        .remove(id)
        .ok_or_else(|| invalid("retained attempt absent"))?;
    if attempt.scope != current.review.scope
        || attempt.enrollment != current.row.ownership.enrollment_id
        || !matches!(
            attempt.state,
            State::SupersededUnknown | State::Historical | State::Completed
        )
        || current.row.stored.state.pending() != Some(attempt.ticket.operation())
    {
        return Err(invalid("retained positive receipt owner changed"));
    }
    let evidence = if attempt.state == State::Completed {
        attempt.result
    } else {
        attempt.late
    };
    let Some(ResultKind::Positive(outcome)) = evidence else {
        return Err(invalid("no retained positive evidence"));
    };
    if matches!(
        outcome,
        facade::Outcome::Rejected | facade::Outcome::Uncertain
    ) {
        return Err(invalid("not a positive receipt"));
    }
    let mut state = current.row.stored.state.clone();
    if !facade::apply(&mut state, attempt.ticket.operation(), &outcome) {
        return Err(invalid("retained receipt refused"));
    }
    let row = successor(&current.row, state)?;
    storage::write_row(&tx, &row)?;
    tx.commit()?;
    Ok(row)
}
pub(super) fn unknown_deadline(
    o: &mut RecoveryOwner,
    now: u64,
    known: u64,
) -> Result<u64, AppError> {
    let next = now
        .checked_add(delay(o.unknown_ordinal))
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| invalid("unknown retry clock exhausted"))?;
    o.unknown_ordinal = o.unknown_ordinal.saturating_add(1).min(31);
    o.deadline = o.deadline.max(known).max(next);
    Ok(o.deadline)
}
/// Explicit user choice after takeover. This closes ONLY a read admission.
/// It does not resolve, reject, recreate or dispatch its frozen mutation.
pub(super) fn supersede_unknown(
    store: &DraftStore,
    permit: &Permit,
    expected: &ProtectedReview,
    ticket: &CheckTicket,
    now: u64,
) -> Result<u64, AppError> {
    let _live = permit.guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &permit.account)?;
    let mut o = permit.current(&tx)?;
    let current = storage::exact(&tx, expected)?;
    let mut a = attempts(&tx)?
        .remove(ticket.id())
        .ok_or_else(|| invalid("unknown attempt missing"))?;
    if a.scope != current.review.scope
        || a.scope.key.team != permit.team
        || a.enrollment != current.row.ownership.enrollment_id
        || a.ticket != *ticket
        || a.state != State::Pending
        || a.owner_epoch >= o.epoch
        || a.generation == o.generation
    {
        return Err(invalid("active request cannot be superseded"));
    }
    let until = unknown_deadline(
        &mut o,
        now,
        a.deadline
            .max(storage::team(&tx, &permit.team)?.read_retry_at_ms),
    )?;
    let mut state = current.row.stored.state.clone();
    // The original mutation receipt may already have resolved this operation.
    // Retire its admission regardless, but only clear the exact still-current
    // uncertain ticket. Never rewrite a successor's state or check admission.
    if state.pending() == Some(ticket.operation())
        && matches!(state.workflow(), Workflow::Uncertain(_))
        && state.pending_check(ticket.operation()) == Some(ticket)
    {
        if !state.finish_check(ticket, until) {
            return Err(invalid("unknown check changed"));
        }
        storage::write_row(&tx, &successor(&current.row, state)?)?;
    }
    a.state = State::SupersededUnknown;
    a.deadline = until;
    write_owner(&tx, &o)?;
    write_attempt(&tx, &a)?;
    tx.commit()?;
    Ok(until)
}

/// Same policy for an interrupted baseline read; keep original full record in
/// an immutable audit entry, and never set verified_epoch or synthesize an event.
pub(super) fn supersede_baseline(
    store: &DraftStore,
    permit: &Permit,
    expected: &ProtectedReview,
    id: &str,
    now: u64,
) -> Result<u64, AppError> {
    let _live = permit.guard()?;
    let tx = adapter_atomic::begin(&store.0)?;
    owner(&tx, &permit.account)?;
    let mut o = permit.current(&tx)?;
    let current = storage::exact(&tx, expected)?;
    let mut stream = storage::team(&tx, &permit.team)?;
    let mut old = inventory::reads(&tx)?
        .remove(id)
        .ok_or_else(|| invalid("baseline attempt missing"))?;
    if old.team != permit.team
        || old.enrollment_id != current.row.ownership.enrollment_id
        || old.state != ReadState::Unresolved
        || old.epoch >= stream.epoch
        || stream.pending.as_ref().is_some_and(|p| p.id == id)
    {
        return Err(invalid("baseline attempt remains live"));
    }
    let until = unknown_deadline(&mut o, now, old.retry_at_ms.max(stream.read_retry_at_ms))?;
    let audit = Abandonment {
        version: 1,
        scope: current.review.scope.clone(),
        original: old.clone(),
        generation: o.generation.clone(),
        retry_at_ms: until,
    };
    tx.execute(
        "INSERT INTO composer_draft_read_abandonment VALUES(?1,?2,?3)",
        params![id, permit.team, serde_json::to_string(&audit)?],
    )?;
    old.state = ReadState::SupersededUnknown;
    old.completion = Some(ReadCompletion::SupersededUnknown);
    old.retry_at_ms = until;
    stream.read_retry_at_ms = stream.read_retry_at_ms.max(until);
    readback::write_record(&tx, id, &old)?;
    storage::write_team(&tx, &permit.team, &stream)?;
    write_owner(&tx, &o)?;
    tx.commit()?;
    Ok(until)
}
