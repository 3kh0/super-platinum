use super::*;
use serde_json::json;
use std::cell::Cell;
use super_platinum_core::drafts::{
    DraftKey,
    sync::{RemoteDraft, Scope, Snapshot},
};

fn fixture() -> (Binding, Operation, Value) {
    let binding = Binding {
        account: "00000000-0000-4000-8000-000000000041".into(),
        epoch: 7,
        cookie: "fixture-cookie".into(),
        transport: Arc::new(Transport::new("fixture-cookie").unwrap()),
        client: SlackClient::default(),
        workspace: WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-token".into(),
        },
        cooldown_until_ms: Arc::new(Mutex::new(0)),
    };
    let scope = Scope {
        account_id: binding.account.clone(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Frozen update"}]}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    };
    let op = Operation {
        id: "00000000-0000-4000-8000-000000000042".into(),
        scope: scope.clone(),
        client_msg_id: "00000000-0000-4000-8000-000000000043".into(),
        local_revision: 2,
        kind: OperationKind::Update,
        snapshot: Some(snapshot.clone()),
        base: Some(RemoteDraft {
            scope,
            id: "Dr1".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000043".into(),
            last_updated_ts: "100.000001".into(),
            snapshot: snapshot.clone(),
        }),
        consume_id: None,
    };
    let raw = json!({"ok":true,"draft":{"id":"Dr1","client_msg_id":op.client_msg_id,"last_updated_ts":"100.000002","blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations,"is_deleted":false,"is_sent":false,"date_scheduled":0,"user_id":"U1","team_id":"T1"}});
    (binding, op, raw)
}

#[tokio::test]
async fn exact_newer_payload_uses_one_info_post_without_mutation_dispatch() {
    let (binding, op, raw) = fixture();
    let calls = Cell::new(0);
    let read = binding
        .check_update_with(&op, 1_800_000_000_000, |request| {
            calls.set(calls.get() + 1);
            assert_eq!(request.method, "POST");
            assert!(request.url.contains("/drafts.info"));
            assert!(!request.url.contains("drafts.update"));
            let super_platinum_core::slack::client::RequestBody::Form(fields) = &request.body
            else {
                panic!("form read expected")
            };
            assert!(
                fields
                    .iter()
                    .any(|(key, value)| key == "draft_id" && value == "Dr1")
            );
            std::future::ready(Ok(raw.clone()))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert!(matches!(read.outcome, Some(Outcome::Draft(_))));
    assert_eq!(read.raw, Some(raw));
    assert_eq!(read.issue, None);
}

#[test]
fn wrong_identity_version_payload_and_lifecycle_remain_unresolved() {
    let (binding, op, good) = fixture();
    for (field, value) in [
        ("id", json!("Dr2")),
        (
            "client_msg_id",
            json!("00000000-0000-4000-8000-000000000044"),
        ),
        ("last_updated_ts", json!("100.000001")),
        ("last_updated_ts", json!("99.999999")),
        ("last_updated_ts", json!("future")),
        ("blocks", json!([])),
        ("file_ids", json!(["F1"])),
        ("destinations", json!([{"channel_id":"C2"}])),
        ("team_id", json!("T2")),
        ("user_id", json!("U2")),
        ("is_sent", json!(true)),
        ("is_deleted", json!(true)),
        ("date_scheduled", json!(1)),
        ("is_from_composer", json!(true)),
        ("unknown_outcome", json!(false)),
    ] {
        let mut raw = good.clone();
        raw["draft"][field] = value;
        let read = decode_update(&op, &binding.workspace, raw.clone());
        assert!(read.outcome.is_none(), "{field}");
        assert_eq!(read.raw, Some(raw));
    }
    for raw in [
        json!({"ok":false,"error":"draft_not_found"}),
        json!({"ok":true,"draft":null}),
        json!({"ok":false,"error":"invalid_auth","draft":good["draft"]}),
    ] {
        assert!(
            decode_update(&op, &binding.workspace, raw)
                .outcome
                .is_none()
        );
    }
}

#[tokio::test]
async fn foreign_scope_unsupported_kind_and_cooldown_never_call_read() {
    let (binding, op, _) = fixture();
    let mut foreign = op.clone();
    foreign.scope.account_id.push('x');
    let mut create = op.clone();
    create.kind = OperationKind::Create;
    for invalid in [foreign, create] {
        let read = binding
            .check_update_with(&invalid, 100, |_| async {
                panic!("invalid read dispatched")
            })
            .await;
        assert!(!read.requested);
    }
    binding.defer_until(90000);
    let read = binding
        .check_update_with(&op, 100, |_| async { panic!("cooldown read dispatched") })
        .await;
    assert!(!read.requested);
    assert_eq!(read.cooldown_until_ms, Some(90000));
}

#[tokio::test]
async fn rate_limit_retains_unknown_update_and_monotonic_deadline() {
    let (binding, op, _) = fixture();
    let read = binding
        .check_update_with(&op, 100, |_| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(120),
            })
        })
        .await;
    assert!(read.outcome.is_none());
    assert_eq!(read.issue, Some(Issue::RateLimited));
    let deadline = read.cooldown_until_ms.unwrap();
    assert!(deadline >= 120100);
    let blocked = binding
        .clone()
        .check_update_with(&op, 101, |_| async { panic!("rate limit replay") })
        .await;
    assert!(!blocked.requested);
    assert_eq!(blocked.cooldown_until_ms, Some(deadline));
}
