use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(address: std::net::SocketAddr, options: &str) -> PreparedRequest {
    PreparedRequest {
        method: "POST".into(),
        url: format!("http://{address}/api/lists.columns.update"),
        headers: vec![],
        body: RequestBody::Form(vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F1".into()),
            ("key".into(), "estimate".into()),
            ("name".into(), "Effort".into()),
            ("options".into(), options.into()),
            ("backfill_missing_cells_with_default".into(), "true".into()),
            ("_x_reason".into(), "update-column".into()),
        ]),
    }
}
#[tokio::test]
async fn exact_number_forms_preserve_no_retry_for_received_or_lost_responses() {
    for precision in 0..=5 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            if precision != 5 {
                let status = if precision == 4 { 429 } else { 503 };
                stream.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: 11\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{{\"ok\":true}}").as_bytes()).await.unwrap();
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(80), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = Transport::new("fixture-cookie")
            .unwrap()
            .execute_list_column_rename(
                request(address, &format!("{{\"precision\":{precision}}}")),
                || true,
            )
            .await;
        assert!(matches!(
            result,
            Err(ListColumnRenameFailure::Unconfirmed(_))
        ));
        if precision == 4 {
            assert!(matches!(
                result,
                Err(ListColumnRenameFailure::Unconfirmed(Error::RateLimited {
                    retry_after_secs: Some(7)
                }))
            ));
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn expanded_name_validator_refuses_other_shapes_and_stale_or_offline_owners() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for options in [
        "{}",
        "{\"precision\":6}",
        "{\"precision\":2.0}",
        "{\"precision\":\"2\"}",
        "{\"precision\":2,\"future\":true}",
        "{\"precision\":2,\"precision\":2}",
        "{ \"precision\":2}",
    ] {
        assert!(matches!(
            transport
                .execute_list_column_rename(request(address, options), || true)
                .await,
            Err(ListColumnRenameFailure::NotRequested(_))
        ));
    }
    let mut extra = request(address, "{\"precision\":2}");
    if let RequestBody::Form(fields) = &mut extra.body {
        fields.push(("type".into(), "number".into()));
    }
    assert!(matches!(
        transport.execute_list_column_rename(extra, || true).await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address, "{\"precision\":2}"), || false)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    transport.set_health(Health::Offline);
    assert!(matches!(
        transport
            .execute_list_column_rename(request(address, "{\"precision\":2}"), || true)
            .await,
        Err(ListColumnRenameFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
