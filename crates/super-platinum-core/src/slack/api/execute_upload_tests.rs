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
                files: vec![(path, Arc::new(AtomicU64::new(0)))],
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
