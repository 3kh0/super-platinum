use super::*;

#[test]
fn upload_uncertainty_depends_on_delivery_stage() {
    for error in [
        Error::Transport("read local attachment".into()),
        Error::Offline("connection reset".into()),
        Error::Api("internal_error".into()),
        Error::Api("fatal_error".into()),
        Error::HttpStatus {
            status: 503,
            retry_after_secs: None,
        },
        Error::HttpStatus {
            status: 408,
            retry_after_secs: None,
        },
    ] {
        assert!(!UploadMessageError::BeforeDelivery(error.clone()).delivery_uncertain());
        assert!(UploadMessageError::Completion(error).delivery_uncertain());
    }
    for error in [
        Error::Api("not_in_channel".into()),
        Error::UploadCanceled,
        Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(30),
        },
    ] {
        assert!(!UploadMessageError::BeforeDelivery(error.clone()).delivery_uncertain());
        assert!(!UploadMessageError::Completion(error).delivery_uncertain());
    }
}

#[test]
fn upload_completion_requires_exact_positive_file_receipt() {
    use serde_json::json;
    let staged = vec![json!({"id":"F1","title":"日本語 🚀"}), json!({"id":"F2"})];
    assert_eq!(
        completed_upload(
            &json!({"ok":true,"files":[{"id":"F2"},{"id":"F1"}]}),
            &staged
        )
        .unwrap()
        .file_ids,
        vec!["F1", "F2"]
    );
    for response in [
        json!({"ok":true}),
        json!({"files":[{"id":"F1"},{"id":"F2"}]}),
        json!({"ok":true,"files":[{"id":"F1"}]}),
        json!({"ok":true,"files":[{"id":"F1"},{"id":"F1"}]}),
        json!({"ok":true,"files":[{"id":"F1"},{"id":"F3"}]}),
    ] {
        assert!(
            completed_upload(&response, &staged)
                .unwrap_err()
                .delivery_uncertain()
        );
    }
}

#[tokio::test]
async fn missing_local_file_and_cancellation_fail_before_message_delivery() {
    // Reading the missing file/cancel flag happens before any transport call.
    let transport = Transport::new("fixture-cookie").unwrap();
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "http://127.0.0.1:1".into(),
        token: "fixture-token".into(),
    };
    for canceled in [false, true] {
        let path = std::env::temp_dir().join(format!("missing-upload-{}", uuid::Uuid::new_v4()));
        let error = upload_files(
            &transport,
            &SlackClient::default(),
            &workspace,
            UploadRequest {
                reply_broadcast: false,
                client_msg_id: "fixture-message".into(),
                channel: "C1".into(),
                thread_ts: None,
                initial_comment: "Keep caption 🚀".into(),
                files: vec![UploadFile {
                    filename: path.file_name().unwrap().to_str().unwrap().into(),
                    path,
                    progress: Arc::new(AtomicU64::new(0)),
                }],
                canceled: Arc::new(AtomicBool::new(canceled)),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, UploadMessageError::BeforeDelivery(_)));
        assert!(!error.delivery_uncertain());
        assert_eq!(matches!(error.error(), Error::UploadCanceled), canceled);
    }
}

#[test]
fn staged_receipt_requires_nonempty_unique_exact_ids() {
    use serde_json::json;
    for invalid in [
        vec![],
        vec![json!({"id":""})],
        vec![json!({"id":null})],
        vec![json!({"id":"F1"}), json!({"id":"F1"})],
    ] {
        assert!(upload_file_ids(&invalid).is_none());
    }
    assert_eq!(
        upload_file_ids(&[json!({"id":"F2"}), json!({"id":"F1"})])
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["F1", "F2"]
    );
}

