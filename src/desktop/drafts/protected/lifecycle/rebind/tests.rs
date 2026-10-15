//! Real worker/Editor results; native admission is independently exercised by
//! the feature-only native cohort, never fabricated in these transaction tests.
use super::super::install::tests::Fixture;
use super::*;
use super_platinum_core::ComposerState;
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C0".into(),
        thread: None,
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
async fn replace(f: &mut Fixture) -> Selection {
    let mut next = ShellState::fixture(crate::media::MediaRegistry::default());
    next.core.active_account = Some(f.account.id().into());
    next.core.account_epoch = 9;
    f.shell = next;
    let selected = f.host.select(&f.shell, f.account.clone()).unwrap();
    let load = f.host.begin_load(&f.shell).unwrap();
    load.ready().await;
    f.host.finish_load(&f.shell, &load).unwrap();
    selected
}
#[tokio::test]
async fn explicit_rebind_keeps_exact_editor_and_durable_inventory() {
    let mut f = Fixture::new().await;
    let original = f.installed().await;
    let expected = f.current(0).await;
    let selected = replace(&mut f).await;
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    let request = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap();
    let ready = request.prepare().await.unwrap();
    let rebound = f.host.commit_rebind(&f.shell, ready).unwrap();
    assert_ne!(rebound.id(), original.id());
    assert_eq!(rebound.rebound_from(), Some(original.id()));
    assert!(Arc::ptr_eq(original.editor(), rebound.editor()));
    assert!(f.current(0).await.same(&expected));
    f.host
        .submit(&f.shell, &selected, &key(), &local("After explicit return"))
        .unwrap();
    rebound.editor().flush().await.unwrap();
    assert!(same_local(
        f.current(0).await.local(),
        &local("After explicit return")
    ));
    assert!(
        f.host
            .rebind_request(&f.shell, &selected, &original, None, Visit::new())
            .is_err()
    );
}
#[tokio::test]
async fn canceled_rebind_read_keeps_detached_editor_until_fresh_attempt() {
    let mut f = Fixture::new().await;
    let original = f.installed().await;
    let selected = replace(&mut f).await;
    let ready = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    drop(ready);
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    assert!(Arc::ptr_eq(
        f.host.installation(&f.account, &key()).unwrap().editor(),
        original.editor()
    ));
    let ready = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    f.host.commit_rebind(&f.shell, ready).unwrap();
}
#[tokio::test]
async fn late_retained_input_refuses_rebind_without_erasing_input() {
    let mut f = Fixture::new().await;
    let original = f.installed().await;
    let selected = replace(&mut f).await;
    let ready = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    original
        .editor()
        .submit(&local("Retained newer input"))
        .unwrap();
    assert!(matches!(
        f.host.commit_rebind(&f.shell, ready),
        Err(durable::Error::Stale)
    ));
    original.editor().flush().await.unwrap();
    assert!(same_local(
        f.current(0).await.local(),
        &local("Retained newer input")
    ));
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
}
#[tokio::test]
async fn failed_input_requires_exact_retry_before_explicit_rebind() {
    let mut f = Fixture::new().await;
    let original = f.installed().await;
    account::wait(
        f.isolated
            .fail_next(durable::test_support::FaultPoint::WriterCommit),
    )
    .await
    .unwrap();
    original
        .editor()
        .submit(&local("Failure retained through return"))
        .unwrap();
    assert!(original.editor().flush().await.is_err());
    let selected = replace(&mut f).await;
    assert!(
        f.host
            .rebind_request(&f.shell, &selected, &original, None, Visit::new())
            .is_err()
    );
    original.editor().retry_storage().unwrap();
    original.editor().flush().await.unwrap();
    let load = f.host.begin_load(&f.shell).unwrap();
    load.ready().await;
    f.host.finish_load(&f.shell, &load).unwrap();
    let ready = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    let rebound = f.host.commit_rebind(&f.shell, ready).unwrap();
    assert!(Arc::ptr_eq(rebound.editor(), original.editor()));
    assert!(same_local(
        f.current(0).await.local(),
        &local("Failure retained through return")
    ));
}
#[tokio::test]
async fn late_registry_replacement_refuses_rebind_and_preserves_replacement() {
    let mut f = Fixture::new().await;
    let original = f.installed().await;
    let selected = replace(&mut f).await;
    let ready = f
        .host
        .rebind_request(&f.shell, &selected, &original, None, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    let registry = f.host.discovery.registry(&f.shell).unwrap();
    registry
        .replace(f.account.id(), 9, &key(), None, original.editor().clone())
        .unwrap();
    assert!(matches!(
        f.host.commit_rebind(&f.shell, ready),
        Err(durable::Error::Stale)
    ));
    assert!(registry.current(f.account.id(), 9, &key(), original.editor()));
    assert_eq!(
        f.host.installation(&f.account, &key()).unwrap().id(),
        original.id()
    );
}
