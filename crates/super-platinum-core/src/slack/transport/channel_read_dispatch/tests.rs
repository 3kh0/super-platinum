use super::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn local(url: String, kind: Kind) -> ChannelReadAttempt {
    ChannelReadAttempt::new(
        PreparedRequest {
            method: "POST",
            url,
            headers: vec![],
            body: RequestBody::Json(json!({})),
        },
        kind,
    )
}

#[test]
fn closed_directory_admission_and_unpolled_future_do_not_mint_send_evidence() {
    let read = local(
        "https://fixture.slack.com/api/chat.postMessage".into(),
        Kind::Directory,
    );
    assert!(ChannelReadAttempt::directory(read.request).is_err());
    let transport = Transport::disconnected_fixture("synthetic-cookie");
    let read = local(
        "http://127.0.0.1:1/api/conversations.genericInfo".into(),
        Kind::Generic,
    );
    let receipt = read.receipt();
    drop(transport.execute_channel_read(read));
    assert!(!receipt.entered_send());
}

#[tokio::test]
async fn split_response_retains_original_bytes_for_ordered_adoption() {
    let body = r#"{ "ok":true, "team_prefs":{"workspace_prefs":{"Tz":{},"Ta":{}}} }"#;
    let (read, server) = respond("200 OK", "", body, Kind::Split).await;
    let receipt = read.receipt();
    let transport = Transport::new("synthetic-cookie").unwrap();
    let result = transport.execute_channel_read_once(&read).await.unwrap();
    server.await.unwrap();
    let ChannelReadResponse::Split { raw, bytes } = result else {
        panic!("split response")
    };
    assert_eq!(bytes, body);
    assert_eq!(raw["ok"], true);
    assert!(receipt.entered_send());
}

#[tokio::test]
async fn disconnected_client_and_local_link_hold_leave_receipt_unset() {
    let transport = Transport::disconnected_fixture("synthetic-cookie");
    let read = local("http://127.0.0.1:1/read".into(), Kind::Generic);
    let receipt = read.receipt();
    assert!(matches!(
        transport.execute_channel_read(read).await,
        Err(Error::Offline(_))
    ));
    assert!(!receipt.entered_send());
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(Health::Offline);
    let read = local("http://127.0.0.1:1/read".into(), Kind::Generic);
    let receipt = read.receipt();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            transport.execute_channel_read(read)
        )
        .await
        .is_err()
    );
    assert!(!receipt.entered_send());
}

#[tokio::test]
async fn local_builder_rejection_does_not_enter_send() {
    let transport = Transport::new("synthetic-cookie").unwrap();
    let mut read = local("http://127.0.0.1:1/read".into(), Kind::Generic);
    read.request
        .headers
        .push(("invalid\nheader".into(), "value".into()));
    let receipt = read.receipt();
    assert!(transport.execute_channel_read_once(&read).await.is_err());
    assert!(!receipt.entered_send());
}

async fn respond(
    status: &str,
    extra: &str,
    body: &str,
    kind: Kind,
) -> (ChannelReadAttempt, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes = format!("HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut input = [0; 8192];
        stream.read(&mut input).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    (local(format!("http://{address}/read"), kind), server)
}

#[tokio::test]
async fn received_errors_preserve_categories_and_original_receipt() {
    for (status, extra, body, kind) in [
        ("429 Too Many Requests", "Retry-After: 17\r\n", "", "rate"),
        ("503 Service Unavailable", "Retry-After: 19\r\n", "", "http"),
        (
            "200 OK",
            "",
            r#"{"ok":false,"error":"fixture_failure"}"#,
            "api",
        ),
        ("200 OK", "", "{", "decode"),
    ] {
        let (read, server) = respond(status, extra, body, Kind::Generic).await;
        let receipt = read.receipt();
        let other = local("http://127.0.0.1:1/read".into(), Kind::Generic).receipt();
        let transport = Transport::new("synthetic-cookie").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.execute_channel_read_once(&read),
        )
        .await
        .unwrap();
        server.await.unwrap();
        match kind {
            "rate" => assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(17)
                })
            )),
            "http" => assert!(matches!(
                result,
                Err(Error::HttpStatus {
                    status: 503,
                    retry_after_secs: Some(19)
                })
            )),
            "api" => {
                assert!(matches!(result, Err(Error::Api(ref value)) if value == "fixture_failure"))
            }
            _ => assert!(matches!(result, Err(Error::Transport(_)))),
        }
        assert!(receipt.entered_send());
        assert!(!other.entered_send());
        assert_eq!(transport.health(), Health::Online);
    }
}

#[tokio::test]
async fn ordered_directory_success_keeps_payload_and_encounter_order() {
    let body = r#"{"results":{"z":[{"id":"C00000001"}],"a":[{"id":"C00000002"}]},"unknown":{"keep":true}}"#;
    let (read, server) = respond("200 OK", "", body, Kind::Directory).await;
    let receipt = read.receipt();
    let transport = Transport::new("synthetic-cookie").unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        transport.execute_channel_read_once(&read),
    )
    .await
    .unwrap()
    .unwrap();
    server.await.unwrap();
    let ChannelReadResponse::Directory(value) = response else {
        panic!("directory response")
    };
    assert_eq!(value.raw, serde_json::from_str::<Value>(body).unwrap());
    assert_eq!(value.keys, Some(vec!["z".into(), "a".into()]));
    assert!(receipt.entered_send());
}

#[tokio::test]
async fn original_receipt_survives_cancellation_after_actual_send_entry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (arrived, arrival) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut input = [0; 8192];
        let size = stream.read(&mut input).await.unwrap();
        assert!(size > 0);
        arrived.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let read = local(format!("http://{address}/read"), Kind::Generic);
    let receipt = read.receipt();
    let transport = Transport::new("synthetic-cookie").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let call = transport.execute_channel_read(read);
        tokio::pin!(call);
        tokio::select! {
            _ = &mut call => panic!("server must remain held"),
            result = arrival => result.unwrap(),
        }
        assert!(receipt.entered_send());
        // Leaving this scope drops the live call while preserving its receipt.
    })
    .await;
    server.abort();
    result.unwrap();
    assert!(receipt.entered_send());
}
