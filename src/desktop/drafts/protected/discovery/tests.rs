//! Real isolated FIFO/opaque inventories; no default store, network or UI hooks.
use super::super::account::wait;
use super::*;
use durable::test_support::{FaultPoint, Isolated};
use super_platinum_core::ComposerState;
const ID: &str = "00000000-0000-4000-8000-000000000241";
const OTHER: &str = "00000000-0000-4000-8000-000000000242";

struct Fixture {
    isolated: Isolated,
    account: Arc<Account>,
    owner: Owner,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn shell(id: &str) -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(id.into());
    shell.core.account_epoch = 7;
    shell
}
impl Fixture {
    async fn new(id: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "protected-discovery-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), isolated.client());
        account.initialize().await.unwrap();
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let this = Self {
            isolated,
            account,
            owner,
        };
        this.write("Original \u{65e5}\u{672c} \u{1f680}").await;
        this
    }
    async fn write(&self, text: &str) -> durable::LocalInventory {
        let inventory = wait(
            self.account
                .client()
                .unwrap()
                .review_local(self.owner.scope().clone()),
        )
        .await
        .unwrap();
        let lease = wait(
            self.account
                .client()
                .unwrap()
                .renew_local(inventory.clone()),
        )
        .await
        .unwrap();
        let serial = lease.initial_serial();
        match wait(self.account.client().unwrap().write_local(
            lease,
            serial,
            serial + 1,
            inventory.local().clone(),
            local(text),
        ))
        .await
        .unwrap()
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => panic!("isolated local input was not canonical"),
        }
    }
    async fn enroll(&self) -> durable::Review {
        let inventory = self.owner.local_inventory().await.unwrap();
        wait(
            self.account
                .client()
                .unwrap()
                .enroll_local(self.owner.scope().clone(), inventory),
        )
        .await
        .unwrap()
    }
    async fn discover(&self, shell: &ShellState) -> Discovery {
        let mut discovery = Discovery::default();
        discovery.select(shell, self.account.clone()).unwrap();
        reload(&mut discovery, shell).await;
        discovery
    }
}
async fn reload(discovery: &mut Discovery, shell: &ShellState) {
    let ready = discovery.begin_load(shell).unwrap();
    ready.ready().await;
    discovery.finish_load(shell, &ready).unwrap();
}

#[tokio::test]
async fn current_inventory_is_exact_and_never_enrolls_or_projects_composer() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let before = f.owner.local_inventory().await.unwrap();
    let discovery = f.discover(&shell).await;
    assert_eq!(discovery.status(&shell), Status::Ready);
    let view = discovery.entry(&shell, &f.owner.scope().key).unwrap();
    assert!(matches!(view.inventory(), Inventory::Local(value) if value.same(&before)));
    assert!(view.uncommitted().is_none());
    assert_eq!(
        discovery.keys(&shell).unwrap(),
        &[f.owner.scope().key.clone()]
    );
    assert!(discovery.retained(&shell).unwrap().is_empty());
    assert!(f.owner.local_inventory().await.unwrap().same(&before));
    assert!(shell.core.session.is_none());
    assert!(shell.core.transport.is_none());
}

#[tokio::test]
async fn replaced_shell_same_account_epoch_invalidates_completed_load_and_registry() {
    let f = Fixture::new(ID).await;
    let old = shell(ID);
    let next = shell(ID);
    assert_ne!(old.draft_writes.view_id, next.draft_writes.view_id);
    let mut d = Discovery::default();
    d.select(&old, f.account.clone()).unwrap();
    let registry = d.registry(&old).unwrap();
    let ready = d.begin_load(&old).unwrap();
    ready.ready().await;
    assert_eq!(d.status(&next), Status::Inactive);
    d.select(&next, f.account.clone()).unwrap();
    assert!(!Arc::ptr_eq(&registry, &d.registry(&next).unwrap()));
    assert_eq!(d.finish_load(&next, &ready), Err(durable::Error::Stale));
    assert_eq!(d.status(&next), Status::NeedsLoad);
    reload(&mut d, &next).await;
    assert!(d.entry(&next, &f.owner.scope().key).is_some());
}

