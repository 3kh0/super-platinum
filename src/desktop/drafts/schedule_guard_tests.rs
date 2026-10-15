use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test]
async fn caller_context_lost_during_owner_wait_never_admits_schedule() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let before = f.editor.owner.load().await.unwrap();
    let owner = f.editor.owner();
    let owner_guard = owner.gate.lock().await;
    let current = Arc::new(AtomicBool::new(true));
    let guard_current = current.clone();
    let editor = f.editor.clone();
    let pending = tokio::spawn(async move {
        editor
            .consume_reviewed_schedule_inner(
                lease,
                visit,
                raw,
                500,
                move || {
                    if guard_current.load(Ordering::Acquire) {
                        Ok(())
                    } else {
                        Err(Fault::Stale)
                    }
                },
                |backend, expected, raw, time| async move {
                    backend.commit_reviewed_schedule(expected, raw, time).await
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
