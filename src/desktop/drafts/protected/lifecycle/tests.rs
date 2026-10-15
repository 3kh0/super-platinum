//! Real isolated worker results only; no fabricated Review/Writer/receipt.
use super::super::account::wait;
use super::*;
use durable::test_support::{FaultPoint, Isolated};
use super_platinum_core::ComposerState;
const ID: &str = "00000000-0000-4000-8000-000000000251";
const OTHER: &str = "00000000-0000-4000-8000-000000000252";
struct Fixture {
    isolated: Isolated,
    account: Arc<Account>,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn key(index: usize) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: format!("C{index}"),
        thread: None,
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
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "protected-lifecycle-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), isolated.client());
        account.initialize().await.unwrap();
        Self { isolated, account }
    }
    fn owner(&self, index: usize) -> Owner {
        self.account.owner(key(index)).unwrap()
    }
    async fn inventory(&self, index: usize) -> durable::LocalInventory {
        self.owner(index).local_inventory().await.unwrap()
    }
    async fn lane(
        &self,
        host: &mut Host,
        shell: &ShellState,
        selected: &Selection,
        index: usize,
    ) -> (LocalOpen, Arc<Lane>) {
        let opened = host
            .open_local(shell, selected, self.inventory(index).await)
            .unwrap();
        opened.ready().await;
        let lane = host.local(&opened).unwrap();
        lane.writer()
            .submit(&local("Original \u{65e5}\u{672c} \u{1f680}"))
            .unwrap();
        lane.writer().flush().await.unwrap();
        (opened, lane)
    }
    async fn enroll(&self, index: usize) -> durable::Review {
        wait(self.account.client().unwrap().enroll_local(
            self.owner(index).scope().clone(),
            self.inventory(index).await,
        ))
        .await
        .unwrap()
    }
}
async fn reload(host: &mut Host, shell: &ShellState) {
    let w = host.begin_load(shell).unwrap();
    w.ready().await;
    host.finish_load(shell, &w).unwrap();
}

