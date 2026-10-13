use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}

fn fields(request: PreparedRequest, endpoint: &str) -> BTreeMap<String, String> {
    assert_eq!(request.method, "POST");
    assert!(
        request
            .url
            .starts_with(&format!("https://fixture.slack.com/api/{endpoint}?"))
    );
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form expected")
    };
    fields
        .into_iter()
        .filter(|(key, _)| key != "token")
        .collect()
}

#[test]
fn requests_use_exact_view_protocol_and_single_state_envelope() {
    let client = SlackClient::default();
    let session = workspace();
    assert_eq!(
        fields(
            block_view_get(&client, &session, "V1").unwrap(),
            "views.get"
        ),
        BTreeMap::from([("view_id".into(), "V1".into())])
    );
    let state = json!({"values":{"input":{"note":{"type":"plain_text_input","value":"a&b"}}}});
    let request = fields(
        block_view_submit(&client, &session, "V1", "web-1", &state).unwrap(),
        "views.submit",
    );
    assert_eq!(request.len(), 3);
    assert_eq!(request["client_token"], "web-1");
    assert_eq!(request["view_id"], "V1");
    assert_eq!(
        serde_json::from_str::<Value>(&request["state"]).unwrap(),
        state
    );
    let no_root = fields(
        block_view_close(&client, &session, "V1", "web-2", None).unwrap(),
        "views.close",
    );
    assert_eq!(no_root.len(), 2);
    assert!(!no_root.contains_key("root_view_id"));
    let root = fields(
        block_view_close(&client, &session, "V2", "web-2", Some("V1")).unwrap(),
        "views.close",
    );
    assert_eq!(root["root_view_id"], "V1");
    assert_eq!(root.len(), 3);
}

#[test]
fn requests_reject_invalid_identity_and_noncanonical_state() {
    let c = SlackClient::default();
    let w = workspace();
    for id in ["", " V1", "V1\n"] {
        assert!(block_view_get(&c, &w, id).is_err());
        assert!(block_view_close(&c, &w, "V1", "web-1", Some(id)).is_err());
        assert!(block_view_submit(&c, &w, "V1", id, &json!({"values":{}})).is_err());
    }
    for state in [
        json!({}),
        json!({"values":[]}),
        json!({"values":{},"unknown":true}),
        json!([]),
    ] {
        assert!(block_view_submit(&c, &w, "V1", "web-1", &state).is_err());
    }
}

#[test]
fn submit_decodes_known_actions_and_never_accepts_unknown_actions() {
    for (name, expected) in [
        (
            "update",
            BlockViewResponseAction::Update(json!({"id":"V1","blocks":[]})),
        ),
        (
            "push",
            BlockViewResponseAction::Push(json!({"id":"V1","blocks":[]})),
        ),
        ("clear", BlockViewResponseAction::Clear),
        ("", BlockViewResponseAction::Absent),
    ] {
        let result=decode_block_view_submit(&json!({"ok":true,"response_action":name,"view":{"id":"V1","blocks":[]},"toast_message":"Done"})).unwrap();
        let BlockViewResponse::Accepted(submission) = result else {
            panic!("accepted")
        };
        assert!(submission.action == expected);
        assert_eq!(submission.toast_message.as_deref(), Some("Done"));
    }
    assert!(matches!(
        decode_block_view_submit(&json!({"ok":true})).unwrap(),
        BlockViewResponse::Accepted(BlockViewSubmission {
            action: BlockViewResponseAction::Absent,
            ..
        })
    ));
    for value in [
        json!({"ok":true,"response_action":"future"}),
        json!({"ok":true,"response_action":"update"}),
        json!({"ok":true,"response_action":"push","view":{}}),
        json!({"ok":true,"response_action":true}),
        json!({"ok":true,"toast_message":{}}),
    ] {
        assert!(decode_block_view_submit(&value).is_err());
    }
}

#[test]
fn rejections_keep_field_and_view_feedback_without_debug_content() {
    let raw = json!({"ok":false,"error":"validation_failed","message":"Sensitive note",
        "errors":{"note":"Fix <this>"},"view_error":"Try later","secret":"discard"});
    let result = decode_block_view_submit(&raw).unwrap();
    let BlockViewResponse::Rejected(error) = &result else {
        panic!("rejected")
    };
    assert_eq!(error.error, "validation_failed");
    assert_eq!(error.message.as_deref(), Some("Sensitive note"));
    assert_eq!(error.errors["note"], "Fix <this>");
    assert_eq!(error.view_error.as_deref(), Some("Try later"));
    let debug = format!("{result:?}");
    for text in ["Sensitive note", "Fix <this>", "Try later", "discard"] {
        assert!(!debug.contains(text));
    }
    for raw in [
        json!({"ok":false,"error":"failed","errors":[]}),
        json!({"ok":false,"error":"failed","errors":{"input":{}}}),
        json!({"ok":false,"error":"failed","view_error":[]}),
    ] {
        assert!(decode_block_view_submit(&raw).is_err());
    }
}

#[test]
fn get_and_close_require_explicit_acknowledgement_and_get_requires_view_identity() {
    let raw = json!({"id":"V1","private_metadata":"keep privately","blocks":[]});
    assert_eq!(
        decode_block_view_get(&json!({"ok":true,"view":raw})).unwrap(),
        BlockViewResponse::Accepted(raw)
    );
    assert_eq!(
        decode_block_view_close(&json!({"ok":true})).unwrap(),
        BlockViewResponse::Accepted(())
    );
    for value in [
        json!({}),
        json!({"ok":"true"}),
        json!({"ok":false}),
        json!({"ok":false,"error":0}),
    ] {
        assert!(decode_block_view_get(&value).is_err());
        assert!(decode_block_view_close(&value).is_err());
    }
    for value in [
        json!({"ok":true}),
        json!({"ok":true,"view":[]}),
        json!({"ok":true,"view":{"id":" "}}),
    ] {
        assert!(decode_block_view_get(&value).is_err());
    }
}