#[tokio::test]
async fn account_aba_and_same_id_different_route_refuse_old_load() {
    let f = Fixture::new(ID).await;
    let other = Fixture::new(OTHER).await;
    let replacement = Fixture::new(ID).await;
    let a = shell(ID);
    let b = shell(OTHER);
    let mut d = Discovery::default();
    assert_eq!(d.select(&b, f.account.clone()), Err(durable::Error::Stale));
    d.select(&a, f.account.clone()).unwrap();
    let ready = d.begin_load(&a).unwrap();
    ready.ready().await;
    d.select(&b, other.account.clone()).unwrap();
    d.select(&a, f.account.clone()).unwrap();
    assert_eq!(d.finish_load(&a, &ready), Err(durable::Error::Stale));
    let ready = d.begin_load(&a).unwrap();
    ready.ready().await;
    d.select(&a, replacement.account.clone()).unwrap();
    assert_eq!(d.finish_load(&a, &ready), Err(durable::Error::Stale));
    reload(&mut d, &a).await;
}

#[tokio::test]
async fn canceled_load_waiter_resumes_same_result_once() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = Discovery::default();
    d.select(&shell, f.account.clone()).unwrap();
    drop(d.begin_load(&shell).unwrap());
    let ready = d.pending_load(&shell).unwrap();
    ready.ready().await;
    d.finish_load(&shell, &ready).unwrap();
    assert_eq!(d.finish_load(&shell, &ready), Err(durable::Error::Stale));
    assert!(d.pending_load(&shell).is_none());
}

#[tokio::test]
async fn explicit_enrollment_retains_full_local_without_granting_remote_authority() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = f.discover(&shell).await;
    let before = f.owner.local_inventory().await.unwrap();
    let visit = Visit::new();
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, visit.clone())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    let ready = prepared.enqueue(&mut d, &shell, |_| true).unwrap();
    assert!(!visit.current());
    assert_eq!(d.status(&shell), Status::NeedsLoad);
    ready.ready().await;
    let outcome = d.take_enrollment(&ready).unwrap();
    assert!(outcome.current(&d, &shell));
    assert!(outcome.expected().same(&before));
    let review = outcome.result().unwrap();
    assert!(same_local(review.local(), before.local()));
    assert_eq!(review.row().lineage().verified_epoch(), 0);
    assert_eq!(review.row().lineage().basis(), durable::Basis::Unverified);
    assert!(d.retained(&shell).unwrap().is_empty());
    assert!(d.entry(&shell, &f.owner.scope().key).is_none());
    reload(&mut d, &shell).await;
    assert!(!outcome.current(&d, &shell));
    assert!(
        d.entry(&shell, &f.owner.scope().key)
            .unwrap()
            .inventory()
            .managed()
            .unwrap()
            .same(review)
    );
}

#[tokio::test]
async fn final_guard_and_retired_visit_leave_exact_local_inventory_unchanged() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = f.discover(&shell).await;
    let before = f.owner.local_inventory().await.unwrap();
    let visit = Visit::new();
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, visit.clone())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    assert!(matches!(
        prepared.enqueue(&mut d, &shell, |_| false),
        Err(durable::Error::Stale)
    ));
    assert!(visit.current());
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, visit.clone())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    visit.retire();
    assert!(matches!(
        prepared.enqueue(&mut d, &shell, |_| true),
        Err(durable::Error::Stale)
    ));
    assert!(f.owner.local_inventory().await.unwrap().same(&before));
    assert!(d.pending_enrollment().is_none());
}

#[tokio::test]
async fn typing_before_prepare_refuses_old_full_inventory() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let d = f.discover(&shell).await;
    let request = d
        .enrollment(&shell, &f.owner.scope().key, Visit::new())
        .unwrap();
    let newer = f.write("Newer input").await;
    assert!(matches!(
        request.prepare().await,
        Err(durable::Error::Stale)
    ));
    assert!(f.owner.local_inventory().await.unwrap().same(&newer));
}

