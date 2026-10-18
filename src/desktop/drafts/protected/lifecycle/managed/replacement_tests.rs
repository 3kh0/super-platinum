//! Actual isolated Owned worker/Host/Editor. These tests exercise the existing
//! private final install transaction, not a fabricated native Admission.
use super::*;

async fn installed(f: &mut Fixture) -> (Installed, Arc<Editor>) {
    let (opened, editor) = f.editor().await;
    let installed = f
        .host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
        .await
        .unwrap();
    (installed, editor)
}
fn replace(f: &mut Fixture) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(f.account.id().into());
    shell.core.account_epoch = 9;
    f.shell = shell;
    f.selected = f.host.select(&f.shell, f.account.clone()).unwrap();
}

#[tokio::test]
async fn replacement_typing_is_owned_before_explicit_same_editor_rebind() {
    let mut f = Fixture::new().await;
    let (prior, editor) = installed(&mut f).await;
    let original_writer = editor.snapshot().writer_id;
    let old_selection = f.selected.clone();
    replace(&mut f);
    let gate = editor.gate.lock().await;
    let first = local("First replacement input");
    let mut last = local("Newest \u{1f680} replacement input");
    last.composer.set_selection(0, 6);
    let a = f
        .host
        .submit(&f.shell, &f.selected, &key(), &first)
        .unwrap();
    let b = f.host.submit(&f.shell, &f.selected, &key(), &last).unwrap();
    assert!(b > a);
    assert_eq!(editor.snapshot().serial, b);
    assert!(same_local(
        editor.snapshot().pending.as_ref().unwrap(),
        &last
    ));
    assert!(same_local(
        &f.host.managed_presentations(&f.account).unwrap()[&key()],
        &last
    ));
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    assert!(
        f.host
            .submit(&f.shell, &old_selection, &key(), &local("Stale callback"))
            .is_err()
    );
    assert_eq!(editor.snapshot().serial, b);
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    assert!(f.host.retained_locals(&f.account).is_empty());
    drop(gate);
    editor.flush().await.unwrap();
    f.reload().await;
    let current = f.owner.review().await.unwrap();
    assert!(same_local(current.local(), &last));
    let rebound = f
        .host
        .rebind_without_native_for_managed_test(&f.shell, &f.selected, &prior)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(rebound.editor(), &editor));
    assert_eq!(rebound.rebound_from(), Some(prior.id()));
    assert_eq!(editor.snapshot().writer_id, original_writer);
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    assert!(
        f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    assert!(f.owner.review().await.unwrap().same(&current));
}

#[tokio::test]
async fn failed_detached_input_and_newer_text_survive_another_replacement() {
    let mut f = Fixture::new().await;
    let (prior, editor) = installed(&mut f).await;
    replace(&mut f);
    Held::new(f.account.owned.as_ref().unwrap().operations.fixture_fault(
        f.owner.scope().clone(),
        durable::test_support::FaultPoint::WriterCommit,
    ))
    // Fault injection has a result, but is not a simple-success retirement family.
    .value()
    .await
    .unwrap()
    .unwrap();
    let failed = local("Exact failed input");
    f.host
        .submit(&f.shell, &f.selected, &key(), &failed)
        .unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    let newest = local("Newer input remains owned");
    f.host
        .submit(&f.shell, &f.selected, &key(), &newest)
        .unwrap();
    replace(&mut f);
    f.reload().await;
    let snapshot = editor.snapshot();
    assert!(same_local(&snapshot.failed_inputs[0], &failed));
    assert!(same_local(snapshot.pending.as_ref().unwrap(), &newest));
    assert!(same_local(
        &f.host.managed_presentations(&f.account).unwrap()[&key()],
        &newest
    ));
    assert!(
        f.host
            .rebind_without_native_for_managed_test(&f.shell, &f.selected, &prior,)
            .await
            .is_err()
    );
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    assert!(f.host.retained_locals(&f.account).is_empty());
}

#[tokio::test]
async fn mismatched_registry_or_competing_editor_cannot_admit_retained_route_input() {
    let mut f = Fixture::new().await;
    let (_, editor) = installed(&mut f).await;
    replace(&mut f);
    f.reload().await;
    let registry = f.host.discovery.registry(&f.shell).unwrap();
    let serial = editor.snapshot().serial;
    registry.select(f.account.id().into(), 10);
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &local("Wrong epoch"))
            .is_err()
    );
    registry.select("different-account".into(), 9);
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &local("Wrong account"))
            .is_err()
    );
    registry.select(f.account.id().into(), 9);
    let other = Editor::open_expected(f.owner.clone(), f.owner.review().await.unwrap())
        .await
        .unwrap();
    registry
        .replace(f.account.id(), 9, &key(), None, other.clone())
        .unwrap();
    let other_serial = other.snapshot().serial;
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &local("Competing owner"))
            .is_err()
    );
    assert_eq!(editor.snapshot().serial, serial);
    assert_eq!(other.snapshot().serial, other_serial);
    assert!(editor.snapshot().pending.is_none());
    assert!(other.snapshot().pending.is_none());
    assert!(registry.current(f.account.id(), 9, &key(), &other));
}
