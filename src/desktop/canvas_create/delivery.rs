use super::{
    Source,
    custody::{Job, Phase},
    domain, now,
};
use domain::{Attempt, Delivery, Draft, Outcome, Store};
use std::sync::{Arc, atomic::Ordering};
use super_platinum_core::slack::PreparedRequest;

#[derive(Clone)]
pub(in crate::canvas_create) enum Evidence {
    Unknown,
    RateLimited(u64),
    Created(String),
}
impl Evidence {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Uncertain => Self::Unknown,
            Outcome::RateLimited { retry_at } => Self::RateLimited(retry_at),
            Outcome::Created { file_id } => Self::Created(file_id),
        }
    }
    pub(in crate::canvas_create) fn outcome(&self) -> Outcome {
        match self {
            Self::Unknown => Outcome::Uncertain,
            Self::RateLimited(retry_at) => Outcome::RateLimited {
                retry_at: *retry_at,
            },
            Self::Created(file_id) => Outcome::Created {
                file_id: file_id.clone(),
            },
        }
    }
    fn matches(&self, delivery: &Delivery) -> bool {
        match (self, delivery) {
            (Self::Unknown, Delivery::Uncertain) => true,
            (Self::RateLimited(a), Delivery::RateLimited { retry_at: b }) => a == b,
            (Self::Created(a), Delivery::Created { file_id: b }) => a == b,
            _ => false,
        }
    }
}

pub(in crate::canvas_create) async fn run(
    job: Arc<Job>,
    saved: Draft,
    source: impl Source,
    request: PreparedRequest,
) {
    let result = run_inner(&job, saved, source, request).await;
    if let Err(error) = result {
        job.publish(Phase::Held(error.into()));
    }
}
async fn run_inner(
    job: &Arc<Job>,
    saved: Draft,
    source: impl Source,
    request: PreparedRequest,
) -> domain::Result<()> {
    if !source.current() || job.cancel_requested.load(Ordering::Acquire) {
        job.publish(Phase::Saved(saved));
        return Ok(());
    }
    let reserved_at = now()?;
    job.reserve_started.store(true, Ordering::Release);
    let reservation = job
        .storage
        .run(move |store| store.reserve(&saved, reserved_at))
        .await?;
    *job.reservation.lock().expect("Canvas custody lock") = Some(reservation.clone());
    if !source.current() || job.cancel_requested.load(Ordering::Acquire) {
        let canceled = job
            .storage
            .run(move |store| store.cancel_reserved(&reservation))
            .await?;
        job.publish(Phase::Canceled(canceled));
        return Ok(());
    }
    let admitted_at = now()?;
    let permit = job
        .storage
        .run(move |store| store.admit(&reservation, admitted_at))
        .await?;
    let started = permit.consume();
    // No await from one-use permit consumption through this final serial owner
    // check to the existing executor. A stale owner after Started stays unknown;
    // it is never rewritten as CanceledBeforeAdmission.
    let evidence = if source.current() && !job.cancel_requested.load(Ordering::Acquire) {
        let response = source.execute(request).await;
        // Clock failure must not throw away a returned positive file ID. Use the
        // valid admission clock as a conservative fallback for classification.
        // Overflowing Retry-After is unknown, never retryable.
        Evidence::from(
            Outcome::from_response(&response, now().unwrap_or(admitted_at))
                .unwrap_or(Outcome::Uncertain),
        )
    } else {
        Evidence::Unknown
    };
    // The original stable owner receives evidence BEFORE persistence, notification
    // or any UI-current filter. A storage failure leaves this slot charged.
    *job.observed.lock().expect("Canvas custody lock") = Some((started.clone(), evidence.clone()));
    persist(job, started, evidence).await
}

pub(in crate::canvas_create) async fn persist(
    job: &Arc<Job>,
    original: Attempt,
    evidence: Evidence,
) -> domain::Result<()> {
    let writer = job.writer.clone();
    let operation = job.operation.clone();
    let phase = job
        .storage
        .run(move |store| settle(store, original, evidence, &writer, &operation))
        .await?;
    job.publish(phase);
    Ok(())
}

fn same_attempt(a: &Attempt, b: &Attempt) -> bool {
    a.owner() == b.owner()
        && a.id() == b.id()
        && a.draft_id() == b.draft_id()
        && a.draft_revision() == b.draft_revision()
        && a.content() == b.content()
}
fn settle(
    store: &mut Store,
    original: Attempt,
    evidence: Evidence,
    writer: &str,
    operation: &str,
) -> domain::Result<Phase> {
    match store.settle(&original, evidence.outcome()) {
        Ok(result) => return Ok(Phase::Settled(result)),
        Err(domain::Fault::Stale) => {}
        Err(error) => return Err(error),
    }
    if let Some(current) = store.attempt(original.owner(), original.id())? {
        if !same_attempt(&current, &original)
            || current.result_revision() <= original.result_revision()
        {
            return Err(domain::Fault::Stale);
        }
        if evidence.matches(current.delivery()) {
            return Ok(Phase::Settled(current));
        }
        // A second process may have conservatively marked Started as Uncertain.
        // Only this exact original callback can refine that original attempt.
        if matches!(current.delivery(), Delivery::Uncertain) {
            return store
                .settle(&current, evidence.outcome())
                .map(Phase::Settled);
        }
        // Differing live terminal evidence has no v4 typed adjunct. Keep the
        // original root slot; never classify a different file ID as a duplicate.
        return Err(domain::Fault::Blocked);
    }
    let retired = store
        .retired_attempt(original.owner(), original.id())?
        .ok_or(domain::Fault::Stale)?;
    if retired.same_result(&original, evidence.outcome())? {
        return Ok(Phase::RetiredSame(retired));
    }
    store
        .preserve_late_result(&original, &retired, evidence.outcome(), writer, operation)
        .map(Phase::LateEvidence)
}
