//! Real worker/archive/Editor; native predicates below are explicit unit inputs,
//! not a browser capture or a replacement for the mounted native gate.
use super::*;

async fn completed(keep: bool) -> (Fixture, operation::Operation, Arc<Slash>) {
    let f = Fixture::with_outcome(
        false,
        if keep {
            o::SlashFixtureOutcome::KeepInput
        } else {
            o::SlashFixtureOutcome::Clear
        },
    )
    .await;
    let parent = f.parent();
    let held = f.child(&parent).await;
    held.execute().await.unwrap();
    let after = f.editor.slash_canonical(&held).await.unwrap();
    if keep {
        assert!(held.acknowledged_without_clear());
        assert!(!held.has_fresh_clear().unwrap());
        assert!(
            f.editor
                .reconcile_slash_keep(&held, &after, || false)
                .is_err()
        );
        f.editor
            .reconcile_slash_keep(&held, &after, || true)
            .unwrap();
        held.native_result(NativeClear::NotAttempted, true).unwrap();
    } else {
        assert!(!held.acknowledged_without_clear());
        held.consume_fresh_clear().unwrap();
        f.editor.reconcile_slash(&held, &after, || true).unwrap();
        held.native_result(NativeClear::Cleared, true).unwrap();
    }
    parent.finish(Ok(operation::Status::Finished));
    (f, parent, held)
}

#[tokio::test]
async fn actual_clear_and_keep_archive_retain_until_fresh_native_ack_then_allow_next_input() {
    for keep in [false, true] {
        let (f, parent, child) = completed(keep).await;
        let ops = &f.account.owned.as_ref().unwrap().operations;
        let before = ops.retained().len();
        let serial = f.editor.snapshot().serial;
        let current = f.editor.archive_slash(&parent).await.unwrap();
        assert_eq!(f.editor.snapshot().serial, serial + 1);
        assert!(current.same(f.editor.snapshot().view.inventory().managed().unwrap()));
        assert!(same_local(
            current.local(),
            &local(if keep { "/example original" } else { "" })
        ));
        assert!(!child.retired_success());
        assert!(
            f.editor
                .acknowledge_slash(&parent, &current, || false)
                .is_err()
        );
        assert_eq!(ops.retained().len(), before + 2);
        assert!(child.completion().unwrap().observe_acknowledged().is_ok());
        let archive = child
            .state
            .observe(|s| s.archive.clone())
            .unwrap()
            .1
            .unwrap();
        // Recording an already-observed native disposition cannot replace or
        // discard the separately retained archive/checkpoint family.
        child
            .native_result(
                if keep {
                    NativeClear::NotAttempted
                } else {
                    NativeClear::Cleared
                },
                true,
            )
            .unwrap();
        let retained = child
            .state
            .observe(|s| s.archive.clone())
            .unwrap()
            .1
            .unwrap();
        assert!(Arc::ptr_eq(&archive, &retained));
        let archive_stamp = archive.ownership_stamp().unwrap();
        assert!(archive.validate_ownership(&archive_stamp).is_ok());
        // A native refusal must leave an explicit resumable cleanup, not a
        // permanent ReviewRequired trap or a second checkpoint/archive family.
        for _ in 0..2 {
            let resumed = f.editor.archive_slash(&parent).await.unwrap();
            assert!(resumed.same(&current));
            assert_eq!(f.editor.snapshot().serial, serial + 1);
            assert_eq!(ops.retained().len(), before + 2);
            let same_archive = child
                .state
                .observe(|s| s.archive.clone())
                .unwrap()
                .1
                .unwrap();
            assert!(Arc::ptr_eq(&archive, &same_archive));
            assert_eq!(f.context.requests().len(), 1);
            assert!(
                f.editor
                    .acknowledge_slash(&parent, &resumed, || false)
                    .is_err()
            );
        }
        f.editor
            .acknowledge_slash(&parent, &current, || true)
            .unwrap();
        assert!(child.retired_success());
        assert!(archive.validate_ownership(&archive_stamp).is_err());
        assert_eq!(ops.retained().len(), before - 3);
        parent.acknowledge().unwrap();
        assert!(f.account.retained_operations().unwrap().is_empty());
        assert!(child.execute().await.is_err());
        f.editor.submit(&local("/example next real input")).unwrap();
        f.editor.flush().await.unwrap();
        assert_eq!(
            f.editor.snapshot().view.local().composer.text,
            "/example next real input"
        );
        assert_eq!(ops.retained().len(), before - 3);
        assert_eq!(f.context.requests().len(), 1);
    }
}

#[tokio::test]
async fn changed_input_or_foreign_parent_cannot_acknowledge_archived_family() {
    let (f, parent, child) = completed(false).await;
    let current = f.editor.archive_slash(&parent).await.unwrap();
    let other = Fixture::new(false).await;
    assert!(
        other
            .editor
            .acknowledge_slash(&parent, &current, || true)
            .is_err()
    );
    f.editor.submit(&local("typed after archive read")).unwrap();
    f.editor.flush().await.unwrap();
    assert!(
        f.editor
            .acknowledge_slash(&parent, &current, || true)
            .is_err()
    );
    assert!(!child.retired_success());
    assert!(child.completion().unwrap().observe_acknowledged().is_ok());
    let serial = f.editor.snapshot().serial;
    let retained = f
        .account
        .owned
        .as_ref()
        .unwrap()
        .operations
        .retained()
        .len();
    assert!(f.editor.archive_slash(&parent).await.is_err());
    assert_eq!(f.editor.snapshot().serial, serial);
    assert_eq!(
        f.account
            .owned
            .as_ref()
            .unwrap()
            .operations
            .retained()
            .len(),
        retained
    );
    assert_eq!(f.context.requests().len(), 1);
    assert_eq!(
        f.editor.snapshot().view.local().composer.text,
        "typed after archive read"
    );
}

#[tokio::test]
async fn terminal_status_without_owned_reconciliation_cannot_archive() {
    let f = Fixture::with_outcome(false, o::SlashFixtureOutcome::Clear).await;
    let parent = f.parent();
    let child = f.child(&parent).await;
    child.execute().await.unwrap();
    parent.finish(Ok(operation::Status::Finished));
    let before = f
        .account
        .owned
        .as_ref()
        .unwrap()
        .operations
        .retained()
        .len();
    assert!(f.editor.archive_slash(&parent).await.is_err());
    assert_eq!(
        f.account
            .owned
            .as_ref()
            .unwrap()
            .operations
            .retained()
            .len(),
        before
    );
    assert!(child.has_fresh_clear().unwrap());
    assert_eq!(
        f.editor.snapshot().view.local().composer.text,
        "/example original"
    );
}
