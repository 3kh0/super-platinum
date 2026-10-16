//! Actual staging/commit boundary, not a second mocked download implementation.
use super::*;
use crate::file_preview::refresh::{self, tests as setup};
fn directory() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "sp-refresh-save-{}",
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
#[tokio::test]
async fn refreshed_viewer_cancels_captured_cached_save_before_publication() {
    let dir = directory();
    let destination = dir.join("sentinel.txt");
    std::fs::write(&destination, b"sentinel").unwrap();
    let mut shell = setup::shell();
    let old = shell.viewer.as_ref().unwrap().id.clone();
    let lease = refresh::lease(&shell, &old).unwrap();
    let canceled = lease.cancellation();
    let (owner, bytes) = owner::Owner::capture(&shell, old).unwrap();
    setup::publish(&mut shell);
    let result = transfer(
        &owner,
        bytes,
        destination.clone(),
        || !canceled.load(Ordering::Acquire),
        canceled.clone(),
    )
    .await;
    assert!(matches!(result, Err(DownloadError::Canceled)));
    assert_eq!(std::fs::read(&destination).unwrap(), b"sentinel");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn successful_refresh_trips_same_latch_checked_by_staged_filesystem_commit() {
    for cached in [true, false] {
        let dir = directory();
        let destination = dir.join("original.txt");
        if cached {
            std::fs::write(&destination, b"sentinel").unwrap();
        }
        let mut shell = setup::shell();
        let old = shell.viewer.as_ref().unwrap().id.clone();
        let canceled = refresh::lease(&shell, &old).unwrap().cancellation();
        let mut staged = StagedFile::create(
            destination.clone(),
            if cached {
                SaveMode::ConfirmedCachedOverwrite
            } else {
                SaveMode::NewFile
            },
        )
        .await
        .unwrap();
        if cached {
            staged.cached(b"old cached", || true).await.unwrap();
        } else {
            use tokio::io::AsyncWriteExt;
            staged.writer().write_all(b"old streamed").await.unwrap();
        }
        setup::publish(&mut shell);
        // Even an already-admitted renderer closure cannot bypass the atomic worker latch.
        assert!(matches!(
            staged.commit(|| true, canceled).await,
            Err(DownloadError::Canceled)
        ));
        if cached {
            assert_eq!(std::fs::read(&destination).unwrap(), b"sentinel");
        } else {
            assert!(!destination.exists());
        }
        // StagedFile cleanup is asynchronous; wait only for this owned directory to quiesce.
        for _ in 0..100 {
            if std::fs::read_dir(&dir).unwrap().count() == usize::from(cached) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            usize::from(cached)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
