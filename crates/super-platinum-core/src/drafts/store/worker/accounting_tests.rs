//! Real cycle tests with fresh owned temporary fixtures. No simulated drain.
use super::*;
use crate::drafts::store::protected::{self, upgrade};
use crate::drafts::store::sync::tests::local;
use std::sync::Arc;

// Compare the complete persisted payload, including selection and broadcast.
// These fixtures have no attachments, so no skipped upload-runtime fields exist.
#[track_caller]
fn assert_local_payload(actual: &LocalDraft, expected: &LocalDraft) {
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

struct Fixture {
    owner: Arc<upgrade::Owned>,
    worker: Worker,
}
impl Fixture {
    fn new() -> Self {
        let owner = upgrade::Owned::fresh().unwrap();
        let worker = Worker::default();
        let id = worker
            .upgrade_id
            .get_or_init(|| uuid::Uuid::new_v4().to_string());
        owner.bind_worker(id).unwrap();
        worker.pending.lock().unwrap().accounting = Some(Book::owned(owner.account(), id));
        Self { owner, worker }
    }
    fn account(&self) -> &str {
        self.owner.account()
    }
    fn scope(&self) -> Scope {
        Scope {
            account_id: self.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        }
    }
    fn open(&self, account: &str) -> Result<DraftStore, AppError> {
        assert_eq!(account, self.account());
        self.owner.check()?;
        DraftStore::open(self.owner.path())
    }
    fn snapshot(&self) -> Snapshot {
        let pending = self.worker.pending.lock().unwrap();
        pending.accounting.as_ref().unwrap().snapshot(&pending)
    }
    fn load(&self) -> mpsc::Receiver<Result<Entries, AppError>> {
        let (send, receive) = mpsc::sync_channel(1);
        self.worker
            .submit(Request::Load(self.account().into(), send));
        receive
    }
}

#[test]
fn identity_uses_actual_scope_review_upload_ticket_and_command() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    let (send, _) = mpsc::sync_channel(1);
    let command = Request::ReadLocalReview(scope.clone(), send);
    assert_eq!(command.identity().account, fixture.account());
    assert_eq!(command.identity().key, Some(&scope.key));
    let review = fixture
        .open(fixture.account())
        .unwrap()
        .read_local_review(&scope)
        .unwrap();
    let (send, _) = mpsc::sync_channel(1);
    let command = Request::CommitReviewedSchedule(
        review,
        crate::drafts::sync::Snapshot {
            blocks: vec![],
            file_ids: vec![],
            destinations: vec![],
        },
        42,
        send,
    );
    assert_eq!(command.identity().account, fixture.account());
    assert_eq!(command.identity().family, "reviewed-schedule");
    let mut draft = crate::drafts::store::uploads::tests::draft();
    draft.account_id = fixture.account().into();
    let stored = fixture
        .open(fixture.account())
        .unwrap()
        .reserve_upload(&draft)
        .unwrap()
        .stored;
    let (send, _) = mpsc::sync_channel(1);
    let reserve = Request::Upload(uploads::Command::Reserve(draft, send));
    let (send, _) = mpsc::sync_channel(1);
    let finalize = Request::Upload(uploads::Command::Finalize(
        stored.ticket.clone(),
        vec![],
        send,
    ));
    let (send, _) = mpsc::sync_channel(1);
    let retire = Request::Upload(uploads::Command::Retire(stored, send));
    let (send, _) = mpsc::sync_channel(1);
    let load = Request::Upload(uploads::Command::Load(fixture.account().into(), send));
    for (command, family) in [
        (reserve, "upload-reserve"),
        (finalize, "upload-finalize"),
        (retire, "upload-retire"),
        (load, "upload-load"),
    ] {
        assert_eq!(command.identity().account, fixture.account());
        assert_eq!(command.identity().family, family);
    }
    let (command, _) = upgrade::Command::new(fixture.owner.clone(), upgrade::Action::Inspect);
    assert_eq!(
        Request::StoreUpgrade(Box::new(command)).identity().account,
        fixture.account()
    );
    let (reply, _) = mpsc::channel();
    let command = Request::Protected(Box::new(protected::commands::Command {
        account: fixture.account().into(),
        action: protected::commands::Action::Load {
            mode: protected::LoadMode::Current,
            reply,
        },
    }));
    assert_eq!(command.identity().account, fixture.account());
}

