//! Actual temporary Owned worker + Host + Editor. No HTTP and no native proof
//! claim: only the private final renderer-admission check is bypassed in tests.
use super::super::super::owned_route::{Held, o};
use super::*;
use super_platinum_core::ComposerState;
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
struct Fixture {
    account: Arc<Account>,
    owner: Owner,
    host: Host,
    shell: ShellState,
    selected: Selection,
    expected: durable::Review,
}
impl Fixture {
    async fn new() -> Self {
        let initialized = tokio::task::spawn_blocking(|| {
            let route = o::OwnedRoute::fresh_fixture().unwrap();
            let before = route.inspect().unwrap().wait().unwrap();
            route.commit(&before).unwrap().wait().unwrap();
            route
                .inspect()
                .unwrap()
                .wait()
                .unwrap()
                .open()
                .unwrap()
                .initialize_cold(&o::FactoryIdentity::default())
                .unwrap()
                .wait()
                .unwrap()
        })
        .await
        .unwrap();
        let account = Account::from_retained(initialized);
        let owner = account.owner(key()).unwrap();
        let route = account.owned.as_ref().unwrap();
        let inventory = route.local(owner.scope().clone()).await.unwrap();
        let expected = Held::new(route.operations.enroll(inventory))
            .observed_success()
            .await
            .unwrap();
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(account.id().into());
        shell.core.account_epoch = 9;
        let mut host = Host::default();
        let selected = host.select(&shell, account.clone()).unwrap();
        let read = host.begin_load(&shell).unwrap();
        read.ready().await;
        host.finish_load(&shell, &read).unwrap();
        Self {
            account,
            owner,
            host,
            shell,
            selected,
            expected,
        }
    }
    fn open(&mut self) -> ManagedOpen {
        self.host
            .begin_managed(&self.shell, &self.selected, self.expected.clone())
            .unwrap()
    }
    async fn editor(&mut self) -> (ManagedOpen, Arc<Editor>) {
        let opened = self.open();
        opened.ready().await;
        let editor = self.host.managed(&opened).unwrap();
        (opened, editor)
    }
    async fn reload(&mut self) {
        let read = self.host.begin_load(&self.shell).unwrap();
        read.ready().await;
        self.host.finish_load(&self.shell, &read).unwrap();
    }
    fn request(&self, opened: &ManagedOpen) -> ManagedInstallRequest {
        self.host
            .managed_install_request(&self.shell, &self.selected, opened, None, Visit::new())
            .unwrap()
    }
}
#[tokio::test]
async fn managed_open_joins_one_registration_and_preserves_unverified_basis() {
    let mut f = Fixture::new().await;
    let original = f.expected.clone();
    let opened = f.open();
    let joined = f.open();
    assert_eq!(opened.id(), joined.id());
    drop(opened);
    joined.ready().await;
    let editor = f.host.managed(&joined).unwrap();
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    let receipt = f
        .host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &joined)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&editor, receipt.editor()));
    assert!(f.owner.review().await.unwrap().same(&original));
    assert_eq!(original.row().lineage().basis(), durable::Basis::Unverified);
    assert!(f.host.retained_managed(&f.account).is_empty());
    assert!(f.host.managed(&joined).is_err());
    assert!(
        f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
    assert!(
        !f.account
            .owned
            .as_ref()
            .unwrap()
            .unsupported
            .load(Ordering::Acquire)
    );
}
#[tokio::test]
async fn opening_retains_latest_unadmitted_input_and_account_retirement_cannot_rewrite_it() {
    let mut f = Fixture::new().await;
    let gate = f.owner.gate.clone().lock_owned().await;
    let opened = f.open();
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &local("first"))
            .is_err()
    );
    let latest = local("latest \u{65e5}\u{1f680}");
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &latest)
            .is_err()
    );
    let original_selection = f.selected.clone();
    f.host.retire_selection();
    assert!(
        f.host
            .submit(&f.shell, &original_selection, &key(), &local("stale"))
            .is_err()
    );
    drop(gate);
    opened.ready().await;
    let editor = f.host.managed(&opened).unwrap();
    editor.flush().await.unwrap();
    assert!(same_local(f.owner.review().await.unwrap().local(), &latest));
    assert!(same_local(
        &f.host.managed_presentations(&f.account).unwrap()[&key()],
        &latest
    ));
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    f.selected = f.host.select(&f.shell, f.account.clone()).unwrap();
    f.reload().await;
    f.host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
        .await
        .unwrap();
}
#[tokio::test]
async fn actual_existing_editor_is_reused_but_multiple_writers_are_not_chosen() {
    let mut f = Fixture::new().await;
    let existing = Editor::open_expected(f.owner.clone(), f.expected.clone())
        .await
        .unwrap();
    let (opened, editor) = f.editor().await;
    assert!(Arc::ptr_eq(&existing, &editor));
    let _other = Editor::open_expected(f.owner.clone(), f.expected.clone())
        .await
        .unwrap();
    assert!(
        f.host
            .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
            .await
            .is_err()
    );
    assert_eq!(f.account.retained_editors().unwrap().len(), 2);
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
}
#[tokio::test]
async fn input_after_final_read_refuses_install_and_keeps_same_opening_editor() {
    let mut f = Fixture::new().await;
    let (opened, editor) = f.editor().await;
    let ready = f.request(&opened).prepare().await.unwrap();
    f.host
        .submit(&f.shell, &f.selected, &key(), &local("after read"))
        .unwrap();
    assert!(f.host.commit_managed_installation(&f.shell, ready).is_err());
    editor.flush().await.unwrap();
    assert!(same_local(
        f.owner.review().await.unwrap().local(),
        &local("after read")
    ));
    assert!(Arc::ptr_eq(&f.host.managed(&opened).unwrap(), &editor));
    f.host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
        .await
        .unwrap();
}
#[tokio::test]
async fn canceled_install_read_is_retryable_but_old_selection_never_revives() {
    let mut f = Fixture::new().await;
    let (opened, editor) = f.editor().await;
    drop(f.request(&opened).prepare().await.unwrap());
    let ready = f.request(&opened).prepare().await.unwrap();
    let mut replacement = ShellState::fixture(crate::media::MediaRegistry::default());
    replacement.core.active_account = Some(f.account.id().into());
    replacement.core.account_epoch = 9;
    f.shell = replacement;
    f.selected = f.host.select(&f.shell, f.account.clone()).unwrap();
    assert!(f.host.commit_managed_installation(&f.shell, ready).is_err());
    f.reload().await;
    let installed = f
        .host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(installed.editor(), &editor));
    assert!(
        f.host
            .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn independent_writer_change_rejects_stale_review_without_new_registration() {
    let mut f = Fixture::new().await;
    let (opened, _) = f.editor().await;
    let request = f.request(&opened);
    let other = Editor::open_expected(f.owner.clone(), f.expected.clone())
        .await
        .unwrap();
    other.submit(&local("independent actual writer")).unwrap();
    other.flush().await.unwrap();
    assert!(request.prepare().await.is_err());
    assert!(f.host.managed(&opened).is_ok());
    assert!(
        !f.host
            .route_snapshot(&f.shell, &key())
            .unwrap()
            .registry_current
    );
}
#[tokio::test]
async fn failed_expected_open_retains_raw_review_and_new_input_without_local_fallback() {
    let mut f = Fixture::new().await;
    let gate = f.owner.gate.clone().lock_owned().await;
    let opened = f.open();
    let route = f.account.owned.as_ref().unwrap();
    let writer = Held::new(route.operations.register(f.expected.clone()))
        .observed_success()
        .await
        .unwrap();
    let checkpoint = Held::new(route.operations.checkpoint(o::CheckpointInput {
        serial: writer.initial_serial() + 1,
        writer,
        expected: f.expected.clone(),
        local: local("independent"),
        direct: None,
    }));
    checkpoint.submit().unwrap();
    assert_eq!(checkpoint.wait().await.unwrap(), o::JobState::Ready);
    let (observed, acknowledgment) = checkpoint.job().unwrap().observe_checkpoint().unwrap();
    assert!(same_local(observed.local(), &local("independent")));
    acknowledgment
        .acknowledge()
        .map_err(|(error, _)| error)
        .unwrap();
    assert!(
        f.host
            .submit(&f.shell, &f.selected, &key(), &local("retain untouched"))
            .is_err()
    );
    drop(gate);
    opened.ready().await;
    assert!(f.host.managed(&opened).is_err());
    assert!(same_local(
        &f.host.managed_presentations(&f.account).unwrap()[&key()],
        &local("retain untouched")
    ));
    assert!(f.host.managed[opened.id()].expected.same(&f.expected));
    assert!(f.host.retained_locals(&f.account).is_empty());
    assert_eq!(f.account.retained_openings().unwrap().len(), 1);
}

#[path = "remote_tests.rs"]
mod remote_tests;
#[path = "replacement_tests.rs"]
mod replacement_tests;
#[tokio::test]
async fn failed_checkpoint_keeps_exact_retry_and_newer_input_until_explicit_storage_repair() {
    let mut f = Fixture::new().await;
    let (opened, editor) = f.editor().await;
    let route = f.account.owned.as_ref().unwrap();
    Held::new(route.operations.fixture_fault(
        f.owner.scope().clone(),
        durable::test_support::FaultPoint::WriterCommit,
    ))
    // Fault injection has a result, but is not a simple-success retirement family.
    .value()
    .await
    .unwrap()
    .unwrap();
    f.host
        .submit(
            &f.shell,
            &f.selected,
            &key(),
            &local("failed immutable command"),
        )
        .unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    f.host
        .submit(&f.shell, &f.selected, &key(), &local("newer input"))
        .unwrap();
    assert!(
        f.host
            .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
            .await
            .is_err()
    );
    let snapshot = editor.snapshot();
    assert!(same_local(
        &snapshot.failed_inputs[0],
        &local("failed immutable command")
    ));
    assert!(same_local(
        snapshot.pending.as_ref().unwrap(),
        &local("newer input")
    ));
    assert!(same_local(
        &f.host.managed_presentations(&f.account).unwrap()[&key()],
        &local("newer input")
    ));
    editor.retry_storage().unwrap();
    editor.flush().await.unwrap();
    let installed = f
        .host
        .install_managed_without_native_for_test(&f.shell, &f.selected, &opened)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&editor, installed.editor()));
    assert!(same_local(
        f.owner.review().await.unwrap().local(),
        &local("newer input")
    ));
}
