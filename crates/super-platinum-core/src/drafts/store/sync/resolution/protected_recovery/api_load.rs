//! One consistent bounded inventory; loaded history cannot mint live authority.
use super::*;

pub(super) fn load(
    store: &DraftStore,
    account: &str,
    mode: LoadMode,
    pending: &BTreeMap<DraftKey, LocalDraft>,
) -> Result<Loaded, Error> {
    if mode == LoadMode::AfterRestart {
        lifecycle::revoke_account(account);
    }
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    adapter_schema::require(&tx).map_err(|_| Error::WrongFormat)?;
    owner(&tx, account).map_err(|_| Error::Storage)?;
    if mode == LoadMode::AfterRestart {
        migration::normalize(store, account).map_err(|_| Error::Storage)?;
    }
    writers::records(&tx).map_err(|_| Error::Storage)?;
    journal::inventory(&tx).map_err(|_| Error::Storage)?;
    baseline::validate_inventory(&tx).map_err(|_| Error::Storage)?;
    inventory::bound(&tx,"SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(destination AS BLOB))),0) FROM (SELECT destination FROM composer_drafts UNION SELECT destination FROM composer_draft_sync UNION SELECT destination FROM composer_draft_blocked UNION SELECT destination FROM composer_draft_alternates UNION SELECT destination FROM composer_draft_local_clock)").map_err(|_|Error::Storage)?;
    let keys = {
        let mut q=tx.prepare("SELECT destination FROM composer_drafts UNION SELECT destination FROM composer_draft_sync UNION SELECT destination FROM composer_draft_blocked UNION SELECT destination FROM composer_draft_alternates UNION SELECT destination FROM composer_draft_local_clock").map_err(|_|Error::Storage)?;
        q.query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::Storage)?
    };
    let mut out = Loaded {
        managed: BTreeMap::new(),
        locals: BTreeMap::new(),
        uncommitted: pending.clone(),
        recovery: vec![],
    };
    let mut enrollments = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for raw in keys {
        let key: DraftKey = wire::strict(&raw).map_err(|_| Error::Storage)?;
        if !seen.insert(key.clone()) {
            return Err(Error::Storage);
        }
        let scope = Scope {
            account_id: account.into(),
            key: key.clone(),
        };
        let mut local = views::local(&tx, &scope).map_err(|_| Error::Storage)?;
        local.uncommitted = pending.get(&key).cloned();
        if local.row().is_some() {
            let review = views::managed(local)?;
            if enrollments
                .insert(review.row.lineage().enrollment().to_owned(), scope)
                .is_some()
            {
                return Err(Error::Storage);
            }
            out.managed.insert(key, review);
        } else {
            out.locals.insert(key, local);
        }
    }
    for (key, input) in pending {
        if !out.managed.contains_key(key) && !out.locals.contains_key(key) {
            let mut local = views::local(
                &tx,
                &Scope {
                    account_id: account.into(),
                    key: key.clone(),
                },
            )
            .map_err(|_| Error::Storage)?;
            local.uncommitted = Some(input.clone());
            out.locals.insert(key.clone(), local);
        }
    }
    for archived in archive::inventory(&tx, account).map_err(|_| Error::Storage)? {
        if enrollments
            .insert(archived.enrollment.clone(), archived.scope.clone())
            .is_some()
        {
            return Err(Error::Storage);
        }
    }
    for (_, attempt) in recovery::attempts(&tx).map_err(|_| Error::Storage)? {
        if attempt.scope.account_id != account {
            return Err(Error::Storage);
        }
        out.recovery.push(RecoverySummary {
            scope: attempt.scope.clone(),
            enrollment: attempt.enrollment.clone(),
            operation: Some(attempt.ticket.operation().clone()),
            phase: match attempt.state {
                recovery::State::Pending => RecoveryPhase::Pending,
                recovery::State::Completed => RecoveryPhase::Completed,
                recovery::State::SupersededUnknown => RecoveryPhase::SupersededUnknown,
                recovery::State::Historical => RecoveryPhase::Historical,
            },
            deadline: attempt
                .deadline
                .max(recovery::deadline(&tx, &attempt.scope.key.team).map_err(|_| Error::Storage)?)
                .max(
                    adapter_schema::known(&tx, &attempt.scope.key.team)
                        .map_err(|_| Error::Storage)?,
                ),
            check: Some(LostCheck(attempt)),
            baseline: None,
        });
    }
    for (id, record) in inventory::reads(&tx).map_err(|_| Error::Storage)? {
        let scope = enrollments
            .get(&record.enrollment_id)
            .ok_or(Error::Storage)?;
        if scope.key.team != record.team {
            return Err(Error::Storage);
        }
        out.recovery.push(RecoverySummary {
            scope: scope.clone(),
            enrollment: record.enrollment_id.clone(),
            operation: None,
            phase: match record.state {
                ReadState::Unresolved => RecoveryPhase::Pending,
                ReadState::Finished => RecoveryPhase::Completed,
                ReadState::SupersededUnknown => RecoveryPhase::SupersededUnknown,
            },
            deadline: record
                .retry_at_ms
                .max(recovery::deadline(&tx, &record.team).map_err(|_| Error::Storage)?)
                .max(adapter_schema::known(&tx, &record.team).map_err(|_| Error::Storage)?),
            check: None,
            baseline: Some(LostBaseline {
                scope: scope.clone(),
                id,
                record,
            }),
        });
    }
    tx.commit().map_err(|_| Error::Storage)?;
    Ok(out)
}
