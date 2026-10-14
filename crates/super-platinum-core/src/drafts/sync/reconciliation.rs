//! Explicit positive readback resolution; never an event, enrollment or dispatch.
//! Unreferenced pending the dedicated core/storage gate.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReconcileError {
    #[error("the remote draft identity is not the acknowledged owner")]
    Identity,
    #[error("delivery or unsupported evidence must be resolved separately")]
    Blocked,
    #[error("the returned draft is older than retained remote evidence")]
    StaleRemote,
    #[error("the chosen or remote draft cannot round-trip through the supported composer")]
    Unsupported,
    #[error("the local draft revision is exhausted")]
    Revision,
}

/// The caller must prove positive active/unscheduled readback and explicit human
/// choice, then persist this result with the chosen local value under whole-review
/// CAS. `remote` contains no authentication/lifecycle proof by itself.
/// Existing event clocks and retirement identities are never reset. In particular
/// unsupported/equal-clock blocked evidence cannot be silently cleared: that
/// requires a separate archival format and recovery contract.
pub fn resolve_reviewed_remote(
    expected: &DraftSync,
    remote: &RemoteDraft,
    desired: Option<Snapshot>,
) -> Result<DraftSync, ReconcileError> {
    if expected.pending().is_some()
        || expected.consumption.is_some()
        || expected.blocked_observation.is_some()
        || !matches!(
            expected.workflow,
            Workflow::Idle | Workflow::Conflict { pending: None, .. }
        )
    {
        return Err(ReconcileError::Blocked);
    }
    let base = expected
        .acknowledged
        .as_ref()
        .ok_or(ReconcileError::Identity)?;
    if remote.scope != expected.scope
        || remote.id != base.id
        || remote.client_msg_id != base.client_msg_id
        || remote.client_msg_id != expected.client_msg_id
        || expected.retired_ids.contains(&remote.id)
        || DraftSync::adopt_remote(&expected.scope, remote.clone()).is_err()
    {
        return Err(ReconcileError::Identity);
    }
    // Bounded reconciliation policy, not a claim that every server revision
    // uses this precision. ts_key compares fractional integers, so ambiguous
    // legacy/returned precision must remain unsupported rather than misordered.
    let canonical_revision = |draft: &RemoteDraft| {
        revision(&draft.last_updated_ts)
            && draft
                .last_updated_ts
                .split_once('.')
                .is_some_and(|(_, fraction)| fraction.len() == 6)
    };
    if !canonical_revision(remote)
        || !canonical_revision(base)
        || matches!(&expected.workflow, Workflow::Conflict { remote: Some(old), .. } if !canonical_revision(old))
        || expected.observed_events.values().any(|event| {
            matches!(&event.kind, ObservationKind::Active(old) if old.id == remote.id && !canonical_revision(old))
        })
    {
        return Err(ReconcileError::Unsupported);
    }
    let newer_than = |old: &RemoteDraft| {
        ts_key(&remote.last_updated_ts) > ts_key(&old.last_updated_ts)
            || (ts_key(&remote.last_updated_ts) == ts_key(&old.last_updated_ts)
                && remote.snapshot == old.snapshot)
    };
    if !newer_than(base)
        || matches!(&expected.workflow, Workflow::Conflict { remote: Some(old), .. } if !newer_than(old))
        || expected.observed_events.values().any(|event| {
            matches!(&event.kind, ObservationKind::Active(old) if old.id == remote.id && !newer_than(old))
        })
    {
        return Err(ReconcileError::StaleRemote);
    }
    if crate::drafts::codec::decode(&expected.scope.key, &remote.snapshot).is_err()
        || desired.as_ref().is_some_and(|snapshot| {
            crate::drafts::codec::decode(&expected.scope.key, snapshot).is_err()
        })
    {
        return Err(ReconcileError::Unsupported);
    }
    let mut result = expected.clone();
    // A review establishes a new semantic predecessor even if visible text is
    // equal. Invalidate prior local claims without inventing an event timestamp.
    result.local_revision = result
        .local_revision
        .checked_add(1)
        .ok_or(ReconcileError::Revision)?;
    if desired.as_ref() == Some(&remote.snapshot) {
        result.acknowledged_local_revision = result.local_revision;
    }
    result.desired = desired;
    result.tombstone = result.desired.is_none();
    result.acknowledged = Some(remote.clone());
    result.workflow = Workflow::Idle;
    Ok(result)
}

#[cfg(test)]
mod tests;