#[tokio::test]
async fn same_bytes_local_clock_aba_refuses_old_enrollment_review() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let d = f.discover(&shell).await;
    let request = d
        .enrollment(&shell, &f.owner.scope().key, Visit::new())
        .unwrap();
    let before = request.expected().clone();
    let after = f.write(&before.local().composer.text).await;
    assert!(same_local(before.local(), after.local()));
    assert!(!before.same(&after));
    assert!(matches!(
        request.prepare().await,
        Err(durable::Error::Stale)
    ));
}

#[tokio::test]
async fn replacement_shell_cannot_spend_previous_prepared_visit() {
    let f = Fixture::new(ID).await;
    let old = shell(ID);
    let next = shell(ID);
    let mut d = f.discover(&old).await;
    let visit = Visit::new();
    let prepared = d
        .enrollment(&old, &f.owner.scope().key, visit.clone())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    d.select(&next, f.account.clone()).unwrap();
    assert!(matches!(
        prepared.enqueue(&mut d, &next, |_| true),
        Err(durable::Error::Stale)
    ));
    assert!(visit.current());
    assert!(f.owner.local_inventory().await.unwrap().row().is_none());
}

#[tokio::test]
async fn fifo_enrollment_refuses_typing_after_prepare_without_overwrite_or_retry() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = f.discover(&shell).await;
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    // Another typed FIFO caller bypasses this desktop Owner gate. The worker
    // must still enforce the exact inventory CAS at the actual mutation.
    let newer = f.write("Typed after prepare").await;
    let ready = prepared.enqueue(&mut d, &shell, |_| true).unwrap();
    ready.ready().await;
    let outcome = d.take_enrollment(&ready).unwrap();
    assert!(matches!(outcome.result(), Err(durable::Error::Stale)));
    assert!(f.owner.local_inventory().await.unwrap().same(&newer));
    assert_eq!(d.status(&shell), Status::NeedsLoad);
    assert!(d.pending_enrollment().is_none());
}

#[tokio::test]
async fn canceled_enrollment_waiter_and_account_switch_retain_captured_result_only() {
    let f = Fixture::new(ID).await;
    let other = Fixture::new(OTHER).await;
    let a = shell(ID);
    let b = shell(OTHER);
    let mut d = f.discover(&a).await;
    let untouched = other.owner.local_inventory().await.unwrap();
    let prepared = d
        .enrollment(&a, &f.owner.scope().key, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    drop(prepared.enqueue(&mut d, &a, |_| true).unwrap());
    d.select(&b, other.account.clone()).unwrap();
    let ready = d.pending_enrollment().unwrap();
    ready.ready().await;
    let outcome = d.take_enrollment(&ready).unwrap();
    assert_eq!(outcome.account(), ID);
    assert!(!outcome.current(&d, &b));
    assert!(outcome.result().is_ok());
    assert!(matches!(
        d.take_enrollment(&ready),
        Err(durable::Error::Stale)
    ));
    assert!(
        other
            .owner
            .local_inventory()
            .await
            .unwrap()
            .same(&untouched)
    );
    assert_eq!(d.status(&b), Status::NeedsLoad);
    d.select(&a, f.account.clone()).unwrap();
    assert!(!outcome.current(&d, &a));
    reload(&mut d, &a).await;
    assert!(
        d.entry(&a, &f.owner.scope().key)
            .unwrap()
            .inventory()
            .managed()
            .is_ok()
    );
}

#[tokio::test]
async fn load_generation_change_prevents_prepared_enrollment_enqueue() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = f.discover(&shell).await;
    let before = f.owner.local_inventory().await.unwrap();
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    reload(&mut d, &shell).await;
    assert!(matches!(
        prepared.enqueue(&mut d, &shell, |_| true),
        Err(durable::Error::Stale)
    ));
    assert!(f.owner.local_inventory().await.unwrap().same(&before));
}

