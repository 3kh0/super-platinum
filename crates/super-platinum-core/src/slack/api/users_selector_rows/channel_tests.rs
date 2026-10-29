use super::*;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, future::ready};

#[tokio::test]
async fn channel_selector_raw_budget_keeps_endpoint_identity_and_permissive_rows() {
    let build = |marker: Option<&str>| {
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
            enterprise_id: None,
        };
        let mut body = json!({"query":"literal query","count":24,"fuzz":1});
        if let Some(marker) = marker {
            body["marker"] = json!(marker);
        }
        SlackClient::default()
            .edge_json(&session, "channels/search", body)
            .map(Some)
            .map_err(|_| Error::Api("conversation_search_unavailable".into()))
    };
    let pages = RefCell::new(VecDeque::from([
        json!({"results":[],"next_marker":"empty"}),
        json!({"ok":"legacy ignored","failed_ids":[1],"results":vec![Value::Null;20],"next_marker":"next"}),
        json!({"results":(0..20).map(|n|json!({"id":format!("C{n}"),"is_channel":true})).collect::<Vec<_>>(),"next_marker":{"unused":true}}),
    ]));
    let markers = RefCell::new(Vec::new());
    let rows = fetch_channels_selector_rows_with(
        build,
        |request| {
            assert!(request.url.contains("/channels/search"));
            let super::super::super::client::RequestBody::Json(body) = request.body else {
                panic!("JSON")
            };
            assert_eq!(body["count"], 24);
            assert!(body.get("check_membership").is_none());
            markers.borrow_mut().push(body.get("marker").cloned());
            ready(Ok(Some(pages.borrow_mut().pop_front().unwrap())))
        },
        || true,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        *markers.borrow(),
        vec![None, Some(json!("empty")), Some(json!("next"))]
    );
    assert_eq!(rows.len(), 24);
    let channels = rows
        .into_iter()
        .filter_map(|row| serde_json::from_value::<Channel>(row).ok())
        .collect::<Vec<_>>();
    assert_eq!(
        channels.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["C0", "C1", "C2", "C3"]
    );
    for (invalid_marker, value) in [
        (false, json!({"results":null})),
        (true, json!({"results":[],"next_marker":true})),
    ] {
        let error =
            fetch_channels_selector_rows_with(build, |_| ready(Ok(Some(value.clone()))), || true)
                .await
                .unwrap_err();
        match (invalid_marker, error) {
            (false, Error::Api(message)) => assert_eq!(message, "conversation_search_invalid"),
            (true, Error::Transport(message)) => assert_eq!(
                message,
                "decode channels/search: unsupported continuation marker"
            ),
            (_, other) => panic!("unexpected {other:?}"),
        }
    }
}
