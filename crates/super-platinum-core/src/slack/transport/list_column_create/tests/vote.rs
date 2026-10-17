use super::*;
#[tokio::test]
async fn vote_closed_configuration_refuses_unknown_duplicate_and_incomplete_forms_before_dispatch()
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let transport = Transport::new("fixture-cookie").unwrap();
    for options in [
        r#"{}"#,
        r#"{"emoji":"heart"}"#,
        r#"{"emoji":null}"#,
        r#"{"emoji":":heart:","emoji_team_id":null}"#,
        r#"{"emoji":":heart:","max":5}"#,
        r#"{"emoji":":heart:","emoji_url":"https://fixture.invalid/x"}"#,
        r#"{"emoji":":heart:","emoji":":star:"}"#,
        r#"{"emoji":":heart:","anonymous":true}"#,
    ] {
        let mut r = request(address);
        let RequestBody::Form(fields) = &mut r.body else {
            panic!()
        };
        fields[4].1 = "vote".into();
        fields[5].1 = options.into();
        assert!(
            matches!(
                transport.execute_list_column_create(r, || true).await,
                Err(ListColumnCreateFailure::NotRequested(_))
            ),
            "{options}"
        );
    }
    let mut r = request(address);
    let RequestBody::Form(fields) = &mut r.body else {
        panic!()
    };
    fields[4].1 = "vote".into();
    fields.remove(5);
    assert!(matches!(
        transport.execute_list_column_create(r, || true).await,
        Err(ListColumnCreateFailure::NotRequested(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err()
    );
}
