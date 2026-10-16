use super::*;
use crate::file_download::{SaveMode, StagedFile};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn path() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("sp-original-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&p).unwrap();
    p.join("original.bin")
}
async fn server(reply: Vec<u8>) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut raw = Vec::new();
        loop {
            let mut b = [0u8; 1024];
            let n = socket.read(&mut b).await.unwrap();
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&b[..n]);
            if raw.windows(4).any(|s| s == b"\r\n\r\n") {
                break;
            }
        }
        socket.write_all(&reply).await.unwrap();
        String::from_utf8(raw).unwrap()
    });
    (url, task)
}
async fn clean(path: &std::path::Path) {
    let root = path.parent().unwrap();
    for _ in 0..100 {
        if std::fs::read_dir(root)
            .unwrap()
            .all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn actual_original_stream_crosses_preview_limit_without_whole_body_buffering() {
    let _serial = SERIAL.lock().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let bytes = 65 * 1024 * 1024u64 + 7;
    let task = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        s.read(&mut request).await.unwrap();
        s.write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Length: {bytes}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
        let chunk = [0x5au8; 64 * 1024];
        let mut left = bytes;
        while left > 0 {
            let n = left.min(chunk.len() as u64) as usize;
            s.write_all(&chunk[..n]).await.unwrap();
            left -= n as u64;
        }
    });
    let path = path();
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    let transport = Transport::new("synthetic").unwrap();
    assert_eq!(
        transport
            .download_original(
                &OriginalSource::parse(&url).unwrap(),
                "test",
                stage.writer(),
                || true
            )
            .await
            .unwrap(),
        bytes
    );
    assert!(!path.exists());
    stage
        .commit(|| true, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    task.await.unwrap();
    assert_eq!(tokio::fs::metadata(&path).await.unwrap().len(), bytes);
    let mut f = tokio::fs::File::open(&path).await.unwrap();
    f.seek(std::io::SeekFrom::End(-7)).await.unwrap();
    let mut tail = [0; 7];
    f.read_exact(&mut tail).await.unwrap();
    assert_eq!(tail, [0x5a; 7]);
    drop(f);
    clean(&path).await;
}
#[tokio::test]
async fn rate_limits_and_http_errors_do_not_write_a_body_or_retry() {
    let _serial = SERIAL.lock().await;
    for (status, header, expected) in [
        (429, "", None),
        (429, "Retry-After: 17\r\n", Some(17)),
        (503, "Retry-After: 19\r\n", Some(19)),
        (200, "Retry-After: 23\r\n", Some(23)),
    ] {
        let (url,task)=server(format!("HTTP/1.1 {status} Test\r\n{header}Content-Length: 3\r\nConnection: close\r\n\r\nBAD").into_bytes()).await;
        let path = path();
        let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
            .await
            .unwrap();
        let error = Transport::new("synthetic")
            .unwrap()
            .download_original(
                &OriginalSource::parse(&url).unwrap(),
                "test",
                stage.writer(),
                || true,
            )
            .await
            .unwrap_err();
        match error {
            DownloadError::Network(Error::RateLimited { retry_after_secs })
            | DownloadError::Network(Error::HttpStatus {
                retry_after_secs, ..
            }) => assert_eq!(retry_after_secs, expected),
            _ => panic!("wrong failure"),
        };
        assert_eq!(stage.writer().metadata().await.unwrap().len(), 0);
        drop(stage);
        task.await.unwrap();
        assert!(!path.exists());
        clean(&path).await;
    }
}
#[tokio::test]
async fn cross_origin_redirect_drops_cookie_and_never_reattaches_it() {
    let _serial = SERIAL.lock().await;
    let (final_url, final_task) =
        server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK".to_vec())
            .await;
    let (url,first)=server(format!("HTTP/1.1 302 Found\r\nLocation: {final_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()).await;
    let mut source = OriginalSource::parse(&url).unwrap();
    source.authenticated = true; // Synthetic cookie only, loopback authority only in this test.
    let path = path();
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    Transport::new("synthetic-cookie")
        .unwrap()
        .download_original(&source, "test", stage.writer(), || true)
        .await
        .unwrap();
    assert!(first.await.unwrap().to_lowercase().contains("cookie:"));
    assert!(!final_task.await.unwrap().to_lowercase().contains("cookie:"));
    drop(stage);
    clean(&path).await;
}
#[tokio::test]
async fn declared_oversize_truncated_and_redirect_failures_preserve_destination_absence() {
    let _serial = SERIAL.lock().await;
    for reply in [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_ORIGINAL_BYTES+1),"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".into(),"HTTP/1.1 302 Found\r\nLocation: file:///private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbad".into()] {
        let (url,task)=server(reply.into_bytes()).await;let path=path();let mut stage=StagedFile::create(path.clone(),SaveMode::NewFile).await.unwrap();
        assert!(Transport::new("synthetic").unwrap().download_original(&OriginalSource::parse(&url).unwrap(),"test",stage.writer(),||true).await.is_err());drop(stage);task.await.unwrap();assert!(!path.exists());clean(&path).await;
    }
}
#[tokio::test]
async fn retirement_interrupts_held_body_and_offline_never_dispatches() {
    let _serial = SERIAL.lock().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let source = OriginalSource::parse(&url).unwrap();
    let path = path();
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .download_original(&source, "test", stage.writer(), || true)
            .await,
        Err(DownloadError::Network(Error::Offline(_)))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
    transport.set_health(Health::Online);
    let current = Arc::new(AtomicBool::new(true));
    let retired = current.clone();
    let task = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        s.read(&mut request).await.unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\npartial")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        retired.store(false, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    assert!(matches!(
        transport
            .download_original(&source, "test", stage.writer(), || current
                .load(Ordering::Acquire))
            .await,
        Err(DownloadError::Canceled)
    ));
    drop(stage);
    task.await.unwrap();
    assert!(!path.exists());
    clean(&path).await;
}
#[test]
fn retry_after_dates_and_malformed_values_are_bounded_without_replay() {
    let mut h = HeaderMap::new();
    h.insert(
        "retry-after",
        "Wed, 01 Jan 2098 00:00:00 GMT".parse().unwrap(),
    );
    assert!(original_retry_after(&h).unwrap() > 1000);
    h.insert("retry-after", "invalid".parse().unwrap());
    assert_eq!(original_retry_after(&h), Some(30));
    h.insert("retry-after", "0".parse().unwrap());
    assert_eq!(original_retry_after(&h), Some(1));
}

#[tokio::test]
async fn chunked_body_enforces_the_same_streaming_count_bound() {
    let _serial = SERIAL.lock().await;
    let reply = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n50\r\n{}\r\n0\r\n\r\n",
        "x".repeat(80)
    );
    let (url, task) = server(reply.into_bytes()).await;
    let path = path();
    let mut stage = StagedFile::create(path.clone(), SaveMode::NewFile)
        .await
        .unwrap();
    let source = OriginalSource::parse(&url).unwrap();
    assert!(matches!(
        Transport::new("synthetic")
            .unwrap()
            .original_inner(&source, "test", stage.writer(), &|| true, 64)
            .await,
        Err(DownloadError::TooLarge)
    ));
    drop(stage);
    task.await.unwrap();
    assert!(!path.exists());
    clean(&path).await;
}
#[tokio::test(start_paused = true)]
async fn held_original_wait_has_a_finite_idle_deadline() {
    let transport = Transport::new("synthetic").unwrap();
    let started = tokio::time::Instant::now();
    let result = transport
        .original_wait(
            tokio::time::timeout(IDLE, std::future::pending::<()>()),
            &|| true,
        )
        .await
        .unwrap();
    assert!(result.is_err());
    assert!(started.elapsed() >= IDLE);
}
