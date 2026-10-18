use super::*;
fn setup() -> (ShellState, Arc<Capability>) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nonce = format!(
        "{:016x}{:016x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    // A freshly copied/checked-out source tree has no gitignored tmp directory.
    // Create only that shared parent; keep the nonce-owned root exclusive.
    let base = std::env::current_dir().unwrap().join("tmp");
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join(format!("cached-viewer-save-{nonce}"));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("downloads")).unwrap();
    std::fs::write(
        root.join("owner.json"),
        serde_json::to_vec(&json!({"version":1,"nonce":nonce,"fixture":ALIAS})).unwrap(),
    )
    .unwrap();
    let directory = paths::Directory::capture(root.clone(), nonce.clone()).unwrap();
    let mut shell = refresh::tests::shell();
    let old = shell.viewer.as_ref().unwrap().id.clone();
    shell
        .media
        .insert(old.clone(), "text/plain", OLD.as_bytes());
    refresh::opened(&mut shell);
    let cancel = refresh::lease(&shell, &old).unwrap().cancellation();
    let (owner, bytes) = owner::Owner::capture(&shell, old.clone()).unwrap();
    let cap = Arc::new(Capability {
        root,
        nonce,
        directory,
        owner,
        old,
        bytes: bytes.unwrap(),
        cancel,
        ledger: Mutex::new(Ledger::default()),
        release: Mutex::new(None),
    });
    (shell, cap)
}
fn cleanup(cap: Arc<Capability>) {
    let root = cap.root.clone();
    assert_eq!(Arc::strong_count(&cap), 1);
    drop(cap);
    // Exact test-owned path, never default cache/session storage.
    assert!(root.parent().unwrap().ends_with("tmp"));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("cached-viewer-save-")
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn selected_path_is_preserved_and_refresh_cancels_held_owner() {
    let (mut shell, cap) = setup();
    let overwrite = cap
        .root
        .join("downloads")
        .join(paths::expected_leaf(2, &cap.nonce).unwrap());
    let retired = cap
        .root
        .join("downloads")
        .join(paths::expected_leaf(3, &cap.nonce).unwrap());
    std::fs::write(&overwrite, b"first sentinel").unwrap();
    std::fs::write(&retired, b"second sentinel").unwrap();
    {
        let mut l = cap.ledger.lock().unwrap();
        l.begin().unwrap();
        l.returned(1, "none").unwrap();
        l.begin().unwrap();
    }
    let saved = Input {
        cap: cap.clone(),
        attempt: 2,
    };
    assert_eq!(
        saved.returned(Some(overwrite.clone())).await.unwrap(),
        Some(overwrite.clone())
    );
    saved.before_current().await.unwrap();
    saved.transfer().await.unwrap();
    let result = super::super::super::transfer(
        &cap.owner,
        Some(cap.bytes.clone()),
        overwrite.clone(),
        || !cap.cancel.load(Ordering::Acquire),
        cap.cancel.clone(),
    )
    .await;
    saved.completed(&result).await;
    assert!(result.is_ok());
    assert_eq!(std::fs::read(&overwrite).unwrap(), OLD.as_bytes());
    drop(saved);
    cap.ledger.lock().unwrap().begin().unwrap();
    let input = Input {
        cap: cap.clone(),
        attempt: 3,
    };
    assert_eq!(
        input.returned(Some(retired.clone())).await.unwrap(),
        Some(retired.clone())
    );
    {
        let waiter = input.before_current();
        tokio::pin!(waiter);
        // Poll the actual checkpoint until its oneshot is registered; no sleep race.
        tokio::select! {r=&mut waiter=>panic!("checkpoint escaped: {r:?}"), _=tokio::task::yield_now()=>{}}
        assert!(cap.release.lock().unwrap().is_some());
        assert!(!cap.cancel.load(Ordering::Acquire));
        refresh::tests::publish(&mut shell);
        assert!(cap.cancel.load(Ordering::Acquire));
        assert!(cap.owner.current(&shell)); // Actual retirement latch, not account replacement.
        {
            let mut l = cap.ledger.lock().unwrap();
            l.metadata().unwrap();
            l.body().unwrap();
            l.release().unwrap();
        }
        cap.release
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .unwrap();
        waiter.await.unwrap();
    }
    input.retired(cap.cancel.load(Ordering::Acquire)).await;
    assert!(cap.ledger.lock().unwrap().at("retired"));
    assert_eq!(std::fs::read(retired).unwrap(), b"second sentinel");
    assert_eq!(
        std::fs::read_dir(cap.root.join("downloads"))
            .unwrap()
            .count(),
        2
    );
    drop(input);
    cleanup(cap);
}
#[test]
fn private_path_and_owner_mutations_refuse_without_writes() {
    let (_, cap) = setup();
    let path = cap
        .root
        .join("downloads")
        .join(paths::expected_leaf(3, &cap.nonce).unwrap());
    assert!(cap.directory.validate(&path, 3).is_err());
    std::fs::write(&path, b"sentinel").unwrap();
    assert_eq!(cap.directory.validate(&path, 3).unwrap(), "existing");
    assert!(cap.directory.validate(&path, 2).is_err());
    assert!(cap.directory.validate(&path, 1).is_err());
    assert!(
        cap.directory
            .validate(&cap.root.join(path.file_name().unwrap()), 3)
            .is_err()
    );
    std::fs::write(cap.root.join("owner.json"), b"{}").unwrap();
    assert!(cap.directory.validate(&path, 3).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"sentinel");
    cleanup(cap);
}
#[tokio::test]
async fn dropped_return_checkpoint_never_admits_a_transfer() {
    let (_, cap) = setup();
    {
        let mut l = cap.ledger.lock().unwrap();
        l.begin().unwrap();
        l.returned(1, "none").unwrap();
        l.begin().unwrap();
        l.returned(2, "existing").unwrap();
        l.transfer(2).unwrap();
        l.saved(2, true).unwrap();
        l.begin().unwrap();
        l.returned(3, "existing").unwrap();
    }
    let input = Input {
        cap: cap.clone(),
        attempt: 3,
    };
    {
        let wait = input.before_current();
        tokio::pin!(wait);
        tokio::select! {r=&mut wait=>panic!("checkpoint escaped: {r:?}"), _=tokio::task::yield_now()=>{}}
    }
    assert!(
        cap.release
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .is_err()
    );
    assert_eq!(cap.ledger.lock().unwrap().transfers, [2]);
    assert_eq!(cap.ledger.lock().unwrap().outcomes, [(2, "saved")]);
    drop(input);
    cleanup(cap);
}
