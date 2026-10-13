use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

fn fixture() -> (
    SlackClient,
    WorkspaceSession,
    Document,
    RecordsPage,
    PersonalTaskLists,
) {
    (
        SlackClient::default(),
        WorkspaceSession {
            team_id: "T_TEST".into(),
            enterprise_id: None,
            user_id: "U_TEST".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic-token".into(),
        },
        Document::parse(
            json!({"id":"F_LIST","mode":"list","access":"write","is_locked":false,
        "list_metadata":{"schema":[],"views":[]}}),
        )
        .unwrap(),
        RecordsPage::parse(json!({"ok":true,"records":[{"id":"R_ITEM","fields":[]}]})).unwrap(),
        PersonalTaskLists::from_response(
            &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap(),
    )
}

#[test]
fn exact_source_forms_have_no_hlc_retry_key_or_bulk_ids() {
    let (client, workspace, document, page, personal) = fixture();
    let intent =
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&personal)).unwrap();
    let delete = list_record_delete(&client, &workspace, &intent);
    assert!(!delete.retry_safe());
    assert!(delete.url.contains("/api/lists.records.delete?"));
    let RequestBody::Form(fields) = delete.body else {
        panic!("expected form")
    };
    assert_eq!(
        fields
            .into_iter()
            .filter(|(key, _)| key != "token")
            .collect::<Vec<_>>(),
        vec![
            ("list_id".into(), "F_LIST".into()),
            ("id".into(), "R_ITEM".into()),
            ("_x_reason".into(), "delete-record".into())
        ]
    );
    let info = list_record_info(&client, &workspace, "F_LIST", "R_ITEM");
    assert!(info.url.contains("/api/lists.records.info?"));
    let RequestBody::Form(fields) = info.body else {
        panic!("expected form")
    };
    assert_eq!(
        fields
            .into_iter()
            .filter(|(key, _)| key != "token")
            .collect::<Vec<_>>(),
        vec![
            ("list_id".into(), "F_LIST".into()),
            ("id".into(), "R_ITEM".into()),
            ("include_is_subscribed".into(), "true".into()),
            ("_x_reason".into(), "get-list-record".into())
        ]
    );
}

#[tokio::test]
async fn fresh_permission_rejection_and_known_offline_are_not_sent() {
    let (client, workspace, document, page, personal) = fixture();
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(crate::slack::transport::Health::Offline);
    let capture = || {
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&personal)).unwrap()
    };
    assert!(matches!(
        delete_list_record(
            &transport,
            &client,
            &workspace,
            &document,
            &page,
            None,
            Some(&personal),
            capture()
        )
        .await,
        Err(ListDeleteFailure::NotSent(Error::Api(_)))
    ));
    assert!(matches!(
        delete_list_record(
            &transport,
            &client,
            &workspace,
            &document,
            &page,
            Some("full"),
            Some(&personal),
            capture()
        )
        .await,
        Err(ListDeleteFailure::NotSent(Error::Offline(_)))
    ));
}

#[test]
fn undo_and_redo_update_same_id_without_recreation_or_field_payloads() {
    let (client, workspace, document, page, policy) = fixture();
    let original =
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&policy)).unwrap();
    for deleted in [false, true] {
        let response = if deleted {
            json!({"ok":true,"record":page.records[0].raw})
        } else {
            json!({"ok":false,"error":"record_deleted"})
        };
        let readback = ListRecordReadback::from_response("F_LIST", "R_ITEM", &response).unwrap();
        let intent = original
            .deletion_state_intent(
                &document,
                &page,
                &readback,
                deleted,
                Some("full"),
                Some(&policy),
            )
            .unwrap();
        let request = list_record_update_deletion(&client, &workspace, &intent);
        assert!(!request.retry_safe());
        assert!(request.url.contains("/api/lists.records.updateDeletion?"));
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(
            fields
                .into_iter()
                .filter(|(key, _)| key != "token")
                .collect::<Vec<_>>(),
            vec![
                ("list_id".into(), "F_LIST".into()),
                ("id".into(), "R_ITEM".into()),
                ("deleted".into(), deleted.to_string()),
                ("_x_reason".into(), "update-record-deletion-state".into())
            ]
        );
    }
}

#[tokio::test]
async fn undo_preflight_and_offline_failures_are_definitely_not_sent() {
    let (client, workspace, document, page, policy) = fixture();
    let original =
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&policy)).unwrap();
    let readback = ListRecordReadback::from_response(
        "F_LIST",
        "R_ITEM",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(crate::slack::transport::Health::Offline);
    for allow in [Some("full"), None] {
        let intent = original
            .deletion_state_intent(
                &document,
                &page,
                &readback,
                false,
                Some("full"),
                Some(&policy),
            )
            .unwrap();
        let result = update_list_record_deletion(
            &transport,
            &client,
            &workspace,
            &document,
            &page,
            &readback,
            allow,
            Some(&policy),
            intent,
        )
        .await;
        assert!(matches!(result, Err(ListDeleteFailure::NotSent(_))));
    }
}
