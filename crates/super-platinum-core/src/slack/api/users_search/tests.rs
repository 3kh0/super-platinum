use super::*;
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    }
}

#[tokio::test]
async fn user_lookup_keeps_existing_name_email_request_and_service_order() {
    let users = fetch_users_search_with(&SlackClient::default(), &session(), "name@example.test".into(), |request| async move {
        assert!(url::Url::parse(&request.url).unwrap().path().ends_with("/users/search"));
        let crate::slack::client::RequestBody::Json(body) = request.body else {panic!("JSON required")};
        assert_eq!(body["query"], "name@example.test");
        assert_eq!(body["count"], 30);
        assert_eq!(body["search_email"], true);
        assert_eq!(body["enable_workspace_ranking"], true);
        assert_eq!(body["include_profile_only_users"], true);
        assert!(body.get("page").is_none() && body.get("sort").is_none());
        Ok(json!({"ok":true,"results":[{"id":"U_SECOND","profile":{"display_name":"Second"}},{"id":"U_FIRST","profile":{"display_name":"First"}}]}))
    }).await.unwrap();
    assert_eq!(
        users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
        ["U_SECOND", "U_FIRST"]
    );
}

#[tokio::test]
async fn malformed_user_lookup_is_not_a_successful_empty_list() {
    for value in [
        json!({"ok":true}),
        json!({"ok":true,"results":null}),
        json!({"ok":true,"results":{}}),
        json!({"ok":true,"results":[false]}),
        json!({"ok":"yes","results":[]}),
        json!({"ok":null,"results":[]}),
        json!({"ok":true,"failed_ids":false,"results":[]}),
    ] {
        assert!(
            fetch_users_search_with(
                &SlackClient::default(),
                &session(),
                "name".into(),
                |_| async { Ok(value) }
            )
            .await
            .is_err()
        );
    }
    assert!(
        fetch_users_search_with(
            &SlackClient::default(),
            &session(),
            "name".into(),
            |_| async { Ok(json!({"ok":true,"results":[]})) }
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn lookup_retains_api_and_rate_limit_failures_for_existing_retry_policy() {
    let error = fetch_users_search_with(
        &SlackClient::default(),
        &session(),
        "name".into(),
        |_| async { Ok(json!({"ok":false,"error":"invalid_auth","results":[]})) },
    )
    .await
    .unwrap_err();
    assert!(matches!(error,Error::Api(code) if code=="invalid_auth"));
    let error = fetch_users_search_with(
        &SlackClient::default(),
        &session(),
        "name".into(),
        |_| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(17),
            })
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::RateLimited {
            retry_after_secs: Some(17)
        }
    ));
}
