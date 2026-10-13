use super::*;
use crate::drafts::{DraftKey, sync::RemoteDraft};
use crate::slack::client::RequestBody;
use serde_json::json;

fn operation(kind: OperationKind, remote: bool) -> Operation {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("123.456".into()),
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"日本語 🚀\nKeep *literal*","style":{"bold":true}}]}]}),
        ],
        file_ids: vec!["F1".into(), "F2".into()],
        destinations: vec![json!({"channel_id":"C1","thread_ts":"123.456","broadcast":true})],
    };
    let client_msg_id = uuid::Uuid::new_v4().to_string();
    Operation {
        id: uuid::Uuid::new_v4().to_string(),
        scope: scope.clone(),
        client_msg_id: client_msg_id.clone(),
        local_revision: 9,
        kind,
        snapshot: (kind != OperationKind::Delete).then_some(snapshot.clone()),
        base: remote.then_some(RemoteDraft {
            scope,
            id: "Dr1".into(),
            client_msg_id,
            last_updated_ts: "1234.567".into(),
            snapshot,
        }),
        consume_id: None,
    }
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "test-token".into(),
    }
}
fn build(op: &Operation) -> DraftOperationRequest {
    let result =
        prepare_draft_operation(&SlackClient::default(), &workspace(), op, 1_800_000_000_123)
            .unwrap();
    assert!(!result.request.retry_safe());
    assert_eq!(result.identity.operation_id, op.id);
    assert_eq!(result.identity.scope, op.scope);
    assert_eq!(result.identity.client_msg_id, op.client_msg_id);
    assert_eq!(result.identity.kind, op.kind);
    assert_eq!(result.identity.consume_id, op.consume_id);
    result
}
fn fields(request: &DraftOperationRequest) -> BTreeMap<&str, &str> {
    let RequestBody::Form(fields) = &request.request.body else {
        panic!("expected form")
    };
    fields
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

#[test]
fn ordinary_create_and_update_preserve_content_without_scheduling() {
    for (kind, remote, method) in [
        (OperationKind::Create, false, "drafts.create"),
        (OperationKind::Update, true, "drafts.update"),
    ] {
        let op = operation(kind, remote);
        let request = build(&op);
        assert!(request.request.url.contains(method));
        let fields = fields(&request);
        assert_eq!(fields["is_from_composer"], "false");
        assert!(!fields.contains_key("date_scheduled"));
        let snapshot = op.snapshot.as_ref().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(fields["blocks"]).unwrap(),
            json!(snapshot.blocks)
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(fields["destinations"]).unwrap(),
            json!(snapshot.destinations)
        );
        assert_eq!(fields["file_ids"], "[\"F1\",\"F2\"]");
        if remote {
            assert_eq!(fields["draft_id"], "Dr1");
            assert_eq!(fields["client_last_updated_ts"], "1800000000123");
            assert!(!fields.contains_key("client_msg_id"));
        } else {
            assert_eq!(fields["client_msg_id"], op.client_msg_id);
            assert!(!fields.contains_key("client_last_updated_ts"));
        }
    }
}

#[test]
fn delete_preserves_files_and_uses_supplied_milliseconds() {
    let op = operation(OperationKind::Delete, true);
    let request = build(&op);
    assert!(request.request.url.contains("drafts.delete"));
    let fields = fields(&request);
    assert_eq!(fields["draft_id"], "Dr1");
    assert_eq!(fields["skip_file_deletion"], "true");
    assert_eq!(fields["client_last_updated_ts"], "1800000000123");
    assert!(!fields.contains_key("blocks"));
    let rebuilt = build(&op);
    assert_eq!(request.request.body, rebuilt.request.body);
    assert_eq!(request.identity, rebuilt.identity);
}

