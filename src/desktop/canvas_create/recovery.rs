use super::{
    custody::{Job, Phase},
    domain,
};
use domain::{Content, Delivery, Draft, Fault, Outcome, Store};
use std::sync::{Arc, atomic::Ordering};

pub(in crate::canvas_create) async fn save(job: &Arc<Job>, content: Content) {
    let base = job.base.clone();
    let writer = job.writer.clone();
    let operation = job.operation.clone();
    let result = job
        .storage
        .run(move |store| {
            // Recover an exact completed save whose worker reply was lost, not a
            // latest-row rebase or text-only match.
            if let Some(current) = store.active(base.owner())? {
                if completed_save(&base, &current, &writer, &content) {
                    return Ok(Phase::Saved(current));
                }
            }
            match store.save(&base, &writer, content.clone()) {
                Ok(saved) => Ok(Phase::Saved(saved)),
                Err(Fault::Stale | Fault::Blocked) => {
                    match store.preserve_conflict(&base, &writer, &operation, content) {
                        Ok(conflict) => Ok(Phase::Conflict(conflict)),
                        Err(Fault::Retired) => Ok(Phase::CaptureDisposed),
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            }
        })
        .await;
    if let Ok(Phase::Saved(saved)) = &result {
        *job.saved.lock().expect("Canvas custody lock") = Some(saved.clone());
    }
    job.publish(result.unwrap_or_else(|e| Phase::Held(e.into())));
}
fn completed_save(base: &Draft, current: &Draft, writer: &str, content: &Content) -> bool {
    current.owner() == base.owner()
        && current.id() == base.id()
        && base.revision().checked_add(1) == Some(current.revision())
        && current.writer() == writer
        && current.content() == content
        && current.decision_revision() == base.decision_revision()
        && !current.archived()
        && current.attempt_id().is_none()
}
pub(in crate::canvas_create) async fn retry(job: &Arc<Job>) {
    if !job.reserve_started.load(Ordering::Acquire) {
        let content = job.captured.lock().expect("Canvas custody lock").clone();
        match content {
            Some(content) => save(job, content).await,
            None => job.publish(Phase::Held(super::custody::Failure::Storage)),
        }
        return;
    }
    let saved = job.saved.lock().expect("Canvas custody lock").clone();
    let reservation = job.reservation.lock().expect("Canvas custody lock").clone();
    let Some(saved) = saved else {
        job.publish(Phase::Held(super::custody::Failure::Storage));
        return;
    };
    let writer = job.writer.clone();
    let operation = job.operation.clone();
    let result = job
        .storage
        .run(move |store| match reconcile(store, &saved, reservation) {
            Err(Fault::Stale) => {
                match store.preserve_conflict(&saved, &writer, &operation, saved.content().clone())
                {
                    Ok(conflict) => Ok(Phase::Conflict(conflict)),
                    Err(Fault::Retired) => Ok(Phase::CaptureDisposed),
                    Err(error) => Err(error),
                }
            }
            other => other,
        })
        .await;
    job.publish(result.unwrap_or_else(|e| Phase::Held(e.into())));
}
fn reconcile(
    store: &mut Store,
    saved: &Draft,
    reservation: Option<domain::Reservation>,
) -> domain::Result<Phase> {
    if let Some(reservation) = reservation {
        let original = reservation.attempt();
        let current = store
            .attempt(original.owner(), original.id())?
            .ok_or(Fault::Stale)?;
        if current.owner() != original.owner()
            || current.draft_id() != original.draft_id()
            || current.draft_revision() != original.draft_revision()
            || current.content() != original.content()
        {
            return Err(Fault::Stale);
        }
        return match current.delivery() {
            Delivery::Reserved => store.cancel_reserved(&reservation).map(Phase::Canceled),
            Delivery::Started => store
                .settle(&current, Outcome::Uncertain)
                .map(Phase::Settled),
            Delivery::CanceledBeforeAdmission => Ok(Phase::Canceled(current)),
            // Disclose durable history, never produce a permit or request.
            _ => review(store, saved),
        };
    }
    // Reserve may have committed before its reply was lost. Never adopt or
    // cancel a different process's reservation by matching visible text.
    review(store, saved)
}

fn review(store: &Store, saved: &Draft) -> domain::Result<Phase> {
    // One coherent existing snapshot; separate active/attempt reads would race
    // a valid concurrent archive/new transaction.
    let inventory = store.retirement_review(saved.owner())?;
    let draft = inventory
        .drafts()
        .iter()
        .find(|d| d.id() == saved.id())
        .ok_or(Fault::Stale)?;
    if draft.owner() != saved.owner()
        || draft.revision() != saved.revision()
        || draft.writer() != saved.writer()
        || draft.content() != saved.content()
    {
        return Err(Fault::Stale);
    }
    if draft == saved {
        return Ok(Phase::Saved(draft.clone()));
    }
    let attempt = draft
        .attempt_id()
        .map(|id| {
            inventory
                .attempts()
                .iter()
                .find(|a| a.id() == id)
                .cloned()
                .ok_or(Fault::Invalid)
        })
        .transpose()?;
    Ok(Phase::Review {
        draft: draft.clone(),
        attempt,
    })
}