#[tokio::test]
async fn failed_editor_survives_replacement_and_is_recovered_by_exact_identity() {
    let f = Fixture::new(ID).await;
    f.enroll().await;
    let old = shell(ID);
    let next = shell(ID);
    let mut d = f.discover(&old).await;
    let editor = editor::Editor::open(f.owner.clone()).await.unwrap();
    let weak = Arc::downgrade(&editor);
    let id = editor.retained_id().to_owned();
    d.registry(&old)
        .unwrap()
        .replace(ID, 7, &f.owner.scope().key, None, editor.clone())
        .unwrap();
    wait(f.isolated.fail_next(FaultPoint::WriterCommit))
        .await
        .unwrap();
    editor.submit(&local("Failed newer input")).unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    d.retire();
    drop(editor);
    d.select(&next, f.account.clone()).unwrap();
    let retained = d.retained(&next).unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].retained_id(), id);
    assert!(Arc::ptr_eq(&retained[0], &weak.upgrade().unwrap()));
    assert_eq!(
        retained[0].snapshot().pending.unwrap().composer.text,
        "Failed newer input"
    );
    assert!(
        d.registry(&next)
            .unwrap()
            .get(ID, 7, &f.owner.scope().key)
            .is_none()
    );
    retained[0].retry_storage().unwrap();
    retained[0].flush().await.unwrap();
    assert_eq!(
        f.owner.review().await.unwrap().local().composer.text,
        "Failed newer input"
    );
}

#[tokio::test]
async fn all_same_scope_editors_are_exposed_and_released_writer_blocks_first_enrollment() {
    let f = Fixture::new(ID).await;
    f.enroll().await;
    let first = editor::Editor::open(f.owner.clone()).await.unwrap();
    let second = editor::Editor::open(f.owner.clone()).await.unwrap();
    first.release().await.unwrap();
    let shell = shell(ID);
    let d = f.discover(&shell).await;
    let retained = d.retained(&shell).unwrap();
    assert_eq!(retained.len(), 2);
    assert!(retained.iter().any(|v| Arc::ptr_eq(v, &first)));
    assert!(retained.iter().any(|v| Arc::ptr_eq(v, &second)));
    assert!(matches!(
        d.enrollment(&shell, &f.owner.scope().key, Visit::new()),
        Err(durable::Error::ReviewRequired)
    ));
    assert!(f.owner.local_inventory().await.unwrap().row().is_none());
}

#[tokio::test]
async fn dropped_completion_becomes_retained_error_and_cannot_generate_second_result() {
    let job = Job::<()>::new();
    let ready = Wait::new(job.clone());
    drop(job.completion());
    ready.ready().await;
    assert_eq!(job.take(), Ok(Err(durable::Error::Storage)));
    assert_eq!(job.take(), Err(durable::Error::Pending));
    let success = Job::<()>::new();
    success.completion().finish(Ok(()));
    assert_eq!(success.take(), Ok(Ok(())));
    assert_eq!(success.take(), Err(durable::Error::Pending));
}

#[tokio::test]
async fn detached_editor_registration_reservation_blocks_enrollment_before_and_after_prepare() {
    let f = Fixture::new(ID).await;
    let shell = shell(ID);
    let mut d = f.discover(&shell).await;
    let slot = f.account.reserve_editor().unwrap();
    assert!(matches!(
        d.enrollment(&shell, &f.owner.scope().key, Visit::new()),
        Err(durable::Error::Pending)
    ));
    drop(slot);
    let prepared = d
        .enrollment(&shell, &f.owner.scope().key, Visit::new())
        .unwrap()
        .prepare()
        .await
        .unwrap();
    let slot = f.account.reserve_editor().unwrap();
    assert!(matches!(
        prepared.enqueue(&mut d, &shell, |_| true),
        Err(durable::Error::Pending)
    ));
    drop(slot);
    assert!(f.owner.local_inventory().await.unwrap().row().is_none());
}
