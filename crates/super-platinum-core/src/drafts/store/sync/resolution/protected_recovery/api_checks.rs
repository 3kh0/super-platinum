//! One-use status reads and disk-only completion retry, independent of a socket.
use super::*;

fn disk<T>(r: Result<T, AppError>) -> Result<T, Error> {
    r.map_err(|_| Error::Storage)
}
pub(super) fn state(
    store: &DraftStore,
    account: &str,
    team: &str,
) -> Result<Option<RecoveryState>, Error> {
    disk(owner(&store.0, account))?;
    Ok(
        disk(recovery::owner_record(&store.0, team))?.map(|inner| RecoveryState {
            account: account.into(),
            team: team.into(),
            inner,
        }),
    )
}
pub(super) fn takeover(
    store: &DraftStore,
    account: &str,
    team: &str,
    expected: Option<&RecoveryState>,
) -> Result<Recovery, Error> {
    if expected.is_some_and(|e| e.account != account || e.team != team) {
        return Err(Error::Stale);
    }
    lifecycle::revoke_team(account, team);
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let previous = disk(recovery::owner_record(&tx, team))?;
    if previous.as_ref() != expected.map(|e| &e.inner) {
        return Err(Error::Stale);
    }
    let inner = disk(recovery::take_over(store, account, team, previous.as_ref()))?;
    disk(tx.commit())?;
    lifecycle::register(account, team, &inner.revoked);
    Ok(Recovery {
        inner: Arc::new(inner),
        account: account.into(),
        team: team.into(),
    })
}
pub(super) fn reserve(
    store: &DraftStore,
    proof: &Recovery,
    expected: &Review,
    receipt: &Receipt,
    now: u64,
) -> Result<CheckTicket, Error> {
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    if current.scope() != receipt.scope()
        || current.row.lineage().enrollment() != receipt.enrollment()
        || current.scope().account_id != proof.account
        || current.scope().key.team != proof.team
        || current.row.state().pending() != Some(receipt.operation())
    {
        return Err(Error::Stale);
    }
    let owner = disk(proof.inner.current(&tx))?;
    let stream = disk(storage::team(&tx, &proof.team))?;
    let until = owner
        .deadline
        .max(stream.read_retry_at_ms)
        .max(disk(adapter_schema::known(&tx, &proof.team))?);
    if now < until {
        return Err(Error::Cooldown(until));
    }
    if stream.pending.is_some()
        || disk(recovery::unresolved(&tx, &proof.team))?
        || disk(readback::unresolved(&tx, &proof.team))?
    {
        return Err(Error::Pending);
    }
    let inner = disk(recovery::reserve(
        store,
        &proof.inner,
        &current.inner,
        receipt.operation(),
        now,
    ))?;
    disk(tx.commit())?;
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    Ok(CheckTicket {
        inner,
        receipt: receipt.clone(),
    })
}
pub(super) fn claim(
    store: &DraftStore,
    proof: &Recovery,
    ticket: CheckTicket,
    now: u64,
) -> Result<CheckClaim, Error> {
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    let tx = disk(adapter_atomic::begin(&store.0))?;
    if ticket.receipt.scope().account_id != proof.account
        || ticket.receipt.scope().key.team != proof.team
    {
        return Err(Error::Stale);
    }
    let _owner = disk(proof.inner.current(&tx))?;
    let known = disk(adapter_schema::known(&tx, &proof.team))?;
    if now < known {
        disk(tx.commit())?;
        if !proof.is_live() {
            return Err(Error::Gap);
        }
        return Ok(CheckClaim::Deferred {
            ticket,
            until: known,
        });
    }
    let CheckTicket { inner, receipt } = ticket;
    let result = match disk(recovery::claim(store, &proof.inner, inner, now))? {
        recovery::Claim::Deferred { ticket, until } => CheckClaim::Deferred {
            ticket: CheckTicket {
                inner: ticket,
                receipt,
            },
            until,
        },
        recovery::Claim::Ready {
            operation,
            completion,
        } => CheckClaim::Ready {
            operation,
            completion: CheckCompletion {
                inner: completion,
                receipt,
            },
        },
    };
    disk(tx.commit())?;
    if !proof.is_live() {
        return Err(Error::Gap);
    }
    Ok(result)
}
fn kind(input: &CheckResult) -> recovery::ResultKind {
    match input {
        CheckResult::Positive(outcome) => {
            recovery::ResultKind::Positive(operations::outcome(outcome))
        }
        CheckResult::Nonpositive(issue) => match issue {
            KnownReadFailure::Missing => {
                recovery::ResultKind::Nonpositive(super::super::KnownReadFailure::Missing)
            }
            KnownReadFailure::Unsupported => {
                recovery::ResultKind::Nonpositive(super::super::KnownReadFailure::Unsupported)
            }
            KnownReadFailure::ApiError => {
                recovery::ResultKind::Nonpositive(super::super::KnownReadFailure::ApiError)
            }
            KnownReadFailure::RateLimited => {
                recovery::ResultKind::Nonpositive(super::super::KnownReadFailure::RateLimited)
            }
            KnownReadFailure::Unavailable | KnownReadFailure::NotRequested => {
                recovery::ResultKind::Executed {
                    version: 1,
                    disposition: *issue,
                }
            }
        },
    }
}
pub(super) fn complete(
    store: &DraftStore,
    completion: &CheckCompletion,
    input: &CheckResult,
    until: u64,
) -> Result<CheckReceipt, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    disk(owner(&tx, &completion.scope().account_id))?;
    if until > 0 {
        disk(recovery::persist_deadline(&tx, completion.scope(), until))?;
    }
    let id = format!("check:{}", completion.inner.attempt.ticket.id());
    let journal_input = journal::Input::Check(input.clone());
    if let Some(previous) = disk(journal::inventory(&tx))?.remove(&id) {
        let result = if journal::matches(&previous, &completion.receipt, &journal_input) {
            Ok(CheckReceipt {
                review: journal::review(&previous)?,
                positive_accepted: previous.positive_accepted,
            })
        } else {
            Err(Error::Stale)
        };
        faults::before_commit(faults::FaultPoint::CheckCompletion).map_err(|_| Error::Storage)?;
        disk(tx.commit())?;
        return result;
    }
    let attempt = disk(recovery::attempts(&tx))?
        .remove(completion.inner.attempt.ticket.id())
        .ok_or(Error::Stale)?;
    let mut positive_accepted = false;
    let mut result = kind(input);
    if let CheckResult::Positive(value) = input {
        if !journal::positive(value) {
            result = recovery::ResultKind::Executed {
                version: 1,
                disposition: KnownReadFailure::Unsupported,
            };
        } else if attempt.state == recovery::State::Pending {
            let current = disk(storage::read_row(&tx, completion.scope()))?;
            if current.ownership.enrollment_id == completion.enrollment()
                && current.stored.state.pending() == Some(completion.operation())
                && matches!(current.stored.state.workflow(), Workflow::Uncertain(_))
                && current.stored.state.pending_check(completion.operation())
                    == Some(&attempt.ticket)
            {
                let mut probe = current.stored.state.clone();
                if facade::apply(
                    &mut probe,
                    completion.operation(),
                    &operations::outcome(value),
                ) {
                    positive_accepted = true;
                } else {
                    result = recovery::ResultKind::Executed {
                        version: 1,
                        disposition: KnownReadFailure::Unsupported,
                    };
                }
            }
        }
    }
    let changed = disk(recovery::complete(store, &completion.inner, result, until))?;
    let review = changed
        .map(|_| views::review(&tx, completion.scope()))
        .transpose()?;
    if positive_accepted && review.is_none() {
        return Err(Error::Storage);
    }
    disk(journal::save(
        &tx,
        &journal::Entry {
            version: 1,
            id,
            scope: completion.scope().clone(),
            enrollment: completion.enrollment().into(),
            operation: completion.operation().clone(),
            input: journal_input,
            positive_accepted,
            original: review
                .as_ref()
                .map(|r| views::InventoryRecord::capture(&views::local_from_review(r)))
                .transpose()
                .map_err(|_| Error::Storage)?,
        },
    ))?;
    faults::before_commit(faults::FaultPoint::CheckCompletion).map_err(|_| Error::Storage)?;
    disk(tx.commit())?;
    Ok(CheckReceipt {
        review,
        positive_accepted,
    })
}
pub(super) fn supersede(
    store: &DraftStore,
    proof: &Recovery,
    expected: &Review,
    lost: &LostCheck,
    now: u64,
) -> Result<u64, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    if disk(recovery::attempts(&tx))?.get(lost.0.ticket.id()) != Some(&lost.0) {
        return Err(Error::Stale);
    }
    let until = disk(recovery::supersede_unknown(
        store,
        &proof.inner,
        &current.inner,
        &lost.0.ticket,
        now,
    ))?;
    disk(tx.commit())?;
    Ok(until)
}
pub(super) fn supersede_baseline(
    store: &DraftStore,
    proof: &Recovery,
    expected: &Review,
    lost: &LostBaseline,
    now: u64,
) -> Result<u64, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    if lost.scope != *current.scope()
        || disk(inventory::reads(&tx))?.get(&lost.id) != Some(&lost.record)
    {
        return Err(Error::Stale);
    }
    let until = disk(recovery::supersede_baseline(
        store,
        &proof.inner,
        &current.inner,
        &lost.id,
        now,
    ))?;
    disk(tx.commit())?;
    Ok(until)
}
pub(super) fn accept_late(
    store: &DraftStore,
    expected: &Review,
    lost: &LostCheck,
) -> Result<Review, Error> {
    let tx = disk(adapter_atomic::begin(&store.0))?;
    let current = views::exact(&tx, expected)?;
    if disk(recovery::attempts(&tx))?.get(lost.0.ticket.id()) != Some(&lost.0) {
        return Err(Error::Stale);
    }
    disk(recovery::accept_late_positive(
        store,
        &current.inner,
        lost.0.ticket.id(),
    ))?;
    let result = views::review(&tx, current.scope())?;
    disk(tx.commit())?;
    Ok(result)
}