#[tokio::test]
async fn canceled_open_waiter_recovers_same_lane_without_new_lease() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    drop(h.open_local(&s, &selected, f.inventory(0).await).unwrap());
    let saved = h.retained_locals(&f.account).pop().unwrap();
    saved.ready().await;
    let first = h.local(&saved).unwrap();
    let before = f.inventory(0).await;
    assert!(Arc::ptr_eq(&first, &h.local(&saved).unwrap()));
    assert!(f.inventory(0).await.same(&before));
    assert!(matches!(
        h.open_local(&s, &selected, before),
        Err(durable::Error::ReviewRequired)
    ));
}
#[tokio::test]
async fn replacement_and_account_aba_never_revive_selection_or_drop_failed_input() {
    let f = Fixture::new(ID).await;
    let other = Fixture::new(OTHER).await;
    let a = shell(ID);
    let b = shell(OTHER);
    let mut h = Host::default();
    let selected = h.select(&a, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &a, &selected, 0).await;
    wait(f.isolated.fail_next(FaultPoint::LocalCommit))
        .await
        .unwrap();
    lane.writer().submit(&local("Failed input")).unwrap();
    assert!(matches!(
        lane.writer().flush().await,
        Err(durable::Error::Storage)
    ));
    let weak = Arc::downgrade(&lane);
    drop(lane);
    h.select(&b, other.account.clone()).unwrap();
    let untouched = other.inventory(0).await;
    let replacement = shell(ID);
    h.select(&replacement, f.account.clone()).unwrap();
    assert!(!selected.current(&a));
    let recovered = h.local(&opened).unwrap();
    assert!(Arc::ptr_eq(&recovered, &weak.upgrade().unwrap()));
    assert_eq!(recovered.writer().snapshot().failed_inputs.len(), 1);
    recovered.writer().retry_oldest_storage().unwrap();
    recovered.writer().flush().await.unwrap();
    assert!(same_local(
        f.inventory(0).await.local(),
        &local("Failed input")
    ));
    assert!(other.inventory(0).await.same(&untouched));
}
#[tokio::test]
async fn wrong_account_and_same_epoch_new_view_refuse_old_admission() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let wrong = shell(OTHER);
    let mut h = Host::default();
    assert!(matches!(
        h.select(&wrong, f.account.clone()),
        Err(durable::Error::Stale)
    ));
    let old = h.select(&s, f.account.clone()).unwrap();
    let next = shell(ID);
    h.select(&next, f.account.clone()).unwrap();
    let before = f.inventory(0).await;
    assert!(matches!(
        h.open_local(&s, &old, before.clone()),
        Err(durable::Error::Stale)
    ));
    assert!(f.inventory(0).await.same(&before));
}
#[tokio::test]
async fn stale_local_clock_cannot_mint_replacement_lane() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let before = f.inventory(0).await;
    let lease = wait(f.account.client().unwrap().renew_local(before.clone()))
        .await
        .unwrap();
    drop(lease);
    let current = f.inventory(0).await;
    assert!(!current.same(&before));
    assert!(same_local(before.local(), current.local()));
    let opened = h.open_local(&s, &selected, before).unwrap();
    opened.ready().await;
    assert!(matches!(h.local(&opened), Err(durable::Error::Stale)));
    assert!(f.inventory(0).await.same(&current));
}
#[tokio::test]
async fn local_capacity_refuses_before_work_and_repairs_at_capacity() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let mut retained = vec![];
    for i in 0..MAX_LOCAL_LANES {
        retained.push(f.lane(&mut h, &s, &selected, i).await);
    }
    let before = f.inventory(MAX_LOCAL_LANES).await;
    assert!(matches!(
        h.open_local(&s, &selected, before.clone()),
        Err(durable::Error::Pending)
    ));
    assert!(f.inventory(MAX_LOCAL_LANES).await.same(&before));
    let (opened, lane) = &retained[0];
    wait(f.isolated.fail_next(FaultPoint::LocalCommit))
        .await
        .unwrap();
    lane.writer().submit(&local("Repair at capacity")).unwrap();
    assert!(lane.writer().flush().await.is_err());
    assert!(h.retire_local(opened).await.is_err());
    lane.writer().retry_oldest_storage().unwrap();
    lane.writer().flush().await.unwrap();
    h.retire_local(opened).await.unwrap();
    assert!(matches!(
        lane.writer().submit(&local("Detached clone")),
        Err(durable::Error::Stale)
    ));
    let next = h.open_local(&s, &selected, before).unwrap();
    next.ready().await;
    assert!(h.local(&next).is_ok());
}
#[tokio::test]
async fn clean_retirement_is_explicit_and_old_handle_cannot_write() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &s, &selected, 0).await;
    let writer = lane.writer().clone();
    let before = f.inventory(0).await;
    h.retire_local(&opened).await.unwrap();
    assert!(h.retained_locals(&f.account).is_empty());
    assert!(matches!(
        writer.submit(&local("Lost old route")),
        Err(durable::Error::Stale)
    ));
    assert!(f.inventory(0).await.same(&before));
}
#[tokio::test]
async fn enrollment_never_authorizes_local_lane_eviction() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &s, &selected, 0).await;
    f.enroll(0).await;
    assert!(matches!(
        h.retire_local(&opened).await,
        Err(durable::Error::Stale)
    ));
    lane.writer()
        .submit(&local("Late input after enrollment"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    let inventory = f.inventory(0).await;
    assert!(!inventory.alternates().is_empty());
    assert!(h.local(&opened).is_ok());
}
#[tokio::test]
async fn exact_open_rejects_changed_review_and_retains_newer_input() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    let editor = editor::Editor::open_expected(f.owner(0), expected.clone())
        .await
        .unwrap();
    editor.submit(&local("Newer managed text")).unwrap();
    editor.flush().await.unwrap();
    let before = f.owner(0).review().await.unwrap();
    assert!(matches!(
        editor::Editor::open_expected(f.owner(0), expected).await,
        Err(durable::Error::Stale)
    ));
    assert!(f.owner(0).review().await.unwrap().same(&before));
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
}
#[tokio::test]
async fn exact_open_rejects_identical_text_reenrollment() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    f.lane(&mut h, &s, &selected, 0).await;
    let old = f.enroll(0).await;
    let editor = editor::Editor::open_expected(f.owner(0), old.clone())
        .await
        .unwrap();
    editor.release().await.unwrap();
    let new = f.enroll(0).await;
    assert!(same_local(old.local(), new.local()));
    assert!(!old.same(&new));
    assert!(matches!(
        editor::Editor::open_expected(f.owner(0), old).await,
        Err(durable::Error::Stale)
    ));
    assert!(f.owner(0).review().await.unwrap().same(&new));
}
#[tokio::test]
async fn canceled_adoption_waiter_recovers_original_editor_without_registry_install() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    drop(
        h.begin_adoption(&s, &selected, &opened, expected.clone(), Visit::new())
            .unwrap(),
    );
    let request = h.retained_adoptions(&f.account).pop().unwrap();
    request.ready().await;
    let first = h.adoption(&s, &request).unwrap();
    assert!(first.current);
    assert!(first.expected.same(&expected));
    let second = h.adoption(&s, &request).unwrap();
    assert!(Arc::ptr_eq(&first.editor, &second.editor));
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    assert!(
        h.discovery
            .registry(&s)
            .unwrap()
            .get(ID, 7, &key(0))
            .is_none()
    );
    assert!(f.owner(0).review().await.unwrap().same(&expected));
    assert!(s.core.session.is_none());
    assert!(s.core.transport.is_none());
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, expected, Visit::new()),
        Err(durable::Error::Pending)
    ));
}
#[tokio::test]
async fn completed_registration_survives_replacement_but_never_becomes_current_again() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let job = h
        .begin_adoption(&s, &selected, &opened, expected, Visit::new())
        .unwrap();
    job.ready().await;
    let first = h.adoption(&s, &job).unwrap();
    let next = shell(ID);
    h.select(&next, f.account.clone()).unwrap();
    let recovered = h.adoption(&next, &job).unwrap();
    assert!(!recovered.current);
    assert!(Arc::ptr_eq(&first.editor, &recovered.editor));
    h.select(&s, f.account.clone()).unwrap();
    assert!(!h.adoption(&s, &job).unwrap().current);
}
#[tokio::test]
async fn typing_after_registration_preserves_input_and_invalidates_completion_freshness() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let job = h
        .begin_adoption(&s, &selected, &opened, expected, Visit::new())
        .unwrap();
    job.ready().await;
    lane.writer()
        .submit(&local("Late \u{1f680} input"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    assert!(!h.adoption(&s, &job).unwrap().current);
    assert!(!f.inventory(0).await.alternates().is_empty());
    assert!(h.retire_local(&opened).await.is_err());
}
#[tokio::test]
async fn failed_lane_and_spent_visit_refuse_registration() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let visit = Visit::new();
    visit.retire();
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, expected.clone(), visit),
        Err(durable::Error::Stale)
    ));
    lane.writer()
        .submit(&local("Preserved before registration"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, expected, Visit::new()),
        Err(durable::Error::ReviewRequired)
    ));
    assert!(f.account.retained_editors().unwrap().is_empty());
}
#[tokio::test]
async fn adoption_capacity_refuses_before_register_but_existing_results_remain_recoverable() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let mut jobs = vec![];
    for i in 0..MAX_ADOPTIONS {
        let (opened, _) = f.lane(&mut h, &s, &selected, i).await;
        let expected = f.enroll(i).await;
        reload(&mut h, &s).await;
        let job = h
            .begin_adoption(&s, &selected, &opened, expected, Visit::new())
            .unwrap();
        job.ready().await;
        assert!(h.adoption(&s, &job).is_ok());
        jobs.push(job);
    }
    let (opened, _) = f.lane(&mut h, &s, &selected, MAX_ADOPTIONS).await;
    let expected = f.enroll(MAX_ADOPTIONS).await;
    reload(&mut h, &s).await;
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, expected.clone(), Visit::new()),
        Err(durable::Error::Pending)
    ));
    assert_eq!(f.account.retained_editors().unwrap().len(), MAX_ADOPTIONS);
    assert!(
        f.owner(MAX_ADOPTIONS)
            .review()
            .await
            .unwrap()
            .same(&expected)
    );
    for job in jobs {
        assert!(h.adoption(&s, &job).is_ok());
    }
}

