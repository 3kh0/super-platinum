use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, ip: bool) -> Request {
    let w = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    };
    let mut r = if ip {
        Request::ip(&crate::slack::SlackClient::default(), &w, "F12345678")
    } else {
        Request::metadata(&crate::slack::SlackClient::default(), &w, "F12345678")
    }
    .unwrap();
    r.inner.url = format!(
        "http://{address}/api/{}",
        if ip {
            "teams.authorized.ip.range.checkIp"
        } else {
            "files.info"
        }
    );
    r
}
async fn wire(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 65536);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|v| {
                    let (k, v) = v.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() == end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
#[tokio::test]
async fn isolated_image_actual_ip_multipart_and_metadata_form_have_single_attempts() {
    for ip in [false, true] {
        for success in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let r = request(listener.local_addr().unwrap(), ip);
            let transport = Transport::new("fixture-cookie").unwrap();
            let server = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = wire(&mut socket).await;
                let (headers, body) = request.split_once("\r\n\r\n").unwrap();
                assert!(
                    headers
                        .to_ascii_lowercase()
                        .contains("cookie: d=fixture-cookie")
                );
                if ip {
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("content-type: multipart/form-data; boundary=")
                    );
                    assert!(body.contains("name=\"token\"\r\n\r\nfixture-token"));
                    assert!(body.contains("name=\"file\"\r\n\r\nF12345678"));
                    assert_eq!(
                        body.matches("content-disposition:").count()
                            + body.matches("Content-Disposition:").count(),
                        2
                    );
                } else {
                    assert!(body.contains(
                        "token=fixture-token&file=F12345678&count=1&page=1&truncate=false"
                    ));
                }
                if success {
                    let body = r#"{"ok":true,"ip_allowed":true,"label":"synthetic"}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                } else {
                    socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nRetry-After: 17\r\nConnection: close\r\n\r\n").await.unwrap();
                }
                assert!(
                    tokio::time::timeout(Duration::from_millis(150), listener.accept())
                        .await
                        .is_err()
                );
            };
            let (result, ()) =
                tokio::join!(transport.execute_file_image_current(&r, || true), server);
            if success {
                assert_eq!(
                    result.unwrap(),
                    serde_json::json!({"ok":true,"ip_allowed":true,"label":"synthetic"})
                );
            } else {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(17)
                    })
                ));
            }
        }
    }
}
#[tokio::test]
async fn isolated_image_transport_retirement_never_publishes_or_retries() {
    use std::cell::Cell;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let r = request(listener.local_addr().unwrap(), true);
    let transport = Transport::new("fixture-cookie").unwrap();
    let current = Cell::new(false);
    assert!(matches!(
        transport
            .execute_file_image_current(&r, || current.get())
            .await,
        Err(Error::Offline(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    current.set(true);
    transport.set_health(Health::Offline);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            transport.execute_file_image_current(&r, || current.get())
        )
        .await
        .is_err(),
        "known offline must hold without dispatch"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    transport.set_health(Health::Online);
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        wire(&mut socket).await;
        current.set(false);
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    let (result, ()) = tokio::join!(
        transport.execute_file_image_current(&r, || current.get()),
        server
    );
    assert!(matches!(result, Err(Error::Offline(_))));
}