#[test]
fn detached_future_request_is_visible_during_earlier_open() {
    let fixture = Fixture::new();
    let first = fixture.load();
    let second = fixture.load();
    let calls = std::cell::Cell::new(0);
    fixture.worker.cycle(|account| {
        if calls.get() == 0 {
            let snapshot = fixture.snapshot();
            assert_eq!(snapshot.pending_requests, 0);
            assert_eq!(snapshot.records.len(), 2);
            assert!(
                snapshot
                    .records
                    .iter()
                    .all(|row| row.stage == Stage::Detached)
            );
        }
        calls.set(calls.get() + 1);
        fixture.open(account)
    });
    first.recv().unwrap().unwrap();
    second.recv().unwrap().unwrap();
    assert_eq!(calls.get(), 2);
    assert!(
        fixture
            .snapshot()
            .records
            .iter()
            .all(|row| row.stage == Stage::FinishedClassified)
    );
}

#[test]
fn input_during_open_retains_detached_and_new_actual_payload() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    let request = fixture.load();
    fixture.worker.cycle(|account| {
        fixture.worker.enqueue(
            account.into(),
            [(scope.key.clone(), local("newer exact input"))].into(),
        );
        let snapshot = fixture.snapshot();
        assert_eq!(snapshot.pending_requests, 0);
        assert_eq!(snapshot.pending_updates, 1);
        assert_eq!(snapshot.records[0].stage, Stage::Detached);
        assert_eq!(snapshot.records[1].stage, Stage::Queued);
        fixture.open(account)
    });
    let entries = request.recv().unwrap().unwrap();
    assert_local_payload(&entries[0].1, &local("newer exact input"));
    fixture.worker.cycle(|account| fixture.open(account));
    assert_local_payload(
        &fixture.open(fixture.account()).unwrap().load().unwrap()[0].1,
        &local("newer exact input"),
    );
}

#[test]
fn failed_old_write_does_not_replace_newer_coalesced_input() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    fixture.worker.enqueue(
        fixture.account().into(),
        [(scope.key.clone(), local("old"))].into(),
    );
    assert!(fixture.worker.cycle(|account| {
        fixture
            .worker
            .enqueue(account.into(), [(scope.key.clone(), local("newer"))].into());
        Err(sync::invalid("actual opener failure"))
    }));
    let snapshot = fixture.snapshot();
    assert!(snapshot.failed);
    assert_eq!(snapshot.pending_updates, 1);
    assert_eq!(snapshot.records[0].stage, Stage::FinishedClassified);
    assert_eq!(snapshot.records[1].stage, Stage::Queued);
    assert_local_payload(
        &fixture.worker.pending.lock().unwrap().updates[&(fixture.account().into(), scope.key)],
        &local("newer"),
    );
    fixture.worker.cycle(|account| fixture.open(account));
    assert_local_payload(
        &fixture.open(fixture.account()).unwrap().load().unwrap()[0].1,
        &local("newer"),
    );
}

#[test]
fn retry_is_observed_without_reclassifying_failure_as_durable_success() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    fixture.worker.enqueue(
        fixture.account().into(),
        [(scope.key.clone(), local("retry retained"))].into(),
    );
    assert!(
        fixture
            .worker
            .cycle(|_| Err(sync::invalid("synthetic unavailable store")))
    );
    let snapshot = fixture.snapshot();
    assert!(snapshot.failed);
    assert_eq!(snapshot.records.len(), 2);
    assert_eq!(snapshot.records[0].stage, Stage::FinishedClassified);
    assert_eq!(snapshot.records[1].origin, Origin::CanonicalRetry);
    assert_eq!(snapshot.records[1].stage, Stage::Queued);
    fixture.worker.cycle(|account| fixture.open(account));
    assert!(!fixture.snapshot().failed);
    assert!(
        fixture
            .snapshot()
            .records
            .iter()
            .all(|row| row.stage == Stage::FinishedClassified)
    );
}

#[test]
fn blocked_durable_input_is_separate_from_uncommitted_or_failed_input() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    crate::drafts::store::sync::tests::enroll(
        &mut fixture.open(fixture.account()).unwrap(),
        &scope,
    );
    fixture.worker.enqueue(
        fixture.account().into(),
        [(scope.key.clone(), local("blocked exact"))].into(),
    );
    assert!(!fixture.worker.cycle(|account| fixture.open(account)));
    let snapshot = fixture.snapshot();
    assert!(snapshot.blocked);
    assert!(!snapshot.failed);
    assert_eq!(snapshot.pending_updates, 0);
    let receive = fixture.load();
    fixture.worker.cycle(|account| fixture.open(account));
    receive.recv().unwrap().unwrap();
    assert!(
        fixture.snapshot().blocked,
        "an unrelated successful read cannot erase blocked evidence"
    );
    let loaded = fixture
        .open(fixture.account())
        .unwrap()
        .load_synced(fixture.account(), LoadMode::Current)
        .unwrap();
    assert!(!loaded.blocked_local.is_empty());
}

