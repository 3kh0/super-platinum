use super::*;
use crate::drafts::store::protected::upgrade;

fn value(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

fn setup() -> (Registry, String, PathBuf) {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("sp-configured-owner-{}", uuid::Uuid::new_v4()));
    let account = uuid::Uuid::new_v4().to_string();
    (
        Registry {
            root: Some(root.clone()),
            ..Registry::default()
        },
        account,
        root,
    )
}
fn inspect(queue: &owned_queue::Queue) -> Arc<upgrade::Review> {
    let reply = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*reply else {
        panic!("inspection")
    };
    review.clone()
}

#[test]
fn cold_acquisition_retains_same_worker_and_does_not_create_database() {
    let (registry, account, root) = setup();
    let first = registry.acquire(&account).unwrap();
    let id = first.worker_id().to_owned();
    assert!(!root.join(format!("drafts-{account}.sqlite")).exists());
    assert_eq!(inspect(&first).class(), upgrade::Class::Missing);
    drop(first);
    let again = registry.acquire(&account).unwrap();
    assert_eq!(again.worker_id(), id);
    assert!(!registry.ordinary(&account));
    assert!(registry.open_ordinary(&account).is_err());
    assert!(!root.join(format!("drafts-{account}.sqlite")).exists());
}

#[test]
fn ordinary_submission_is_irrevocable_before_any_open_or_queue_drain() {
    let (registry, account, root) = setup();
    assert!(registry.ordinary(&account));
    assert!(!root.exists());
    assert!(matches!(registry.acquire(&account), Err(Error::Stale)));
    // No queue contents are used as a certificate, so even zero pending work
    // after that first submission cannot turn this into a cold account.
    assert!(registry.ordinary(&account));
    assert!(matches!(registry.acquire(&account), Err(Error::Stale)));
}

#[test]
fn competing_first_submission_and_cold_acquisition_have_one_winner() {
    let (registry, account, _) = setup();
    let registry = Arc::new(registry);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let other = registry.clone();
    let key = account.clone();
    let gate = barrier.clone();
    let ordinary = std::thread::spawn(move || {
        gate.wait();
        other.ordinary(&key)
    });
    barrier.wait();
    let acquired = registry.acquire(&account).is_ok();
    assert_ne!(ordinary.join().unwrap(), acquired);
}

#[test]
fn independent_coordinator_cannot_share_marker_and_failure_is_retained() {
    let (registry, account, root) = setup();
    let owner = registry.acquire(&account).unwrap();
    let second = Registry {
        root: Some(root),
        ..Registry::default()
    };
    assert!(second.acquire(&account).is_err());
    drop(owner);
    drop(registry);
    // An actual fresh caller must not turn an earlier failed job into a retry.
    assert!(second.acquire(&account).is_err());
}

#[test]
fn ordinary_marker_excludes_protected_owner_and_preserves_existing_drafts() {
    let (registry, account, root) = setup();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("drafts-{account}.sqlite"));
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: Some("1.2".into()),
    };
    let local = LocalDraft {
        composer: crate::ComposerState::with_text("preserved 日本語 🚀"),
        ..LocalDraft::default()
    };
    DraftStore::open(&path)
        .unwrap()
        .save([(key.clone(), local.clone())])
        .unwrap();
    assert_eq!(
        value(&registry.open_ordinary(&account).unwrap().load().unwrap()),
        value(&vec![(key.clone(), local.clone())])
    );
    let other = Registry {
        root: Some(root.clone()),
        ..Registry::default()
    };
    assert!(other.acquire(&account).is_err());
    assert_eq!(
        value(&DraftStore::open(&path).unwrap().load().unwrap()),
        value(&vec![(key, local)])
    );
}

#[test]
fn configured_actual_upgrade_preserves_unicode_thread_local_and_reuses_handoff() {
    let (registry, account, root) = setup();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("drafts-{account}.sqlite"));
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: Some("1.2".into()),
    };
    let local = LocalDraft {
        composer: crate::ComposerState::with_text("unsent 日本語 🚀"),
        ..LocalDraft::default()
    };
    DraftStore::open(&path)
        .unwrap()
        .save([(key.clone(), local.clone())])
        .unwrap();
    let queue = registry.acquire(&account).unwrap();
    let review = inspect(&queue);
    assert_eq!(review.class(), upgrade::Class::OrdinaryLocal);
    queue
        .submit_upgrade(upgrade::Action::Commit(review))
        .unwrap()
        .wait()
        .unwrap();
    let review = inspect(&queue);
    assert_eq!(review.class(), upgrade::Class::Protected7);
    queue.open(review.clone()).unwrap();
    queue.open(review).unwrap();
    assert!(Arc::ptr_eq(&queue, &registry.acquire(&account).unwrap()));
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let (stored_key, stored_local): (String, String) = conn
        .query_row(
            "SELECT destination,content FROM composer_drafts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<DraftKey>(&stored_key).unwrap(), key);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stored_local).unwrap(),
        value(&local)
    );
}

#[test]
fn malformed_marker_refuses_without_repair_and_invalid_identity_creates_nothing() {
    let (registry, account, root) = setup();
    assert!(registry.acquire("../not-an-account").is_err());
    assert!(!root.exists());
    std::fs::create_dir_all(&root).unwrap();
    let marker = root.join(format!("drafts-{account}.owner"));
    std::fs::write(&marker, b"other owner").unwrap();
    assert!(registry.acquire(&account).is_err());
    assert_eq!(std::fs::read(marker).unwrap(), b"other owner");
    assert!(!root.join(format!("drafts-{account}.sqlite")).exists());
}

#[test]
fn later_ordinary_local_input_is_retained_without_a_retry_loop() {
    // cfg(test) config::data_dir is isolated; no environment or user path change.
    let account = uuid::Uuid::new_v4().to_string();
    let _queue = registry().acquire(&account).unwrap();
    let worker = Worker {
        configured_default: true,
        ..Worker::default()
    };
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let draft = LocalDraft {
        composer: crate::ComposerState::with_text("retain refused input"),
        ..LocalDraft::default()
    };
    worker.enqueue(
        account.clone(),
        BTreeMap::from([(key.clone(), draft.clone())]),
    );
    let pending = worker.pending.lock().unwrap();
    assert!(pending.updates.is_empty() && pending.requests.is_empty());
    assert!(pending.failed.contains(&account));
    assert_eq!(
        value(&pending.refused_configured_updates.get(&(account, key))),
        value(&Some(&draft))
    );
}
