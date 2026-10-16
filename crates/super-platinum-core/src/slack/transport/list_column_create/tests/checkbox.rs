use super::*;
fn checkbox(address: std::net::SocketAddr) -> PreparedRequest {
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[4].1 = "checkbox".into();
    fields.remove(5);
    r
}
#[tokio::test]
async fn checkbox_rejects_any_options_duplicate_omission_or_foreign_fields_without_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(allowed(&checkbox(address)));
    for options in [
        "{}",
        "null",
        "[]",
        r#"{"markAsDoneWhenChecked":false}"#,
        r#"{"format":"text"}"#,
    ] {
        let mut r = checkbox(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.insert(5, ("options".into(), options.into()));
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for index in 0..6 {
        for duplicate in [false, true] {
            let mut r = checkbox(address);
            let RequestBody::Form(fields) = &mut r.body else {
                panic!()
            };
            if duplicate {
                fields.push(fields[index].clone())
            } else {
                fields.remove(index);
            }
            assert!(matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ));
        }
    }
    for kind in ["text", "number", "completed", "todo_completed"] {
        let mut r = checkbox(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = kind.into();
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for extra in ["id", "key", "position", "backfill"] {
        let mut r = checkbox(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.push((extra.into(), "x".into()));
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn checkbox_rate_limit_lost_reply_and_offline_do_not_replay() {
    for reply in [
        Some(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
        None,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            if let Some(reply) = reply {
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_create(checkbox(address), || true)
            .await;
        if reply.is_some() {
            assert!(matches!(
                result,
                Err(ListColumnCreateFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(2)
                }))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ListColumnCreateFailure::Unconfirmed(_))
            ));
        }
        server.await.unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_column_create(checkbox(address), || true)
            .await,
        Err(ListColumnCreateFailure::NotRequested(Error::Offline(_)))
    ));
    transport.set_health(Health::Online);
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