async fn registration_reserved(account: &Arc<Account>) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if account
                .editors
                .lock()
                .unwrap()
                .values()
                .any(Option::is_none)
            {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("registration reached existing Owner gate");
}
#[tokio::test]
async fn canceled_exact_open_waiter_does_not_lose_registered_editor() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    let owner = f.owner(0);
    let guard = owner.gate.clone().lock_owned().await;
    let task = tokio::spawn(Box::pin(editor::Editor::open_expected(
        owner,
        expected.clone(),
    )));
    registration_reserved(&f.account).await;
    task.abort();
    assert!(matches!(task.await,Err(error) if error.is_cancelled()));
    drop(guard);
    let retained = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let entries = f.account.retained_editors().unwrap();
            if let Some(editor) = entries.first() {
                return editor.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        matches!(retained.snapshot().view.inventory(),Inventory::Managed(r) if r.same(&expected))
    );
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    retained.submit(&local("Input after lost waiter")).unwrap();
    retained.flush().await.unwrap();
    assert!(same_local(
        f.owner(0).review().await.unwrap().local(),
        &local("Input after lost waiter")
    ));
}
#[tokio::test]
async fn typing_while_registration_waits_refuses_exact_review_and_retains_original_input() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, lane) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let owner = f.owner(0);
    let guard = owner.gate.clone().lock_owned().await;
    let job = h
        .begin_adoption(&s, &selected, &opened, expected, Visit::new())
        .unwrap();
    registration_reserved(&f.account).await;
    lane.writer()
        .submit(&local("During registration wait"))
        .unwrap();
    assert!(lane.writer().flush().await.is_err());
    // Read through the typed FIFO directly; Owner.gate is intentionally held.
    let before = wait(
        f.account
            .client()
            .unwrap()
            .review_local(owner.scope().clone()),
    )
    .await
    .unwrap();
    assert!(!before.alternates().is_empty());
    drop(guard);
    job.ready().await;
    assert!(matches!(h.adoption(&s, &job), Err(durable::Error::Stale)));
    assert!(f.account.retained_editors().unwrap().is_empty());
    assert!(f.inventory(0).await.same(&before));
    assert!(
        h.local(&opened)
            .unwrap()
            .writer()
            .snapshot()
            .pending
            .is_some()
    );
    // Recovery of a failed job must not rerun registration.
    let recovered = h.retained_adoptions(&f.account).pop().unwrap();
    recovered.ready().await;
    assert!(matches!(
        h.adoption(&s, &recovered),
        Err(durable::Error::Stale)
    ));
    assert!(f.account.retained_editors().unwrap().is_empty());
}
#[tokio::test]
async fn selection_retired_while_registration_waits_retains_completion_without_adoption() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let guard = f.owner(0).gate.clone().lock_owned().await;
    let job = h
        .begin_adoption(&s, &selected, &opened, expected, Visit::new())
        .unwrap();
    registration_reserved(&f.account).await;
    let next = shell(ID);
    h.select(&next, f.account.clone()).unwrap();
    drop(guard);
    job.ready().await;
    let result = h.adoption(&next, &job).unwrap();
    assert!(!result.current);
    assert_eq!(f.account.retained_editors().unwrap().len(), 1);
    assert!(
        h.discovery
            .registry(&next)
            .unwrap()
            .get(ID, 7, &key(0))
            .is_none()
    );
    assert!(h.local(&opened).is_ok());
}
#[tokio::test]
async fn retained_failed_editor_blocks_a_second_registration_without_dropping_either_lane() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    let editor = editor::Editor::open_expected(f.owner(0), expected.clone())
        .await
        .unwrap();
    wait(f.isolated.fail_next(FaultPoint::WriterCommit))
        .await
        .unwrap();
    editor.submit(&local("Failed registered input")).unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    let weak = Arc::downgrade(&editor);
    drop(editor);
    reload(&mut h, &s).await;
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, expected, Visit::new()),
        Err(durable::Error::ReviewRequired)
    ));
    assert_eq!(weak.upgrade().unwrap().snapshot().failed_inputs.len(), 1);
    assert!(h.local(&opened).is_ok());
}

