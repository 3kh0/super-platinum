use super::*;
use crate::{config::WorkspaceSession, slack::SlackClient};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request() -> PreparedRequest {
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    SlackClient::default().rest_form(
        &owner,
        "files.info",
        vec![
            ("file", "F12345678".into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
            ("skip_shares", "true".into()),
            ("_x_reason", "files-store-unknown-fetch".into()),
        ],
    )
}

fn known_request() -> PreparedRequest {
    let mut req = request();
    let RequestBody::Form(fields) = &mut req.body else {
        unreachable!()
    };
    fields.retain(|(k, _)| k != "skip_shares");
    fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
        "files-store.ConditionalFetchManager.fetch".into();
    req
}

#[test]
fn known_manager_exact_request_does_not_admit_mixed_or_unknown_shapes() {
    assert!(allowed(&known_request()));
    assert!(!super::super::allowed(&known_request()));
    for change in 0..11 {
        let mut req = known_request();
        match change {
            0 => req.method = "GET",
            1 => req.url = req.url.replace("files.info", "files.delete"),
            2 => req.url = req.url.replace("fixture.slack.com", "slack.com.invalid"),
            3 => req.url.push_str("#fragment"),
            _ => {
                let RequestBody::Form(fields) = &mut req.body else {
                    unreachable!()
                };
                match change {
                    4 => fields.push(("skip_shares".into(), "true".into())),
                    5 => fields.push((
                        "_x_reason".into(),
                        "files-store.ConditionalFetchManager.fetch".into(),
                    )),
                    6 => {
                        fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                            "files-store-unknown-fetch".into()
                    }
                    7 => fields.iter_mut().find(|(k, _)| k == "page").unwrap().1 = "2".into(),
                    8 => fields.iter_mut().find(|(k, _)| k == "file").unwrap().1 = "Fshort".into(),
                    9 => fields.push(("unexpected".into(), "true".into())),
                    _ => fields
                        .iter_mut()
                        .find(|(k, _)| k == "token")
                        .unwrap()
                        .1
                        .clear(),
                }
            }
        }
        assert!(!allowed(&req), "{change}");
    }
}

#[tokio::test]
async fn retired_known_metadata_never_requests_an_http_capability() {
    let transport = Transport::disconnected_fixture("synthetic");
    assert!(
        matches!(transport.execute_file_metadata_current(known_request(), || false).await,
        Err(Error::Offline(text)) if text == "File metadata owner retired")
    );
}

#[test]
fn exact_metadata_request_does_not_widen_other_singleton_read_admission() {
    assert!(allowed(&request()));
    assert!(!super::super::allowed(&request()));
    for change in 0..9 {
        let mut req = request();
        match change {
            0 => req.method = "GET",
            1 => req.url = req.url.replace("files.info", "files.delete"),
            2 => req.url = req.url.replace("https:", "http:"),
            3 => req.url = req.url.replace("fixture.slack.com", "slack.com.invalid"),
            4 => req.url.push_str("#fragment"),
            _ => {
                let RequestBody::Form(fields) = &mut req.body else {
                    unreachable!()
                };
                match change {
                    5 => fields.push(("file".into(), "F12345678".into())),
                    6 => {
                        fields
                            .iter_mut()
                            .find(|(k, _)| k == "skip_shares")
                            .unwrap()
                            .1 = "false".into()
                    }
                    7 => {
                        fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                            "other".into()
                    }
                    _ => fields.iter_mut().find(|(k, _)| k == "file").unwrap().1 = "Fshort".into(),
                }
            }
        }
        assert!(!allowed(&req), "{change}");
    }
}

#[tokio::test]
async fn retired_metadata_never_requests_an_http_capability() {
    let transport = Transport::disconnected_fixture("synthetic");
    let error = transport
        .execute_file_metadata_current(request(), || false)
        .await
        .unwrap_err();
    assert!(matches!(error,Error::Offline(text) if text=="File metadata owner retired"));
}

async fn server(
    status: &str,
    headers: &str,
    body: &[u8],
) -> (
    PreparedRequest,
    Arc<AtomicUsize>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _=&mut stopped=>break,
                accepted=listener.accept()=>{
                    let (mut stream,_)=accepted.unwrap();
                    let mut input=[0;8192];
                    stream.read(&mut input).await.unwrap();
                    seen.fetch_add(1,Ordering::SeqCst);
                    stream.write_all(&response).await.unwrap();
                }
            }
        }
    });
    let mut request = request();
    request.url = format!("http://{address}/api/files.info");
    (request, count, stop, task)
}

#[tokio::test]
async fn actual_rate_limit_keeps_full_deadline_and_never_retries_automatically() {
    for (status, body) in [
        ("429 Too Many Requests", ""),
        ("200 OK", r#"{"ok":false,"error":"ratelimited"}"#),
    ] {
        let (req, count, stop, server) =
            server(status, "Retry-After: 120\r\n", body.as_bytes()).await;
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.file_metadata_admitted(&req, || true),
        )
        .await
        .unwrap();
        let _ = stop.send(());
        server.await.unwrap();
        assert!(matches!(
            result,
            Err(Error::RateLimited {
                retry_after_secs: Some(120)
            })
        ));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn actual_metadata_preserves_raw_omissions_and_rejects_invalid_utf8() {
    for (body, valid) in [
        (
            br#"{"ok":true,"file":{"id":"F12345678","name":"","opaque":{"flag":false}}}"#
                .as_slice(),
            true,
        ),
        (
            b"{\"ok\":true,\"file\":{\"id\":\"F12345678\",\"name\":\"\xff\"}}".as_slice(),
            false,
        ),
    ] {
        let (req, count, stop, server) = server(
            "200 OK",
            "Content-Type: application/json; charset=windows-1252\r\n",
            body,
        )
        .await;
        let transport = Transport::new("synthetic").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            transport.file_metadata_admitted(&req, || true),
        )
        .await
        .unwrap();
        let _ = stop.send(());
        server.await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        if valid {
            let raw = result.unwrap();
            assert_eq!(raw["file"]["name"], "");
            assert!(raw["file"].get("permalink").is_none());
            assert_eq!(raw["file"]["opaque"]["flag"], false);
        } else {
            assert!(
                matches!(result,Err(Error::Transport(ref text)) if text=="invalid original response UTF8")
            );
        }
    }
}
