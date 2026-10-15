//! Exercise actual Worker::cycle, not synthetic queue snapshots.
use super::*;
use crate::drafts::store::sync::tests::local;
macro_rules! assert_saved_eq {
    ($a:expr, $b:expr) => {
        assert_eq!(
            serde_json::to_value(&$a).unwrap(),
            serde_json::to_value(&$b).unwrap()
        )
    };
}
fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn open(owner: &Owned, account: &str) -> Result<DraftStore, AppError> {
    assert_eq!(account, owner.account());
    DraftStore::open(owner.path())
}
fn inspect(worker: &Arc<Worker>, owner: &Arc<Owned>) -> Arc<upgrade::Review> {
    let (c, rx) = Command::new(owner.clone(), Action::Inspect);
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(owner, a));
    let UpgradeReply::Inspected(v) = rx.recv().unwrap().unwrap() else {
        panic!("inspection")
    };
    v
}
fn ordinary(owner: &Owned) {
    DraftStore::open(owner.path())
        .unwrap()
        .save([(key(), local("before"))])
        .unwrap();
}
#[test]
fn m07_older_coalesced_write_is_committed_before_inspection() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let worker = Arc::new(Worker::default());
    worker.enqueue(
        owner.account().into(),
        BTreeMap::from([(key(), local("older retained"))]),
    );
    let review = inspect(&worker, &owner);
    assert_eq!(review.class(), upgrade::Class::OrdinaryLocal);
    assert_saved_eq!(
        DraftStore::open(owner.path()).unwrap().load().unwrap()[0].1,
        local("older retained")
    );
    let (c, rx) = Command::new(owner.clone(), Action::Commit(review));
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(&owner, a));
    assert!(matches!(
        rx.recv().unwrap().unwrap(),
        UpgradeReply::Committed(_)
    ));
}
#[test]
fn m07_detached_later_request_blocks_upgrade_even_when_pending_requests_empty() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let worker = Arc::new(Worker::default());
    let review = inspect(&worker, &owner);
    let (c, rx) = Command::new(owner.clone(), Action::Commit(review));
    let (tx, loaded) = mpsc::sync_channel(1);
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.submit(Request::Load(owner.account().into(), tx));
    worker.cycle(|a| open(&owner, a));
    assert!(rx.recv().unwrap().is_err());
    assert_saved_eq!(loaded.recv().unwrap().unwrap()[0].1, local("before"));
    assert!(!DraftStore::open(owner.path()).unwrap().0.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='composer_draft_continuity_format')",[],|r|r.get::<_,bool>(0)).unwrap());
}
#[test]
fn m07_failed_older_write_and_prior_blocked_flag_are_never_cleared_to_admit() {
    for blocked in [false, true] {
        let owner = Owned::fresh().unwrap();
        ordinary(&owner);
        let worker = Arc::new(Worker::default());
        if blocked {
            worker
                .pending
                .lock()
                .unwrap()
                .blocked
                .insert(owner.account().into());
        } else {
            worker.enqueue(
                owner.account().into(),
                BTreeMap::from([(key(), local("failed input"))]),
            );
        }
        let (c, rx) = Command::new(owner.clone(), Action::Inspect);
        worker.submit(Request::StoreUpgrade(Box::new(c)));
        worker.cycle(|_| Err(sync::invalid("injected ordinary open failure")));
        assert!(rx.recv().unwrap().is_err());
        let pending = worker.pending.lock().unwrap();
        if blocked {
            assert!(pending.blocked.contains(owner.account()));
        } else {
            assert!(pending.failed.contains(owner.account()));
            assert_saved_eq!(
                pending.updates.get(&(owner.account().into(), key())),
                Some(&local("failed input"))
            );
        }
    }
}
#[test]
fn m07_new_input_during_open_and_before_commit_invalidates_without_losing_input() {
    for after_open in [true, false] {
        let owner = Owned::fresh().unwrap();
        ordinary(&owner);
        let worker = Arc::new(Worker::default());
        let review = inspect(&worker, &owner);
        let (mut c, rx) = Command::new(owner.clone(), Action::Commit(review));
        let queued = worker.clone();
        let account = owner.account().to_owned();
        let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
            queued.enqueue(
                account,
                BTreeMap::from([(key(), local("queued 日本語 🚀"))]),
            )
        });
        if after_open {
            c.after_open = Some(hook);
        } else {
            c.before_seal = Some(hook);
        }
        worker.submit(Request::StoreUpgrade(Box::new(c)));
        worker.cycle(|a| open(&owner, a));
        assert!(rx.recv().unwrap().is_err());
        assert_saved_eq!(
            worker
                .pending
                .lock()
                .unwrap()
                .updates
                .get(&(owner.account().into(), key())),
            Some(&local("queued 日本語 🚀"))
        );
        assert_saved_eq!(
            DraftStore::open(owner.path()).unwrap().load().unwrap()[0].1,
            local("before")
        );
        worker.cycle(|a| open(&owner, a));
        assert_saved_eq!(
            DraftStore::open(owner.path()).unwrap().load().unwrap()[0].1,
            local("queued 日本語 🚀")
        );
    }
}
#[test]
fn m07_same_bytes_queue_aba_invalidates_review_despite_equal_sqlite_rows() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let worker = Arc::new(Worker::default());
    let review = inspect(&worker, &owner);
    worker.enqueue(
        owner.account().into(),
        BTreeMap::from([(key(), local("before"))]),
    );
    worker.cycle(|a| open(&owner, a));
    let (c, rx) = Command::new(owner.clone(), Action::Commit(review));
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(&owner, a));
    assert!(rx.recv().unwrap().is_err());
}
#[test]
fn m07_input_after_seal_is_ordered_after_upgrade_and_remains_queued() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let worker = Arc::new(Worker::default());
    let review = inspect(&worker, &owner);
    let (mut c, rx) = Command::new(owner.clone(), Action::Commit(review));
    let queued = worker.clone();
    let account = owner.account().to_owned();
    c.after_seal = Some(Box::new(move || {
        queued.enqueue(account, BTreeMap::from([(key(), local("later input"))]))
    }));
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(&owner, a));
    assert!(rx.recv().unwrap().is_ok());
    assert_saved_eq!(
        worker
            .pending
            .lock()
            .unwrap()
            .updates
            .get(&(owner.account().into(), key())),
        Some(&local("later input"))
    );
    worker.cycle(|a| open(&owner, a));
    assert_saved_eq!(
        DraftStore::open(owner.path()).unwrap().load().unwrap()[0].1,
        local("later input")
    );
}
#[test]
fn m07_queue_counter_exhaustion_fails_closed_without_reset() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let worker = Arc::new(Worker::default());
    worker.pending.lock().unwrap().upgrade_generation = u64::MAX;
    let (c, rx) = Command::new(owner.clone(), Action::Inspect);
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(&owner, a));
    assert!(rx.recv().unwrap().is_err());
    assert_eq!(worker.pending.lock().unwrap().upgrade_generation, u64::MAX);
}

