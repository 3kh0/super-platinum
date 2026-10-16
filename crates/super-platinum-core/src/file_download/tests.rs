use super::*;
fn dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("sp-download-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&p).unwrap();
    p
}
async fn cleaned(root: &Path) {
    for _ in 0..100 {
        if std::fs::read_dir(root)
            .unwrap()
            .all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part"))
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("temporary original was not cleaned");
}
#[tokio::test]
async fn cached_confirmed_overwrite_and_new_file_publication_preserve_exact_bytes() {
    let root = dir();
    let path = root.join("original.bin");
    std::fs::write(&path, b"previous").unwrap();
    assert!(matches!(
        StagedFile::create(path.clone(), SaveMode::NewFile).await,
        Err(DownloadError::DestinationExists)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    let mut stage = StagedFile::create(path.clone(), SaveMode::ConfirmedCachedOverwrite)
        .await
        .unwrap();
    stage
        .cached(b"complete cached original", || true)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    stage
        .commit(|| true, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"complete cached original");
    let fresh = root.join("new.bin");
    let mut stage = StagedFile::create(fresh.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    stage.cached(b"whole", || true).await.unwrap();
    assert!(!fresh.exists());
    stage
        .commit(|| true, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    assert_eq!(std::fs::read(fresh).unwrap(), b"whole");
    cleaned(&root).await;
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn raced_in_destination_and_changed_cached_destination_are_preserved() {
    let root = dir();
    let path = root.join("target.bin");
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    stage.cached(b"network", || true).await.unwrap();
    std::fs::write(&path, b"other application").unwrap();
    assert!(matches!(
        stage
            .commit(|| true, Arc::new(AtomicBool::new(false)))
            .await,
        Err(DownloadError::DestinationChanged)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"other application");
    let mut stage = StagedFile::create(path.clone(), SaveMode::ConfirmedCachedOverwrite)
        .await
        .unwrap();
    stage.cached(b"cached", || true).await.unwrap();
    std::fs::write(&path, b"changed and longer").unwrap();
    assert!(matches!(
        stage
            .commit(|| true, Arc::new(AtomicBool::new(false)))
            .await,
        Err(DownloadError::DestinationChanged)
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"changed and longer");
    cleaned(&root).await;
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn canceled_or_dropped_stage_never_publishes_partial_original() {
    let root = dir();
    let path = root.join("target.bin");
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    stage.cached(b"partial", || true).await.unwrap();
    assert!(matches!(
        stage
            .commit(|| false, Arc::new(AtomicBool::new(false)))
            .await,
        Err(DownloadError::Canceled)
    ));
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    stage.cached(b"partial", || true).await.unwrap();
    drop(stage);
    cleaned(&root).await;
    assert!(!path.exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn credentials_are_only_admitted_to_exact_slack_host_suffix_and_no_url_userinfo() {
    for url in ["https://files.slack.com/a", "https://slack-edge.com/a"] {
        assert!(OriginalSource::parse(url).unwrap().authenticated);
    }
    for url in ["https://slack.com.evil.example/a", "https://cdn.example/a"] {
        assert!(!OriginalSource::parse(url).unwrap().authenticated);
    }
    for url in [
        "http://files.slack.com/a",
        "https://user:password@slack.com/a",
        "https://files.slack.com/a#part",
        "file:///tmp/a",
    ] {
        assert!(OriginalSource::parse(url).is_err());
    }
}
