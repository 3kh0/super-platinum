//! Closed memory handoff + real FIFO tests. These do NOT claim native admission.
//! The public commit accepts only a real Admission; tests call the private
//! transaction after actual worker registration/read to exercise interleavings.
use super::super::super::account::wait;
use super::*;
use durable::test_support::{FaultPoint, Isolated};
use super_platinum_core::ComposerState;
pub(in super::super) struct Fixture {
    pub(in super::super) isolated: Isolated,
    pub(in super::super) account: Arc<Account>,
    pub(in super::super) shell: ShellState,
    pub(in super::super) host: Host,
    pub(in super::super) selected: Selection,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn key(i: usize) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: format!("C{i}"),
        thread: None,
    }
}
impl Fixture {
    pub(in super::super) async fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = "00000000-0000-4000-8000-000000000281";
        let path = std::env::temp_dir().join(format!(
            "protected-install-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), isolated.client());
        account.initialize().await.unwrap();
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(id.into());
        shell.core.account_epoch = 9;
        let mut host = Host::default();
        let selected = host.select(&shell, account.clone()).unwrap();
        Self {
            isolated,
            account,
            shell,
            host,
            selected,
        }
    }
    pub(in super::super) async fn registered(
        &mut self,
        i: usize,
    ) -> (LocalOpen, Arc<Lane>, Adoption, durable::Review) {
        let owner = self.account.owner(key(i)).unwrap();
        let inventory = owner.local_inventory().await.unwrap();
        let opened = self
            .host
            .open_local(&self.shell, &self.selected, inventory)
            .unwrap();
        opened.ready().await;
        let lane = self.host.local(&opened).unwrap();
        lane.writer().submit(&local("Original")).unwrap();
        lane.writer().flush().await.unwrap();
        let expected = wait(self.account.client().unwrap().enroll_local(
            owner.scope().clone(),
            owner.local_inventory().await.unwrap(),
        ))
        .await
        .unwrap();
        let load = self.host.begin_load(&self.shell).unwrap();
        load.ready().await;
        self.host.finish_load(&self.shell, &load).unwrap();
        let adopted = self
            .host
            .begin_adoption(
                &self.shell,
                &self.selected,
                &opened,
                expected.clone(),
                Visit::new(),
            )
            .unwrap();
        adopted.ready().await;
        (opened, lane, adopted, expected)
    }
    pub(in super::super) fn request(&self, a: &Adoption) -> InstallRequest {
        self.host
            .install_request(&self.shell, a, None, Visit::new())
            .unwrap()
    }
    pub(in super::super) async fn installed(&mut self) -> Installed {
        let (_, _, a, _) = self.registered(0).await;
        let ready = self.request(&a).prepare().await.unwrap();
        let receipt = self.host.commit_installation(&self.shell, ready).unwrap();
        self.host.acknowledge_installation(&receipt).unwrap();
        receipt
    }
    pub(in super::super) async fn current(&self, i: usize) -> durable::Review {
        self.account.owner(key(i)).unwrap().review().await.unwrap()
    }
}
#[tokio::test]
async fn installation_changes_only_route_and_next_input_uses_registered_writer() {
    let mut f = Fixture::new().await;
    let (_opened, lane, a, expected) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    let receipt = f.host.commit_installation(&f.shell, ready).unwrap();
    assert!(f.current(0).await.same(&expected));
    assert!(matches!(
        lane.writer().submit(&local("Old clone")),
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        f.host.route_snapshot(&f.shell, &key(0)).unwrap().kind,
        "managed"
    );
    assert!(f.host.discovery.registry(&f.shell).unwrap().current(
        f.account.id(),
        9,
        &key(0),
        receipt.editor()
    ));
    f.host
        .submit(
            &f.shell,
            &f.selected,
            &key(0),
            &local("New registered input"),
        )
        .unwrap();
    receipt.editor().flush().await.unwrap();
    assert!(same_local(
        f.current(0).await.local(),
        &local("New registered input")
    ));
    assert_eq!(
        f.current(0).await.row().lineage().basis(),
        durable::Basis::Unverified
    );
}
#[tokio::test]
async fn late_registry_change_refuses_without_retiring_original_lane() {
    let mut f = Fixture::new().await;
    let (_, lane, a, expected) = f.registered(0).await;
    let result = f.host.adoption(&f.shell, &a).unwrap();
    let ready = f.request(&a).prepare().await.unwrap();
    let registry = f.host.discovery.registry(&f.shell).unwrap();
    registry
        .replace(f.account.id(), 9, &key(0), None, result.editor.clone())
        .unwrap();
    assert!(matches!(
        f.host.commit_installation(&f.shell, ready),
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        f.host.route_snapshot(&f.shell, &key(0)).unwrap().kind,
        "local"
    );
    assert!(f.current(0).await.same(&expected));
    lane.writer()
        .submit(&local("Preserved refused install"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    assert!(
        !f.account
            .owner(key(0))
            .unwrap()
            .local_inventory()
            .await
            .unwrap()
            .alternates()
            .is_empty()
    );
}
#[tokio::test]
async fn post_final_read_worker_race_preserves_next_input_via_full_cas() {
    let mut f = Fixture::new().await;
    let (_, _, a, expected) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    // Direct typed worker command intentionally bypasses the held desktop Owner gate.
    let client = f.account.client().unwrap();
    let writer = wait(client.register_writer(expected.clone()))
        .await
        .unwrap();
    let foreign = local("Independent worker input");
    let result = wait(client.checkpoint(writer, 1, expected, foreign.clone(), None))
        .await
        .unwrap();
    assert!(matches!(result, durable::WriteResult::Applied(_)));
    let receipt = f.host.commit_installation(&f.shell, ready).unwrap();
    f.host
        .submit(
            &f.shell,
            &f.selected,
            &key(0),
            &local("Native input after stale read"),
        )
        .unwrap();
    assert!(receipt.editor().flush().await.is_err());
    let current = f.current(0).await;
    assert!(same_local(current.local(), &foreign));
    assert!(
        current
            .alternates()
            .iter()
            .any(|a| same_local(&a.draft, &local("Native input after stale read")))
    );
}
#[tokio::test]
async fn late_local_serial_refuses_with_no_route_or_registry_mutation() {
    let mut f = Fixture::new().await;
    let (_, lane, a, _) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    lane.writer()
        .submit(&local("Typed while final read held"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    assert!(matches!(
        f.host.commit_installation(&f.shell, ready),
        Err(durable::Error::ReviewRequired)
    ));
    assert!(
        f.host
            .discovery
            .registry(&f.shell)
            .unwrap()
            .get(f.account.id(), 9, &key(0))
            .is_none()
    );
    assert_eq!(
        f.host.route_snapshot(&f.shell, &key(0)).unwrap().kind,
        "local"
    );
}
#[tokio::test]
async fn ready_cancellation_retains_original_registration_and_input_route() {
    let mut f = Fixture::new().await;
    let (_, lane, a, expected) = f.registered(0).await;
    let editor = f.host.adoption(&f.shell, &a).unwrap().editor;
    drop(f.request(&a).prepare().await.unwrap());
    assert_eq!(
        f.host.route_snapshot(&f.shell, &key(0)).unwrap().kind,
        "local"
    );
    assert!(Arc::ptr_eq(
        &editor,
        &f.host.adoption(&f.shell, &a).unwrap().editor
    ));
    assert!(f.current(0).await.same(&expected));
    let ready = f.request(&a).prepare().await.unwrap();
    let receipt = f.host.commit_installation(&f.shell, ready).unwrap();
    assert!(Arc::ptr_eq(&editor, receipt.editor()));
    assert!(matches!(
        lane.writer().submit(&local("Cannot revive")),
        Err(durable::Error::Stale)
    ));
}
#[tokio::test]
async fn duplicate_ready_and_cloned_adoption_never_install_twice() {
    let mut f = Fixture::new().await;
    let (_, _, a, _) = f.registered(0).await;
    let first = f.request(&a);
    let second = f.request(&a);
    let ready = first.prepare().await.unwrap();
    let installed = f.host.commit_installation(&f.shell, ready).unwrap();
    let duplicate = second.prepare().await.unwrap();
    assert!(matches!(
        f.host.commit_installation(&f.shell, duplicate),
        Err(durable::Error::Stale)
    ));
    f.host.acknowledge_installation(&installed).unwrap();
    assert!(f.host.retained_adoptions(&f.account).is_empty());
    assert!(matches!(
        f.host.adoption(&f.shell, &a),
        Err(durable::Error::Stale)
    ));
    assert!(matches!(
        f.host.acknowledge_installation(&installed),
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        f.host.installation(&f.account, &key(0)).unwrap().id(),
        installed.id()
    );
}
#[tokio::test]
async fn owner_replacement_refuses_ready_without_hidden_rebind() {
    let mut f = Fixture::new().await;
    let (_, _, a, _) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    let mut replacement = ShellState::fixture(crate::media::MediaRegistry::default());
    replacement.core.active_account = Some(f.account.id().into());
    replacement.core.account_epoch = 9;
    f.host.select(&replacement, f.account.clone()).unwrap();
    assert!(matches!(
        f.host.commit_installation(&replacement, ready),
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        f.host.route_snapshot(&replacement, &key(0)).unwrap().kind,
        "local"
    );
    assert!(
        f.host
            .discovery
            .registry(&replacement)
            .unwrap()
            .get(f.account.id(), 9, &key(0))
            .is_none()
    );
}
#[tokio::test]
async fn first_managed_write_failure_survives_replacement_and_exact_retry() {
    let mut f = Fixture::new().await;
    let (_, _, a, expected) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    let installed = f.host.commit_installation(&f.shell, ready).unwrap();
    f.host.acknowledge_installation(&installed).unwrap();
    wait(f.isolated.fail_next(FaultPoint::WriterCommit))
        .await
        .unwrap();
    f.host
        .submit(
            &f.shell,
            &f.selected,
            &key(0),
            &local("Failed managed input"),
        )
        .unwrap();
    assert!(matches!(
        installed.editor().flush().await,
        Err(durable::Error::Storage)
    ));
    assert!(f.current(0).await.same(&expected));
    let mut next = ShellState::fixture(crate::media::MediaRegistry::default());
    next.core.active_account = Some(f.account.id().into());
    next.core.account_epoch = 9;
    let selected = f.host.select(&next, f.account.clone()).unwrap();
    assert!(matches!(
        f.host
            .submit(&next, &selected, &key(0), &local("No hidden route")),
        Err(durable::Error::Stale)
    ));
    let retained = f.host.installation(&f.account, &key(0)).unwrap();
    assert!(Arc::ptr_eq(retained.editor(), installed.editor()));
    assert_eq!(retained.editor().snapshot().failed_inputs.len(), 1);
    retained.editor().retry_storage().unwrap();
    retained.editor().flush().await.unwrap();
    assert!(same_local(
        f.current(0).await.local(),
        &local("Failed managed input")
    ));
    assert!(
        !f.host
            .route_snapshot(&next, &key(0))
            .unwrap()
            .registry_current
    );
}
#[tokio::test]
async fn completed_job_reclamation_allows_more_than_job_bound_without_replay() {
    let mut f = Fixture::new().await;
    for i in 0..MAX_ADOPTIONS + 2 {
        let (_, _, a, _) = f.registered(i).await;
        let ready = f.request(&a).prepare().await.unwrap();
        let receipt = f.host.commit_installation(&f.shell, ready).unwrap();
        f.host.acknowledge_installation(&receipt).unwrap();
        assert!(f.host.retained_adoptions(&f.account).is_empty());
    }
    assert_eq!(f.host.routes.len(), MAX_ADOPTIONS + 2);
    assert!(f.host.locals.is_empty());
}
#[tokio::test]
async fn clean_editor_retirement_is_required_before_route_removal() {
    let mut f = Fixture::new().await;
    let (_, _, a, _) = f.registered(0).await;
    let ready = f.request(&a).prepare().await.unwrap();
    let receipt = f.host.commit_installation(&f.shell, ready).unwrap();
    f.host.acknowledge_installation(&receipt).unwrap();
    assert!(matches!(
        f.host.forget_retired_installation(&f.shell, &receipt),
        Err(durable::Error::Stale)
    ));
    receipt.editor().retire_clean().await.unwrap();
    f.host
        .forget_retired_installation(&f.shell, &receipt)
        .unwrap();
    assert!(f.host.routes.is_empty());
    assert!(f.host.installation(&f.account, &key(0)).is_none());
}

#[tokio::test]
async fn same_bytes_worker_receipt_follows_existing_typing_rebase_policy() {
    let mut f = Fixture::new().await;
    let (_, _, adoption, expected) = f.registered(0).await;
    let ready = f.request(&adoption).prepare().await.unwrap();
    let client = f.account.client().unwrap();
    let writer = wait(client.register_writer(expected.clone()))
        .await
        .unwrap();
    let durable::WriteResult::Applied(same_bytes) =
        wait(client.checkpoint(writer, 1, expected.clone(), expected.local().clone(), None))
            .await
            .unwrap()
    else {
        panic!("same-byte worker write was not applied")
    };
    assert!(!same_bytes.same(&expected));
    assert_eq!(
        same_bytes.row().state().local_revision(),
        expected.row().state().local_revision()
    );
    assert!(same_local(same_bytes.local(), expected.local()));
    let installed = f.host.commit_installation(&f.shell, ready).unwrap();
    // D does not broaden the inherited receipt-only typing rebase predicate.
    // Native same-text ABA is separately rejected by the actual capture counters.
    f.host
        .submit(
            &f.shell,
            &f.selected,
            &key(0),
            &local("After same-byte worker receipt"),
        )
        .unwrap();
    installed.editor().flush().await.unwrap();
    let current = f.current(0).await;
    assert!(same_local(
        current.local(),
        &local("After same-byte worker receipt")
    ));
    assert!(current.alternates().is_empty());
}