#[test]
fn dropped_reply_does_not_remove_evidence_or_reexecute() {
    let fixture = Fixture::new();
    drop(fixture.load());
    let calls = std::cell::Cell::new(0);
    fixture.worker.cycle(|account| {
        calls.set(calls.get() + 1);
        fixture.open(account)
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(
        fixture.snapshot().records[0].stage,
        Stage::FinishedClassified
    );
    fixture
        .worker
        .cycle(|_| panic!("no replay after dropped waiter"));
}

#[test]
fn ordinary_coalescing_records_supersession_and_writes_only_the_actual_latest_input() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    for text in ["first", "second", "actual latest"] {
        fixture.worker.enqueue(
            fixture.account().into(),
            [(scope.key.clone(), local(text))].into(),
        );
    }
    let before = fixture.snapshot();
    assert_eq!(before.pending_updates, 1);
    assert_eq!(
        before
            .records
            .iter()
            .map(|row| row.stage)
            .collect::<Vec<_>>(),
        vec![Stage::Superseded, Stage::Superseded, Stage::Queued]
    );
    fixture.worker.cycle(|account| fixture.open(account));
    assert_local_payload(
        &fixture.open(fixture.account()).unwrap().load().unwrap()[0].1,
        &local("actual latest"),
    );
    assert_eq!(
        fixture.snapshot().records[2].stage,
        Stage::FinishedClassified
    );
}

#[test]
fn foreign_request_latches_incomplete_without_rerouting_or_suppressing_the_reply() {
    let fixture = Fixture::new();
    let foreign = uuid::Uuid::new_v4().to_string();
    let (reply, receive) = mpsc::sync_channel(1);
    fixture.worker.submit(Request::Load(foreign.clone(), reply));
    assert_eq!(
        fixture.snapshot().incomplete,
        Some(Incomplete::ForeignAccount)
    );
    let seen = std::cell::Cell::new(0);
    fixture.worker.cycle(|account| {
        assert_eq!(account, foreign);
        seen.set(seen.get() + 1);
        Err(sync::invalid("actual ownership refusal"))
    });
    assert!(receive.recv().unwrap().is_err());
    assert_eq!(seen.get(), 1);
    assert_eq!(
        fixture.snapshot().incomplete,
        Some(Incomplete::ForeignAccount)
    );
}

#[test]
fn panic_marks_entire_remaining_detached_batch_unknown() {
    let fixture = Fixture::new();
    let first = fixture.load();
    let second = fixture.load();
    let actual = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fixture.worker.cycle(|_| panic!("held opener panic"))
    }));
    assert!(actual.is_err());
    assert!(first.recv().is_err());
    assert!(second.recv().is_err());
    let snapshot = fixture.snapshot();
    assert_eq!(snapshot.incomplete, Some(Incomplete::Abandoned));
    assert!(
        snapshot
            .records
            .iter()
            .all(|row| row.stage == Stage::AbandonedUnknown)
    );
}

#[test]
fn capacity_overflow_keeps_actual_fifo_and_permanently_incomplete_observations() {
    let fixture = Fixture::new();
    let mut replies = vec![];
    for _ in 0..MAX_RECORDS + 3 {
        replies.push(fixture.load());
    }
    let snapshot = fixture.snapshot();
    assert_eq!(snapshot.records.len(), MAX_RECORDS);
    assert_eq!(snapshot.incomplete, Some(Incomplete::Capacity));
    let calls = std::cell::Cell::new(0);
    fixture.worker.cycle(|account| {
        calls.set(calls.get() + 1);
        fixture.open(account)
    });
    for reply in replies {
        reply.recv().unwrap().unwrap();
    }
    assert_eq!(calls.get(), MAX_RECORDS + 3);
    assert_eq!(fixture.snapshot().incomplete, Some(Incomplete::Capacity));
}

