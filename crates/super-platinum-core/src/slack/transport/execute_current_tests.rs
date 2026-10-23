use super::*;
use std::sync::atomic::AtomicBool;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

fn request(listener: &TcpListener) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!(
            "http://{}/api/conversations.info?fixture=owned",
            listener.local_addr().unwrap()
        ),
        headers: vec![],
        body: RequestBody::Form(vec![]),
    }
}
async fn receive(listener: &TcpListener) -> TcpStream {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("expected loopback request")
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let mut chunk = [0; 1024];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0 && bytes.len() + n <= 16_384);
        bytes.extend_from_slice(&chunk[..n]);
    }
    assert!(bytes.starts_with(b"POST /api/conversations.info?fixture=owned "));
    socket
}
async fn respond(mut socket: TcpStream, status: &str, body: &str, retry_after: Option<u64>) {
    let retry = retry_after
        .map(|n| format!("Retry-After: {n}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await.unwrap();
    socket.shutdown().await.unwrap();
}
fn superseded(result: Result<Value, Error>) {
    assert!(matches!(result, Err(Error::Api(code)) if code == "request_superseded"));
}

#[tokio::test]
async fn execute_current_initial_retirement_never_holds_or_sends() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    superseded(
        tokio::time::timeout(
            Duration::from_millis(200),
            transport.execute_current(request(&listener), || false),
        )
        .await
        .expect("retired request entered offline hold"),
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn execute_current_retirement_during_link_hold_prevents_recovery_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    let current = AtomicBool::new(true);
    let entered = Notify::new();
    let read = transport.execute_current(request(&listener), || {
        entered.notify_one();
        current.load(Ordering::SeqCst)
    });
    let retire = async {
        entered.notified().await;
        current.store(false, Ordering::SeqCst);
        transport.set_health(Health::Unknown);
    };
    let (result, ()) = tokio::join!(read, retire);
    superseded(result);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn execute_current_retirement_during_retry_delay_sends_no_second_attempt() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req = request(&listener);
    let server = tokio::spawn(async move {
        respond(
            receive(&listener).await,
            "503 Service Unavailable",
            "{}",
            None,
        )
        .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(450), listener.accept())
                .await
                .is_err()
        );
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    let current = AtomicBool::new(true);
    let completed_attempt = Notify::new();
    let read = transport.execute_current(req, || {
        // Only the post-response guard sees Online on this fresh transport.
        if transport.health() == Health::Online {
            completed_attempt.notify_one();
        }
        current.load(Ordering::SeqCst)
    });
    let retire = async {
        completed_attempt.notified().await;
        current.store(false, Ordering::SeqCst);
    };
    let (result, ()) = tokio::join!(read, retire);
    superseded(result);
    server.await.unwrap();
}

#[tokio::test]
async fn execute_current_refuses_response_after_dispatched_owner_retires() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req = request(&listener);
    let current = Arc::new(AtomicBool::new(true));
    let original = current.clone();
    let server = tokio::spawn(async move {
        let socket = receive(&listener).await;
        original.store(false, Ordering::SeqCst);
        respond(socket, "200 OK", r#"{"ok":true,"actual":1}"#, None).await;
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    superseded(
        transport
            .execute_current(req, || current.load(Ordering::SeqCst))
            .await,
    );
    server.await.unwrap();
    assert_eq!(transport.health(), Health::Online);
}

#[tokio::test]
async fn execute_current_and_ordinary_execute_preserve_success() {
    for guarded in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let req = request(&listener);
        let server = tokio::spawn(async move {
            respond(
                receive(&listener).await,
                "200 OK",
                r#"{"ok":true,"actual":[1,2]}"#,
                None,
            )
            .await;
        });
        let transport = Transport::new("fixture-cookie").unwrap();
        let result = if guarded {
            transport.execute_current(req, || true).await
        } else {
            transport.execute(req).await
        };
        assert_eq!(
            result.unwrap(),
            serde_json::json!({"ok":true,"actual":[1,2]})
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn execute_current_keeps_retry_after_delay_and_current_retry_success() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req = request(&listener);
    let server = tokio::spawn(async move {
        let socket = receive(&listener).await;
        let start = tokio::time::Instant::now();
        respond(socket, "429 Too Many Requests", "{}", Some(1)).await;
        let socket = receive(&listener).await;
        assert!(start.elapsed() >= Duration::from_secs(1));
        respond(socket, "200 OK", r#"{"ok":true}"#, None).await;
    });
    let transport = Transport::new("fixture-cookie").unwrap();
    assert_eq!(
        transport.execute_current(req, || true).await.unwrap(),
        serde_json::json!({"ok":true})
    );
    server.await.unwrap();
}
