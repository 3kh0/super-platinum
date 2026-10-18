//! Exact completed families only. Never compact an unresolved child or history.
use super::*;

pub(super) fn keys(
    pending: &Pending,
    final_key: &Key,
    families: &[&str],
) -> Result<Vec<Key>, Reason> {
    let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
    a.check(a.identity.owner.account(), false)?;
    let last = a.entry(final_key)?;
    let root = last.parent.ok_or(Reason::Unsupported)?;
    if a.slots.values().filter(|e| e.parent == Some(root)).count() != families.len() - 1 {
        return Err(Reason::Incomplete);
    }
    let mut out = Vec::new();
    for (i, family) in families.iter().enumerate() {
        let mut entries = a.slots.iter().filter(|(id, e)| {
            e.family == *family
                && if i == 0 {
                    **id == root && e.parent.is_none()
                } else {
                    e.parent == Some(root)
                }
        });
        let (id, e) = entries.next().ok_or(Reason::Stale)?;
        if entries.next().is_some() {
            return Err(Reason::Stale);
        }
        out.push(Key {
            route: a.identity.clone(),
            id: *id,
            identity: e.identity.clone(),
        });
    }
    if !same(out.last().unwrap(), final_key) {
        return Err(Reason::Stale);
    }
    Ok(out)
}
fn same(a: &Key, b: &Key) -> bool {
    a.id == b.id && Arc::ptr_eq(&a.route, &b.route) && Arc::ptr_eq(&a.identity, &b.identity)
}
pub(super) fn transferred<I: Send + Sync + 'static, T: Send + 'static>(
    pending: &Pending,
    key: &Key,
    next: &Key,
) -> Result<u64, Reason> {
    let e = pending.admission.as_ref().unwrap().entry(key)?;
    let held = e
        .retained
        .clone()
        .downcast::<StepHeld<I, T>>()
        .map_err(|_| Reason::Stale)?;
    let s = held.status.lock().map_err(|_| Reason::Incomplete)?;
    if s.next.as_ref().is_none_or(|k| !same(k, next))
        || s.reserved.as_ref().is_none_or(|k| !same(k, next))
        || !s.tail.is_empty()
        || s.result.is_some()
        || s.request.is_some()
        || !s.failures.is_empty()
        || s.disconnected
    {
        return Err(Reason::Incomplete);
    }
    s.submitted.ok_or(Reason::Incomplete)
}
pub(super) fn finish(
    pending: &mut Pending,
    keys: &[Key],
    ids: &[u64],
    retire: bool,
) -> Result<(), Reason> {
    let book = pending.accounting.as_ref().ok_or(Reason::Incomplete)?;
    let reports: Vec<_> = ids
        .iter()
        .map(|id| {
            book.terminal(*id)
                .map(|r| (*id, r))
                .ok_or(Reason::Incomplete)
        })
        .collect::<Result<_, _>>()?;
    if reports.len() != keys.len()
        || reports.iter().any(|(_, r)| {
            !matches!(
                r.execution,
                terminal::Execution::ReadObserved | terminal::Execution::StorageAccepted
            )
        })
        || reports
            .last()
            .is_none_or(|(_, r)| r.inventory != terminal::Inventory::NotAssessed)
        || reports
            .iter()
            .any(|(_, r)| r.inventory == terminal::Inventory::ConflictPreserved)
    {
        return Err(Reason::Incomplete);
    }
    if retire {
        if !pending
            .accounting
            .as_mut()
            .unwrap()
            .acknowledge_family(&reports)
        {
            return Err(Reason::Incomplete);
        }
        for key in keys {
            pending.admission.as_mut().unwrap().slots.remove(&key.id);
        }
    }
    Ok(())
}
impl Worker {
    pub(in crate::drafts::store::worker) fn observe_send_family(
        &self,
        c: &Completion,
        retire: bool,
    ) -> Result<p::Review, Reason> {
        // Poll delivery before Pending; never wait for a receiver under Pending.
        c.result()?;
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let keys = keys(
            &pending,
            &c.key,
            &[
                "protected-consume-send",
                "protected-prepare",
                "protected-claim-dispatch",
                "protected-record",
            ],
        )?;
        let actual = pending
            .admission
            .as_ref()
            .unwrap()
            .entry(&c.key)?
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &c.held) {
            return Err(Reason::Stale);
        }
        let s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        let Some(Finished::Mutation(Ok(review))) = s.result.as_deref() else {
            return Err(Reason::Incomplete);
        };
        if !matches!(
            s.raw.as_deref(),
            Some(Raw::Mutation {
                outcome: p::Outcome::Sent { .. },
                ..
            })
        ) || review.row().state().pending().is_some()
            || !s.handed
            || s.building
            || s.disconnected
            || s.issued.is_some()
            || s.dispatch.is_some()
            || s.request.is_some()
            || s.attempts.len() != 1
        {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if s.attempts[0].0 != id || !Arc::ptr_eq(&s.attempts[0].1, s.result.as_ref().unwrap()) {
            return Err(Reason::Incomplete);
        }
        let ids = [
            transferred::<SendInput, p::Review>(&pending, &keys[0], &keys[1])?,
            transferred::<Arc<SendInput>, p::DispatchTicket>(&pending, &keys[1], &keys[2])?,
            transferred::<Arc<SendInput>, p::ClaimedDispatch>(&pending, &keys[2], &keys[3])?,
            id,
        ];
        let observed = review.clone();
        finish(&mut pending, &keys, &ids, retire)?;
        Ok(observed)
    }
}

impl Worker {
    pub(in crate::drafts::store::worker) fn observe_local_handoff_family(
        &self,
        c: &Completion,
        retire: bool,
    ) -> Result<p::LocalWriterLease, Reason> {
        c.result()?;
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let keys = keys(
            &pending,
            &c.key,
            &["protected-release", "protected-start-local"],
        )?;
        let actual = pending
            .admission
            .as_ref()
            .unwrap()
            .entry(&c.key)?
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &c.held) {
            return Err(Reason::Stale);
        }
        let s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        let Some(Finished::Local(Ok(lease))) = s.result.as_deref() else {
            return Err(Reason::Incomplete);
        };
        if !matches!(s.raw.as_deref(), Some(Raw::Local))
            || !matches!(s.binding.as_deref(), Some(Binding::Local(..)))
            || s.handed
            || s.building
            || s.disconnected
            || s.issued.is_some()
            || s.dispatch.is_some()
            || s.request.is_some()
            || s.attempts.len() != 1
        {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if s.attempts[0].0 != id || !Arc::ptr_eq(&s.attempts[0].1, s.result.as_ref().unwrap()) {
            return Err(Reason::Incomplete);
        }
        let ids = [
            transferred::<ReleaseInput, p::Release>(&pending, &keys[0], &keys[1])?,
            id,
        ];
        let observed = lease.clone();
        finish(&mut pending, &keys, &ids, retire)?;
        Ok(observed)
    }
}