#[tokio::test]
async fn retired_registered_editor_is_never_a_current_adoption_result() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    reload(&mut h, &s).await;
    let job = h
        .begin_adoption(&s, &selected, &opened, expected.clone(), Visit::new())
        .unwrap();
    job.ready().await;
    let result = h.adoption(&s, &job).unwrap();
    assert!(result.current);
    result.editor.retire_clean().await.unwrap();
    assert!(!h.adoption(&s, &job).unwrap().current);
    assert!(f.account.retained_editors().unwrap().is_empty());
    assert!(f.owner(0).review().await.unwrap().same(&expected));
    assert!(h.local(&opened).is_ok());
}
#[tokio::test]
async fn adoption_refuses_verified_rows_outside_first_enrollment_scope() {
    let f = Fixture::new(ID).await;
    let s = shell(ID);
    let mut h = Host::default();
    let selected = h.select(&s, f.account.clone()).unwrap();
    let (opened, _) = f.lane(&mut h, &s, &selected, 0).await;
    let expected = f.enroll(0).await;
    let client = f.account.client().unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    let verified = wait(client.verify_local(stream, expected)).await.unwrap();
    assert_eq!(verified.row().lineage().basis(), durable::Basis::Local);
    reload(&mut h, &s).await;
    assert!(matches!(
        h.begin_adoption(&s, &selected, &opened, verified.clone(), Visit::new()),
        Err(durable::Error::ReviewRequired)
    ));
    assert!(f.account.retained_editors().unwrap().is_empty());
    assert!(f.owner(0).review().await.unwrap().same(&verified));
}