#[test]
fn m07_actual_rejected_legacy_write_keeps_blocked_flag_and_durable_input() {
    let owner = Owned::fresh().unwrap();
    ordinary(&owner);
    let mut store = DraftStore::open(owner.path()).unwrap();
    let scope = Scope {
        account_id: owner.account().into(),
        key: key(),
    };
    let state = crate::drafts::store::sync::tests::state(&scope, "before");
    store
        .commit_synced(&scope, None, &local("before"), &state, None)
        .unwrap();
    drop(store);
    let worker = Arc::new(Worker::default());
    worker.enqueue(
        owner.account().into(),
        BTreeMap::from([(key(), local("rejected retained input"))]),
    );
    let (c, rx) = Command::new(owner.clone(), Action::Inspect);
    worker.submit(Request::StoreUpgrade(Box::new(c)));
    worker.cycle(|a| open(&owner, a));
    assert!(rx.recv().unwrap().is_err());
    assert!(
        worker
            .pending
            .lock()
            .unwrap()
            .blocked
            .contains(owner.account())
    );
    let store = DraftStore::open(owner.path()).unwrap();
    let raw: String = store
        .0
        .query_row("SELECT content FROM composer_draft_blocked", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_saved_eq!(
        serde_json::from_str::<LocalDraft>(&raw).unwrap(),
        local("rejected retained input")
    );
    drop(store);
    // Retire the old isolated worker before a fresh cold worker inspects the
    // already-durable blocked row. No live flag is cleared to permit cutover.
    drop(worker);
    let cold = Arc::new(Worker::default());
    assert_eq!(inspect(&cold, &owner).class(), upgrade::Class::Legacy5);
}
