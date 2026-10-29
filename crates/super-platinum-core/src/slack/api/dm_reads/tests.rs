use super::*;
use serde_json::json;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U_SELF".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    }
}
fn fields(request: PreparedRequest, method: &str) -> BTreeMap<String, String> {
    assert!(request.url.contains(method));
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form required")
    };
    fields.into_iter().collect()
}
#[tokio::test]
async fn dm_feed_uses_existing_closed_conversation_paged_factory_and_typed_decoder() {
    let page=fetch_client_dms_with(&SlackClient::default(),&session(),100,Some("older-cursor".into()),|request|async move{
        let f=fields(request,"client.dms");assert_eq!(f["count"],"100");assert_eq!(f["cursor"],"older-cursor");
        assert_eq!(f["include_closed"],"true");assert_eq!(f["include_channel"],"true");assert_eq!(f["priority_mode"],"priority");
        Ok(json!({"ok":true,"dms":[{"id":"D_COLD","latest":"9.1","channel":{"id":"D_COLD","is_im":true,"is_open":false}}],"response_metadata":{"next_cursor":"next-page"}}))
    }).await.unwrap();
    assert_eq!(page.dms[0].id, "D_COLD");
    assert!(page.dms[0].channel.as_ref().unwrap().is_im);
    assert_eq!(
        page.dms[0].channel.as_ref().unwrap().extra["is_open"],
        false
    );
    assert_eq!(
        page.response_metadata.unwrap().next_cursor.as_deref(),
        Some("next-page")
    );
}
#[tokio::test]
async fn dm_feed_rejects_malformed_pages_and_preserves_actual_executor_retry_errors() {
    for value in [
        json!({"ok":true}),
        json!({"dms":null}),
        json!({"dms":{}}),
        json!({"dms":[false]}),
        json!({"dms":[],"response_metadata":{"next_cursor":false}}),
    ] {
        assert!(
            fetch_client_dms_with(&SlackClient::default(), &session(), 100, None, |_| async {
                Ok(value)
            })
            .await
            .is_err()
        );
    }
    let error = fetch_client_dms_with(&SlackClient::default(), &session(), 100, None, |_| async {
        Ok(json!({"ok":false,"error":"invalid_auth"}))
    })
    .await
    .unwrap_err();
    assert!(matches!(error,Error::Api(code) if code=="invalid_auth"));
    let error = fetch_client_dms_with(&SlackClient::default(), &session(), 100, None, |_| async {
        Err(Error::RateLimited {
            retry_after_secs: Some(17),
        })
    })
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::RateLimited {
            retry_after_secs: Some(17)
        }
    ));
    assert!(
        fetch_client_dms_with(&SlackClient::default(), &session(), 100, None, |_| async {
            Ok(json!({"ok":true,"dms":[]}))
        })
        .await
        .unwrap()
        .dms
        .is_empty()
    );
}
#[tokio::test]
async fn conversation_info_uses_existing_read_factory_and_keeps_closed_dm_metadata() {
    let info=fetch_conversation_info_with(&SlackClient::default(),&session(),"D_COLD".into(),|request|async move{
        let f=fields(request,"conversations.info");assert_eq!(f["channel"],"D_COLD");assert_eq!(f["include_num_members"],"false");
        Ok(json!({"ok":true,"channel":{"id":"D_COLD","is_im":true,"is_open":false,"user":"U_PEER"}}))
    }).await.unwrap();
    assert_eq!(info.id, "D_COLD");
    assert!(info.is_im);
    assert_eq!(info.extra["is_open"], false);
    for value in [
        json!({"ok":true}),
        json!({"ok":true,"channel":false}),
        json!({"ok":true,"channel":{"id":13}}),
    ] {
        assert!(
            fetch_conversation_info_with(
                &SlackClient::default(),
                &session(),
                "D_COLD".into(),
                |_| async { Ok(value) }
            )
            .await
            .is_err()
        );
    }
    let error = fetch_conversation_info_with(
        &SlackClient::default(),
        &session(),
        "D_COLD".into(),
        |_| async { Err(Error::Offline("synthetic".into())) },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Offline(_)));
}
