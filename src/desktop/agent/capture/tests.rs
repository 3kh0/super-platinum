use super::*;

#[cfg(target_os = "windows")]
#[tokio::test]
async fn windows_capture_arguments_are_data_not_powershell_source() {
    let path = std::env::temp_dir().join("capture space ' $([int]42) ` &.png");
    let mut command = windows_command(
        "[Console]::Write($env:SUPER_PLATINUM_CAPTURE_HWND + '|' + $env:SUPER_PLATINUM_CAPTURE_PATH)",
        "123456",
        &path,
    );
    let output = run(&mut command, Instant::now() + Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("123456|{}", path.display())
    );
}

fn unique_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[cfg(unix)]
#[tokio::test]
async fn slow_capture_yields_to_other_local_work_and_returns_output() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 0.15; printf captured"]);
    let capture = run(&mut command, Instant::now() + Duration::from_secs(2));
    tokio::pin!(capture);
    tokio::select! {
        result = &mut capture => panic!("capture blocked local timer: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(10)) => {},
    }
    assert_eq!(capture.await.unwrap(), b"captured");
}

#[cfg(unix)]
#[tokio::test]
async fn deadline_kills_and_reaps_child_without_retry() {
    let path = std::env::temp_dir().join(format!("sp-capture-timeout-{}.pid", unique_id()));
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "echo $$ > \"$1\"; exec sleep 30", "capture-test"])
        .arg(&path);
    let result = run(&mut command, Instant::now() + Duration::from_millis(150)).await;
    assert_eq!(result.unwrap_err(), "screenshot process timed out");
    let pid = std::fs::read_to_string(&path).unwrap();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap()
            .success()
    );
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_capture_releases_child_and_reaper_continues() {
    let path = std::env::temp_dir().join(format!("sp-capture-cancel-{}.pid", unique_id()));
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "echo $$ > \"$1\"; exec sleep 30", "capture-test"])
        .arg(&path);
    let mut capture = Box::pin(run(&mut command, Instant::now() + Duration::from_secs(5)));
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            tokio::select! {
                result = &mut capture => panic!("unexpected completion: {result:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if std::fs::read_to_string(&path).is_ok_and(|s| !s.trim().is_empty()) {break;}
                }
            }
        }
    })
    .await
    .unwrap();
    let pid = std::fs::read_to_string(&path).unwrap();
    drop(capture);
    tokio::time::timeout(Duration::from_secs(2), async {
        while Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap()
            .success()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn png_header_is_checked_and_dimensions_preserved() {
    let path = std::env::temp_dir().join(format!("sp-capture-header-{}.png", unique_id()));
    let mut header = Vec::from(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".as_slice());
    header.extend(668_u32.to_be_bytes());
    header.extend(400_u32.to_be_bytes());
    std::fs::write(&path, &header).unwrap();
    assert_eq!(dimensions(&path).unwrap(), (668, 400));
    header[0] = 0;
    std::fs::write(&path, &header).unwrap();
    assert_eq!(dimensions(&path).unwrap_err(), "screenshot is not a PNG");
    std::fs::remove_file(path).unwrap();
}