async fn read_local_request(stream: &mut tokio::net::TcpStream) -> String {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    loop {
        let mut part = [0; 4096];
        let n = stream.read(&mut part).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&part[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let len = header
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn staged_receipt_barrier_is_awaited_and_rejection_or_owner_cancel_prevents_delivery() {
    use tokio::io::AsyncWriteExt;
    const NAME: &str = "日本語 🦊.txt";
    for broadcast in [false, true] {
        for outcome in ["reject", "cancel", "success"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (barrier_waiting_tx, barrier_waiting_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let committed = Arc::new(AtomicBool::new(false));
            let server_committed = committed.clone();
            let server = tokio::spawn(async move {
                // Obtain the ticket and stage bytes; neither posts a message.
                for index in 0..2 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let request = read_local_request(&mut stream).await;
                    let response = if index == 0 {
                        assert!(request.starts_with("POST /api/files.getUploadURLExternal?"));
                        let body = request.split_once("\r\n\r\n").unwrap().1;
                        let fields: std::collections::BTreeMap<String, String> =
                            url::form_urlencoded::parse(body.as_bytes())
                                .into_owned()
                                .collect();
                        assert_eq!(fields["filename"], NAME);
                        serde_json::json!({"ok":true,"file_id":"F1","upload_url":format!("http://{address}/bytes")}).to_string()
                    } else {
                        assert!(request.starts_with("POST /bytes "));
                        "OK".into()
                    };
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
                }
                barrier_waiting_rx.await.unwrap();
                // A suspended durable write must keep the delivery endpoint untouched.
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(40), listener.accept())
                        .await
                        .is_err()
                );
                release_tx.send(()).unwrap();
                if outcome == "success" {
                    for index in 0..if broadcast { 2 } else { 1 } {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        assert!(server_committed.load(Ordering::SeqCst));
                        let request = read_local_request(&mut stream).await;
                        assert!(request.starts_with(if index == 0 {
                            "POST /api/files.completeUploadExternal?"
                        } else {
                            "POST /api/files.share?"
                        }));
                        let response = if index == 0 {
                            let body = request.split_once("\r\n\r\n").unwrap().1;
                            let fields: std::collections::BTreeMap<String,String> = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
                            let files: serde_json::Value = serde_json::from_str(&fields["files"]).unwrap();
                            assert_eq!(files[0]["title"], NAME);
                            serde_json::json!({"ok":true,"files":[{"id":"F1"}]})
                        } else {
                            serde_json::json!({"ok":true,"files":[{"id":"F1"}],"file_msg_ts":"101.000001","channel":"C1","thread_ts":"100.000001","client_msg_id":"fixture-message"})
                        }
                        .to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
                    }
                }
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(80), listener.accept())
                        .await
                        .is_err()
                );
            });
            let path =
                std::env::temp_dir().join(format!("upload-receipt-{}.txt", uuid::Uuid::new_v4()));
            tokio::fs::write(&path, b"synthetic bytes").await.unwrap();
            let workspace = WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: format!("http://{address}"),
                token: "synthetic-token".into(),
            };
            let canceled = Arc::new(AtomicBool::new(false));
            let callback_canceled = canceled.clone();
            let result = upload_files_with_receipt(
                &Transport::new("synthetic-cookie").unwrap(),
                &SlackClient::default(),
                &workspace,
                UploadRequest {
                    reply_broadcast: broadcast,
                    client_msg_id: "fixture-message".into(),
                    channel: "C1".into(),
                    thread_ts: Some("100.000001".into()),
                    initial_comment: "Fixture".into(),
                    files: vec![UploadFile {
                        filename: NAME.into(),
                        path: path.clone(),
                        progress: Arc::new(AtomicU64::new(0)),
                    }],
                    canceled,
                },
                move |staged| async move {
                    assert_eq!(staged.file_ids, vec!["F1"]);
                    barrier_waiting_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    if outcome == "reject" {
                        return Err(Error::Transport("receipt storage failed".into()));
                    }
                    if outcome == "cancel" {
                        callback_canceled.store(true, Ordering::Relaxed);
                    }
                    committed.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;
            tokio::fs::remove_file(path).await.unwrap();
            if outcome == "success" {
                assert_eq!(result.unwrap().file_ids, vec!["F1"]);
            } else {
                let error = result.unwrap_err();
                assert!(matches!(error, UploadMessageError::BeforeDelivery(_)));
                assert!(!error.delivery_uncertain());
                assert_eq!(
                    matches!(error.error(), Error::UploadCanceled),
                    outcome == "cancel"
                );
            }
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn named_files_keep_metadata_for_normal_and_unshared_uploads() {
    use tokio::io::AsyncWriteExt;
    const NAME: &str = "résumé 日本語 🦊 & 100%.txt";
    for unshared in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for index in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_local_request(&mut stream).await;
                let body = request.split_once("\r\n\r\n").unwrap().1;
                let fields: std::collections::BTreeMap<String, String> =
                    url::form_urlencoded::parse(body.as_bytes())
                        .into_owned()
                        .collect();
                let response = match index {
                    0 => {
                        assert!(request.starts_with("POST /api/files.getUploadURLExternal?"));
                        assert_eq!(fields["filename"], NAME);
                        assert_eq!(fields["length"], "10");
                        serde_json::json!({"ok":true,"file_id":"FNAME","upload_url":format!("http://{address}/bytes")}).to_string()
                    }
                    1 => {
                        assert!(request.starts_with("POST /bytes "));
                        assert_eq!(body, "named file");
                        "OK".into()
                    }
                    _ => {
                        assert!(request.starts_with("POST /api/files.completeUploadExternal?"));
                        let files: serde_json::Value =
                            serde_json::from_str(&fields["files"]).unwrap();
                        assert_eq!(files, serde_json::json!([{"id":"FNAME","title":NAME}]));
                        assert_eq!(
                            fields.get("channel_id").map(String::as_str),
                            if unshared { None } else { Some("C1") }
                        );
                        serde_json::json!({"ok":true,"files":[{"id":"FNAME"}]}).to_string()
                    }
                };
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
            }
        });
        let path = std::env::temp_dir().join(format!("opaque-upload-{}.txt", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, b"named file").await.unwrap();
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: format!("http://{address}"),
            token: "synthetic-token".into(),
        };
        let transport = Transport::new("synthetic-cookie").unwrap();
        let client = SlackClient::default();
        let files = vec![UploadFile {
            path: path.clone(),
            filename: NAME.into(),
            progress: Arc::new(AtomicU64::new(0)),
        }];
        let canceled = Arc::new(AtomicBool::new(false));
        if unshared {
            assert_eq!(
                upload_unshared_files(&transport, &client, &workspace, files, canceled)
                    .await
                    .unwrap(),
                vec!["FNAME"]
            );
        } else {
            let result = upload_files(
                &transport,
                &client,
                &workspace,
                UploadRequest {
                    reply_broadcast: false,
                    client_msg_id: "named-fixture".into(),
                    channel: "C1".into(),
                    thread_ts: None,
                    initial_comment: String::new(),
                    files,
                    canceled,
                },
            )
            .await
            .unwrap();
            assert_eq!(result.file_ids, vec!["FNAME"]);
        }
        server.await.unwrap();
        tokio::fs::remove_file(path).await.unwrap();
    }
}

#[tokio::test]
async fn invalid_later_upload_name_is_rejected_before_any_remote_staging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let path = std::env::temp_dir().join(format!("name-validation-{}.txt", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, b"first file").await.unwrap();
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: format!("http://{address}"),
        token: "synthetic-token".into(),
    };
    let transport = Transport::new("synthetic-cookie").unwrap();
    let client = SlackClient::default();
    let files = ["first.txt", "bad/name.txt"]
        .into_iter()
        .map(|filename| UploadFile {
            path: path.clone(),
            filename: filename.into(),
            progress: Arc::new(AtomicU64::new(0)),
        })
        .collect();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        upload_unshared_files(
            &transport,
            &client,
            &workspace,
            files,
            Arc::new(AtomicBool::new(false)),
        ),
    )
    .await
    .expect("metadata rejection must not wait for a remote response");
    assert!(
        matches!(result, Err(Error::Transport(ref message)) if message == "attachment has no valid filename")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    tokio::fs::remove_file(path).await.unwrap();
}
