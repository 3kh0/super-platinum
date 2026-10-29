use super::*;
use std::{cell::Cell, sync::atomic::AtomicBool};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};

fn request(listener: &TcpListener) -> PreparedRequest {
    PreparedRequest {
        method: "POST",
        url: format!("http://{}/api/users/search", listener.local_addr().unwrap()),
        headers: vec![],
        body: RequestBody::Json(serde_json::json!({"count":24})),
    }
}
async fn reply(listener: TcpListener, current: Option<Arc<AtomicBool>>, rate: bool) {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let mut chunk = [0; 1024];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0 && bytes.len() + n <= 16384);
        bytes.extend_from_slice(&chunk[..n]);
    }
    if let Some(current) = current {
        current.store(false, Ordering::SeqCst);
    }
    let (status, extra, body) = if rate {
        ("429 Too Many Requests", "Retry-After: 37\r\n", "{}")
    } else {
        ("200 OK", "", "{\"ok\":true}")
    };
    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    socket.shutdown().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn admission_after_link_hold_refuses_new_cooldown_without_send_or_attempt_observation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = Transport::new("synthetic").unwrap();
    transport.set_health(Health::Offline);
    let entered = Notify::new();
    let cooling = Cell::new(false);
    let admissions = Cell::new(0);
    let observations = Cell::new(0);
    let read = transport.execute_admitted(
        request(&listener),
        || {
            entered.notify_one();
            true
        },
        || {
            admissions.set(admissions.get() + 1);
            !cooling.get()
        },
        |_| observations.set(observations.get() + 1),
    );
    let elsewhere = async {
        entered.notified().await;
        cooling.set(true);
        transport.set_health(Health::Unknown);
    };
    let (result, ()) = tokio::join!(read, elsewhere);
    assert!(result.unwrap().is_none());
    assert_eq!(admissions.get(), 1);
    assert_eq!(observations.get(), 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn attempt_observation_keeps_current_429_and_precedes_stale_query_retirement() {
    for stale in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let req = request(&listener);
        assert!(!req.retry_safe());
        let current = Arc::new(AtomicBool::new(true));
        let server = tokio::spawn(reply(listener, stale.then(|| current.clone()), true));
        let budget = Cell::new(0);
        let observations = Cell::new(0);
        let transport = Transport::new("synthetic").unwrap();
        let result = transport
            .execute_admitted(
                req,
                || current.load(Ordering::SeqCst),
                || budget.get() == 0,
                |result| {
                    observations.set(observations.get() + 1);
                    if let Err(Error::RateLimited {
                        retry_after_secs: Some(seconds),
                    }) = result
                    {
                        budget.set(*seconds);
                    }
                },
            )
            .await;
        assert_eq!(budget.get(), 37);
        assert_eq!(observations.get(), 1);
        if stale {
            assert!(matches!(result,Err(Error::Api(s)) if s=="request_superseded"));
        } else {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(37)
                })
            ));
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn legacy_execution_keeps_three_current_checks_and_nonretry_safe_search() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let req = request(&listener);
    assert!(!req.retry_safe());
    let server = tokio::spawn(reply(listener, None, false));
    let calls = Cell::new(0);
    let transport = Transport::new("synthetic").unwrap();
    let result = transport
        .execute_current(req, || {
            calls.set(calls.get() + 1);
            true
        })
        .await
        .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(calls.get(), 3);
    server.await.unwrap();
}
