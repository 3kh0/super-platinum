use super::*;
fn select(address: std::net::SocketAddr) -> PreparedRequest {
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[4].1 = "select".into();
    fields[5].1 = r#"{"choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}],"format":"single_select"}"#.into();
    r
}
#[tokio::test]
async fn select_rejects_alternate_options_special_types_and_missing_fields_before_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(allowed(&select(address)));
    for options in [
        "null",
        "{}",
        r#"{"format":"single_select","choices":[]}"#,
        r#"{"format":"multiple","choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}]}"#,
        r#"{"format":"single_select","choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH","default":true}]}"#,
        r#"{"format":"single_select","choices":[{"color":"red","label":"Ready","value":"OptABCDEFGH"}]}"#,
        r#"{"format":"single_select","choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}],"default_value":"OptABCDEFGH"}"#,
        r#"{"format":"single_select","format":"single_select","choices":[{"color":"cyan","label":"Ready","value":"OptABCDEFGH"}]}"#,
    ] {
        let mut r = select(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[5].1 = options.into();
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for kind in [
        "date",
        "multi_select",
        "timestamp",
        "text",
        "number",
        "checkbox",
    ] {
        let mut r = select(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = kind.into();
        assert!(!allowed(&r));
    }
    for index in 0..7 {
        for duplicate in [false, true] {
            let mut r = select(address);
            let RequestBody::Form(fields) = &mut r.body else {
                panic!()
            };
            if duplicate {
                fields.push(fields[index].clone());
            } else {
                fields.remove(index);
            }
            assert!(matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ));
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn select_lost_reply_rate_limit_and_retired_owner_do_not_retry() {
    for reply in [
        None,
        Some(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
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
            .execute_list_column_create(select(address), || true)
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
    assert!(matches!(
        Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_create(select(listener.local_addr().unwrap()), || false)
            .await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
