use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{address}/api/saved.update"),
        headers: Vec::new(),
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-only".into()),
            ("item_type".into(), "list_record".into()),
            ("item_id".into(), "F_TASK".into()),
            ("ts".into(), "ROW".into()),
            ("date_due".into(), "300".into()),
            ("_x_reason".into(), "saved-api/updateSavedListRecord".into()),
        ]),
    }
}
#[tokio::test]
async fn reminders_refuse_wrong_shape_offline_and_retired_owner_without_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let t = Transport::new("fixture-cookie").unwrap();
    assert!(matches!(
        t.execute_list_reminder(request(address), || false).await,
        Err(ListReminderFailure::NotRequested(_))
    ));
    t.set_health(Health::Offline);
    assert!(matches!(
        t.execute_list_reminder(request(address), || true).await,
        Err(ListReminderFailure::NotRequested(Error::Offline(_)))
    ));
    t.set_health(Health::Online);
    for i in 0..6 {
        let mut r = request(address);
        if let RequestBody::Form(f) = &mut r.body {
            match i {
                0 => f[1].1 = "message".into(),
                1 => f.push(("todo_state".into(), "completed".into())),
                2 => f[4].1 = "0".into(),
                3 => f[4].1 = "true".into(),
                4 => f[5].1 = "other".into(),
                _ => f[3].1 = "".into(),
            }
        }
        assert!(matches!(
            t.execute_list_reminder(r, || true).await,
            Err(ListReminderFailure::NotRequested(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn reminder_http_429_is_one_attempt_and_keeps_retry_after() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 1024];
        loop {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let size = header
                    .lines()
                    .find_map(|s| {
                        s.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("POST /api/saved.update"));
        assert!(text.contains("item_type=list_record&item_id=F_TASK&ts=ROW&date_due=300"));
        stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 11\r\nContent-Type: application/json\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{\"ok\":true}").await.unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let t = Transport::new("fixture-cookie").unwrap();
    let result = t.execute_list_reminder(request(address), || true).await;
    assert!(matches!(
        result,
        Err(ListReminderFailure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(7)
        }))
    ));
    server.await.unwrap();
}
