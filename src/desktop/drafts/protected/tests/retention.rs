use super::super::{editor::Editor, registry::Registry, retention};
use super::*;
use durable::test_support::FaultPoint;

#[tokio::test]
async fn failed_input_survives_registry_rotation_and_all_external_arc_drops() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let id = editor.retained_id().to_owned();
    let weak = Arc::downgrade(&editor);
    let registry = Registry::default();
    registry.select(ID.into(), 1);
    registry
        .replace(ID, 1, &fixture.owner.scope().key, None, editor.clone())
        .unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::WriterCommit))
        .await
        .unwrap();
    editor
        .submit(&local("Must survive account switch"))
        .unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    registry.select("another account".into(), 2);
    drop(editor);
    assert!(weak.upgrade().is_some());
    let retained = fixture
        .account
        .retained_editors()
        .unwrap()
        .into_iter()
        .find(|e| e.retained_id() == id)
        .unwrap();
    assert_eq!(
        retained.snapshot().pending.unwrap().composer.text,
        "Must survive account switch"
    );
    retained.retry_storage().unwrap();
    retained.flush().await.unwrap();
    registry.select(ID.into(), 3);
    registry
        .replace(ID, 3, &fixture.owner.scope().key, None, retained.clone())
        .unwrap();
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Must survive account switch"
    );
}
#[tokio::test]
async fn local_failed_input_survives_detach_and_exact_oldest_retry() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let id = editor.retained_id().to_owned();
    editor.release().await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::LocalCommit))
        .await
        .unwrap();
    editor.submit(&local("Failed local input")).unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    assert!(matches!(
        editor.submit(&local("Latest unsent input")),
        Err(durable::Error::Pending)
    ));
    drop(editor);
    let editor = fixture
        .account
        .retained_editors()
        .unwrap()
        .into_iter()
        .find(|e| e.retained_id() == id)
        .unwrap();
    assert_eq!(
        editor.snapshot().pending.unwrap().composer.text,
        "Latest unsent input"
    );
    editor.retry_local_storage().unwrap();
    editor.flush().await.unwrap();
    assert_eq!(
        fixture
            .owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        "Latest unsent input"
    );
}
#[tokio::test]
async fn bounded_writer_admission_never_evicts_unresolved_slots() {
    let fixture = Fixture::new().await;
    let slots = (0..retention::MAX_EDITORS)
        .map(|_| fixture.account.reserve_editor().unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        fixture.account.reserve_editor(),
        Err(durable::Error::Pending)
    ));
    drop(slots);
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    editor.retire_clean().await.unwrap();
    assert!(fixture.account.retained_editors().unwrap().is_empty());
    assert!(matches!(
        editor.submit(&local("Retired writer")),
        Err(durable::Error::Stale)
    ));
}
#[tokio::test]
async fn explicit_handoff_repair_retries_exact_active_row_after_release_rollback() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::ReleaseCommit))
        .await
        .unwrap();
    assert!(matches!(
        editor.release().await,
        Err(durable::Error::Storage)
    ));
    assert!(fixture.owner.review().await.unwrap().same(&before));
    editor
        .submit(&local("Input retained during failed release"))
        .unwrap();
    editor.renew_lost_handoff().await.unwrap();
    editor.flush().await.unwrap();
    let local = fixture.owner.local_inventory().await.unwrap();
    assert!(local.row().is_none());
    assert_eq!(
        local.local().composer.text,
        "Input retained during failed release"
    );
}
#[tokio::test]
async fn committed_release_failed_start_repairs_by_fresh_local_inventory() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::StartLocalCommit))
        .await
        .unwrap();
    assert!(matches!(
        editor.release().await,
        Err(durable::Error::Storage)
    ));
    assert!(
        fixture
            .owner
            .local_inventory()
            .await
            .unwrap()
            .row()
            .is_none()
    );
    editor.renew_lost_handoff().await.unwrap();
    editor
        .submit(&local("Fresh generation after lost start"))
        .unwrap();
    editor.flush().await.unwrap();
    assert_eq!(
        fixture
            .owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        "Fresh generation after lost start"
    );
}
#[tokio::test]
async fn native_changed_input_becomes_origin_bearing_alternate_not_canonical() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let snapshot = editor.snapshot();
    editor
        .preserve_native_input(snapshot.serial, local("Native ahead of Rust"))
        .await
        .unwrap();
    let review = fixture.owner.review().await.unwrap();
    assert_eq!(review.local().composer.text, "Original");
    assert!(
        review
            .alternates()
            .iter()
            .any(|version| version.draft.composer.text == "Native ahead of Rust")
    );
    assert!(!review.origins().is_empty());
}

#[tokio::test]
async fn oversized_borrowed_input_remains_with_caller_and_never_advances_a_writer() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let candidate = local(&"x".repeat(retention::MAX_INPUT_BYTES));
    let before = editor.snapshot();
    let durable = fixture.owner.review().await.unwrap();
    assert!(matches!(
        editor.submit(&candidate),
        Err(durable::Error::Unsupported)
    ));
    assert_eq!(candidate.composer.text.len(), retention::MAX_INPUT_BYTES);
    assert_eq!(editor.snapshot().serial, before.serial);
    assert!(editor.snapshot().pending.is_none());
    assert!(fixture.owner.review().await.unwrap().same(&durable));
    editor.release().await.unwrap();
    let before = editor.snapshot();
    let durable = fixture.owner.local_inventory().await.unwrap();
    assert!(matches!(
        editor.submit(&candidate),
        Err(durable::Error::Unsupported)
    ));
    assert_eq!(editor.snapshot().serial, before.serial);
    assert!(editor.snapshot().pending.is_none());
    assert!(
        fixture
            .owner
            .local_inventory()
            .await
            .unwrap()
            .same(&durable)
    );
}
