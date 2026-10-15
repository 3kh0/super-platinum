use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test]
async fn caller_context_lost_during_owner_wait_keeps_rejected_intent() {
    let f = rejected_fixture().await;
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let before = f.editor.owner.load().await.unwrap();
    let owner = f.editor.owner();
    let owner_guard = owner.gate.lock().await;
    let current = Arc::new(AtomicBool::new(true));
    let guard_current = current.clone();
    let editor = f.editor.clone();
    let pending = tokio::spawn(async move {
        editor
            .apply_rejected_schedule_inner(
                lease,
                visit,
                RejectedScheduleChoice::Retry,
                200,
                move || {
                    if guard_current.load(Ordering::Acquire) {
                        Ok(())
                    } else {
                        Err(Fault::Stale)
                    }
                },
                |backend, expected, choice, now| async move {
                    backend
                        .review_rejected_schedule(expected, choice, now)
                        .await
                },
            )
            .await
    });
    editor_gate_held(&f.editor).await;
    current.store(false, Ordering::Release);
    drop(owner_guard);
    assert!(matches!(pending.await.unwrap(), Err(Fault::Stale)));
    let after = f.editor.owner.load().await.unwrap();
    assert!(same_local(&after.local, &before.local));
    assert_eq!(
        after.stored.as_ref().unwrap().revision,
        before.stored.as_ref().unwrap().revision
    );
    assert_eq!(after.stored.unwrap().state, before.stored.unwrap().state);
    assert!(f.editor.snapshot().error.is_none());
}
