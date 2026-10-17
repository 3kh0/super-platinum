use super::*;
#[tokio::test]
async fn rating_closed_form_rejects_unsupported_options_before_any_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for options in [
        r#"{"emoji":":star:","max":0}"#,
        r#"{"emoji":":star:","max":6}"#,
        r#"{"emoji":":star:","max":2.5}"#,
        r#"{"emoji":":star:","max":5,"emoji_team_id":null}"#,
        r#"{"emoji":":star:","max":5,"emoji_url":"https://fixture.invalid/x"}"#,
        r#"{"emoji":":star:","max":1,"max":5}"#,
        r#"{"emoji":":star:","max":5,"default":3}"#,
        r#"{"emoji":"star","max":5}"#,
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = "rating".into();
        fields[5].1 = options.into();
        assert!(
            matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ),
            "{options}"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