#[test]
fn metadata_and_counter_limits_do_not_drop_actual_canonical_content() {
    for counter in [false, true] {
        let fixture = Fixture::new();
        let mut scope = fixture.scope();
        if counter {
            fixture
                .worker
                .pending
                .lock()
                .unwrap()
                .accounting
                .as_mut()
                .unwrap()
                .sequence = u64::MAX;
        } else {
            scope.key.channel = "C".repeat(MAX_COMPONENT + 1);
        }
        let exact = local(&"large input retained ".repeat(1000));
        fixture.worker.enqueue(
            fixture.account().into(),
            [(scope.key.clone(), exact.clone())].into(),
        );
        assert_local_payload(
            &fixture.worker.pending.lock().unwrap().updates
                [&(fixture.account().into(), scope.key.clone())],
            &exact,
        );
        assert_eq!(
            fixture.snapshot().incomplete,
            Some(if counter {
                Incomplete::Counter
            } else {
                Incomplete::Metadata
            })
        );
        // Deliberate error retains original input; accounting does not decide SQL
        // acceptance or require changing product key validation for this test.
        assert!(
            fixture
                .worker
                .cycle(|_| Err(sync::invalid("retained input")))
        );
        assert_local_payload(
            &fixture.worker.pending.lock().unwrap().updates[&(fixture.account().into(), scope.key)],
            &exact,
        );
    }
}

#[test]
fn actual_owned_cold_and_current_requests_use_the_same_diagnostic_book() {
    let queue = super::super::owned_queue::Queue::fresh().unwrap();
    let reply = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*reply else {
        panic!("inspect")
    };
    queue
        .submit_upgrade(upgrade::Action::Commit(review.clone()))
        .unwrap()
        .wait()
        .unwrap();
    let reply = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*reply else {
        panic!("inspect")
    };
    queue.open(review.clone()).unwrap();
    let identity = Arc::new(());
    queue.cold(&identity).unwrap().wait().unwrap();
    let before = queue.diagnostics().unwrap().sequence;
    queue.cold(&identity).unwrap().wait().unwrap();
    assert_eq!(queue.diagnostics().unwrap().sequence, before);
    let (reply, receive) = mpsc::channel();
    queue.submit_protected(protected::commands::Command {
        account: queue.account().into(),
        action: protected::commands::Action::Load {
            mode: protected::LoadMode::Current,
            reply,
        },
    });
    receive.recv().unwrap().unwrap();
    let snapshot = queue.diagnostics().unwrap();
    assert_eq!(snapshot.worker, queue.worker_id());
    assert_eq!(snapshot.account, queue.account());
    assert_eq!(snapshot.sequence, before + 1);
    assert_eq!(snapshot.incomplete, None);
}

#[test]
fn oversized_failed_retry_keeps_original_key_and_empty_diagnostics() {
    let fixture = Fixture::new();
    let mut scope = fixture.scope();
    scope.key.channel = "C".repeat(16 * 1024);
    let original_key_allocation = scope.key.channel.as_ptr();
    let value = local("exact input survives oversized diagnostic key");
    fixture.worker.enqueue(
        fixture.account().into(),
        [(scope.key, value.clone())].into(),
    );
    for _ in 0..2 {
        assert!(
            fixture
                .worker
                .cycle(|_| Err(sync::invalid("retained retry")))
        );
        let pending = fixture.worker.pending.lock().unwrap();
        let ((_, actual_key), actual_value) = pending.updates.first_key_value().unwrap();
        assert_eq!(actual_key.channel.as_ptr(), original_key_allocation);
        assert_local_payload(&actual_value, &value);
        let book = pending.accounting.as_ref().unwrap();
        assert!(book.records.is_empty());
        assert!(book.updates.is_empty());
        assert_eq!(book.incomplete, Some(Incomplete::Metadata));
    }
}

#[test]
fn ordinary_test_worker_without_book_preserves_existing_fifo_and_retry() {
    let database = crate::drafts::store::sync::tests::Database::new();
    let worker = Worker::default();
    let scope = crate::drafts::store::sync::tests::scope();
    worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("ordinary"))].into(),
    );
    assert!(worker.pending.lock().unwrap().accounting.is_none());
    assert!(worker.cycle(|_| Err(sync::invalid("ordinary failure"))));
    assert_local_payload(
        &worker.pending.lock().unwrap().updates[&(scope.account_id.clone(), scope.key.clone())],
        &local("ordinary"),
    );
    assert!(!worker.cycle(|_| Ok(database.open())));
    assert_local_payload(&database.open().load().unwrap()[0].1, &local("ordinary"));
}
