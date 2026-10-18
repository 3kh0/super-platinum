use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn request(url: String) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url,
        headers: vec![],
        body: RequestBody::Form(vec![]),
    }
}

#[test]
fn workflow_once_seam_has_exact_paths_and_method() {
    for name in [
        "workflows.triggers.trip",
        "apps.auth.external.authMapping.update",
        "sfdc.integration.listOrgs",
        "sfdc.integration.authUser",
        "sfdc.integration.auth.loginUrl",
        "sfdc.integration.disconnectUser",
    ] {
        let mut actual = request(format!("https://fixture.slack.com/api/{name}"));
        assert!(is_block_interaction_request(&actual));
        actual.method = "GET";
        assert!(!is_block_interaction_request(&actual));
        actual.method = "POST";
        actual.url.push_str("/arbitrary");
        assert!(!is_block_interaction_request(&actual));
    }
    assert!(!is_block_interaction_request(&request(
        "https://fixture.slack.com/api/chat.postMessage?workflow=/api/workflows.triggers.trip"
            .into()
    )));
}

#[tokio::test]
async fn actual_workflow_feedback_and_rate_limit_use_one_dispatch() {
    for (endpoint, status, headers, body) in [
        (
            "workflows.triggers.trip",
            "200 OK",
            "",
            r#"{"ok":false,"error":"pretrip_ack_required","pre_trip_info":{"unknown":"retained"}}"#,
        ),
        (
            "workflows.triggers.trip",
            "503 Unavailable",
            "",
            "uncertain",
        ),
        (
            "apps.auth.external.authMapping.update",
            "429 Too Many Requests",
            "Retry-After: 9\r\n",
            "limited",
        ),
        (
            "sfdc.integration.authUser",
            "503 Unavailable",
            "",
            "uncertain",
        ),
        (
            "sfdc.integration.disconnectUser",
            "503 Unavailable",
            "",
            "uncertain",
        ),
        (
            "sfdc.integration.auth.loginUrl",
            "429 Too Many Requests",
            "Retry-After: 9\r\n",
            "limited",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = Vec::new();
            while !input.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let size = socket.read(&mut chunk).await.unwrap();
                assert!(size > 0 && input.len() + size <= 16384);
                input.extend_from_slice(&chunk[..size]);
            }
            let first = std::str::from_utf8(&input).unwrap();
            assert!(first.starts_with(&format!("POST /api/{endpoint} ")));
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",body.len()).as_bytes()).await.unwrap();
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(350), listener.accept())
                    .await
                    .is_err(),
                "workflow action retried"
            );
        });
        let transport = Transport::new("synthetic-cookie").unwrap();
        let actual = transport
            .execute_block_interaction(request(format!("http://{address}/api/{endpoint}")))
            .await;
        match status {
            "200 OK" => assert_eq!(actual.unwrap()["pre_trip_info"]["unknown"], "retained"),
            "503 Unavailable" => {
                assert!(matches!(actual, Err(Error::HttpStatus { status: 503, .. })))
            }
            _ => assert!(matches!(
                actual,
                Err(Error::RateLimited {
                    retry_after_secs: Some(9),
                    ..
                })
            )),
        }
        server.await.unwrap();
    }
}
