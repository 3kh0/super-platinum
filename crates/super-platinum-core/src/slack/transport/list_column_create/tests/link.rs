use super::*;
fn link(address: std::net::SocketAddr) -> PreparedRequest {
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[4].1 = "link".into();
    fields.remove(5);
    r
}
#[tokio::test]
async fn link_requires_exact_six_fields_and_refuses_any_options_before_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(allowed(&link(address)));
    for value in ["{}", "null", "[]", r#"{"format":"link"}"#] {
        let mut r = link(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.insert(5, ("options".into(), value.into()));
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for index in 0..6 {
        let mut r = link(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.remove(index);
        assert!(!allowed(&r));
    }
    for kind in ["url", "channel", "text", "user"] {
        let mut r = link(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = kind.into();
        assert!(!allowed(&r));
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