#[test]
fn direct_and_draft_aware_send_retain_identity_and_correct_broadcast_field() {
    for remote in [false, true] {
        for files in [false, true] {
            let mut op = operation(OperationKind::Send, remote);
            op.consume_id = Some(uuid::Uuid::new_v4().to_string());
            if !files {
                op.snapshot.as_mut().unwrap().file_ids.clear();
            }
            // Frozen consumption can include an edit newer than its remote base.
            op.snapshot.as_mut().unwrap().blocks[0]["new_metadata"] = json!("preserved");
            let request = build(&op);
            assert!(request.request.url.contains(if files {
                "files.share"
            } else {
                "chat.postMessage"
            }));
            let fields = fields(&request);
            assert_eq!(fields.get("draft_id").copied(), remote.then_some("Dr1"));
            assert_eq!(fields["client_msg_id"], op.client_msg_id);
            assert_eq!(fields["thread_ts"], "123.456");
            assert_eq!(
                fields[if files {
                    "broadcast"
                } else {
                    "reply_broadcast"
                }],
                "true"
            );
            assert!(!fields.contains_key(if files {
                "reply_broadcast"
            } else {
                "broadcast"
            }));
            assert!(fields["blocks"].contains("preserved"));
            assert!(!fields.contains_key("consume_id"));
            assert!(!fields.contains_key("operation_id"));
        }
    }
}

#[test]
fn schedule_binds_known_draft_or_creates_with_the_frozen_identity() {
    for remote in [false, true] {
        let op = operation(
            OperationKind::Schedule {
                post_at: 1_800_100_000,
            },
            remote,
        );
        let request = build(&op);
        assert!(request.request.url.contains(if remote {
            "drafts.update"
        } else {
            "drafts.create"
        }));
        let fields = fields(&request);
        assert_eq!(fields["is_from_composer"], "true");
        assert_eq!(fields["date_scheduled"], "1800100000");
        assert_eq!(fields.get("draft_id").copied(), remote.then_some("Dr1"));
        assert_eq!(fields["file_ids"], "[\"F1\",\"F2\"]");
    }
}

#[test]
fn mismatched_scope_identity_base_and_unknown_routing_fail_before_dispatch() {
    let valid = operation(OperationKind::Update, true);
    let mut invalid = vec![];
    let mut op = valid.clone();
    op.scope.key.team = "T2".into();
    invalid.push((op, DraftRequestError::InvalidScope));
    let mut op = valid.clone();
    op.scope.account_id.clear();
    invalid.push((op, DraftRequestError::InvalidScope));
    let mut op = valid.clone();
    op.id.clear();
    invalid.push((op, DraftRequestError::InvalidIdentity));
    let mut op = valid.clone();
    op.base.as_mut().unwrap().client_msg_id = uuid::Uuid::new_v4().to_string();
    invalid.push((op, DraftRequestError::InvalidBase));
    let mut op = valid.clone();
    op.base = None;
    invalid.push((op, DraftRequestError::InvalidBase));
    let mut op = valid.clone();
    op.snapshot = None;
    invalid.push((op, DraftRequestError::InvalidSnapshot));
    let mut op = valid.clone();
    op.snapshot.as_mut().unwrap().destinations[0]["user_ids"] = json!(["U2"]);
    invalid.push((op, DraftRequestError::UnsupportedDestination));
    let mut op = valid.clone();
    op.snapshot.as_mut().unwrap().file_ids = vec!["F1,F2".into()];
    invalid.push((op, DraftRequestError::InvalidSnapshot));
    for (op, expected) in invalid {
        assert_eq!(
            prepare_draft_operation(&SlackClient::default(), &workspace(), &op, 1).unwrap_err(),
            expected
        );
    }
    assert_eq!(
        prepare_draft_operation(&SlackClient::default(), &workspace(), &valid, 0).unwrap_err(),
        DraftRequestError::InvalidTime
    );
    let op = operation(OperationKind::Schedule { post_at: 0 }, false);
    assert_eq!(
        prepare_draft_operation(&SlackClient::default(), &workspace(), &op, 1).unwrap_err(),
        DraftRequestError::InvalidTime
    );
}
