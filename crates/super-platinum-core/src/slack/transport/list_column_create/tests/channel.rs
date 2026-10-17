use super::*;
fn channel(address: std::net::SocketAddr) -> PreparedRequest {
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[4].1 = "channel".into();
    fields[5].1 =
        r#"{"format":"single_entity","show_member_name":true,"default_value":null}"#.into();
    r
}
#[tokio::test]
async fn channel_closed_form_refuses_incoherent_defaults_aliases_and_malformed_shapes_before_dispatch()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(allowed(&channel(addr)));
    for options in [
        "null",
        "{}",
        r#"{"format":"single_entity","showMemberName":true,"defaultValue":null}"#,
        r#"{"format":"single_entity","show_member_name":false,"default_value":null}"#,
        r#"{"format":"single_entity","show_member_name":true,"default_value":"C00000001"}"#,
        r#"{"format":"single_entity","show_member_name":true,"default_value":null,"default_value_typed":null}"#,
        r#"{"format":"single_entity","show_member_name":true,"default_value":null,"default_value":null}"#,
    ] {
        let mut r = channel(addr);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[5].1 = options.into();
        assert!(matches!(
            transport.execute_list_column_create(r, || true).await,
            Err(ListColumnCreateFailure::NotRequested(_))
        ));
    }
    for field in 0..7 {
        let mut r = channel(addr);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields.remove(field);
        assert!(!allowed(&r));
    }
    for kind in ["assignee", "todo_assignee", "url", "link"] {
        let mut r = channel(addr);
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
