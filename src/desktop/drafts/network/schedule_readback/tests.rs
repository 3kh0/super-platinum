use super::*;
use serde_json::json;
use std::cell::Cell;
use super_platinum_core::drafts::{
    DraftKey,
    sync::{RemoteDraft, Scope, Snapshot},
};

struct Fixture {
    account: String,
    client: SlackClient,
    workspace: WorkspaceSession,
    cooldown: Mutex<u64>,
}
impl Fixture {
    fn reader(&self) -> ScheduleRead<'_> {
        ScheduleRead {
            account: &self.account,
            client: &self.client,
            workspace: &self.workspace,
            cooldown: &self.cooldown,
        }
    }
}
fn fixture() -> (Fixture, Operation, Value) {
    let f = Fixture {
        account: "00000000-0000-4000-8000-000000000041".into(),
        client: SlackClient::default(),
        workspace: WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Isolated".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-not-a-token".into(),
        },
        cooldown: Mutex::new(0),
    };
    let scope = Scope {
        account_id: f.account.clone(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("100.000001".into()),
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_list","style":"bullet","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Frozen styled schedule","style":{"bold":true}}]}]}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1","thread_ts":"100.000001","broadcast":false})],
    };
    let op = Operation {
        id: "00000000-0000-4000-8000-000000000042".into(),
        scope: scope.clone(),
        client_msg_id: "00000000-0000-4000-8000-000000000043".into(),
        local_revision: 2,
        kind: OperationKind::Schedule { post_at: 500 },
        snapshot: Some(snapshot.clone()),
        base: Some(RemoteDraft {
            scope,
            id: "DrKnown".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000043".into(),
            last_updated_ts: "100.000001".into(),
            snapshot: snapshot.clone(),
        }),
        consume_id: Some("00000000-0000-4000-8000-000000000044".into()),
    };
    let raw = json!({"ok":true,"draft":{"id":"DrKnown","client_msg_id":op.client_msg_id,"last_updated_ts":"101.000001","blocks":snapshot.blocks,"file_ids":[],"destinations":snapshot.destinations,"is_deleted":false,"is_sent":false,"date_scheduled":500,"is_from_composer":true,"team_id":"T1","user_id":"U1"}});
    (f, op, raw)
}

#[tokio::test]
async fn one_exact_info_request_after_scheduled_time_keeps_full_raw_rich_payload() {
    let (f, op, raw) = fixture();
    let calls = Cell::new(0);
    let result = f
        .reader()
        .check(&op, 1_800_000_000_000, |request| {
            calls.set(calls.get() + 1);
            let url = url::Url::parse(&request.url).unwrap();
            assert_eq!(url.host_str(), Some("fixture.slack.com"));
            assert_eq!(url.path(), "/api/drafts.info");
            assert_eq!(request.method, "POST");
            assert!(!request.retry_safe());
            let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
                panic!("form")
            };
            let fields: Vec<_> = fields.into_iter().filter(|(k, _)| k != "token").collect();
            assert_eq!(fields, vec![("draft_id".into(), "DrKnown".into())]);
            std::future::ready(Ok(raw.clone()))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert!(result.requested);
    assert_eq!(result.raw, Some(raw));
    let Some(Outcome::Scheduled { draft, post_at }) = result.outcome else {
        panic!("positive schedule required")
    };
    assert_eq!(post_at, 500);
    assert_eq!(Some(draft.snapshot), op.snapshot);
    assert_eq!(result.issue, None);
}

#[test]
fn exact_positive_identity_content_time_revision_and_lifecycle_are_all_required() {
    let (f, op, good) = fixture();
    for (field, value) in [
        ("id", json!("DrOther")),
        (
            "client_msg_id",
            json!("00000000-0000-4000-8000-000000000099"),
        ),
        ("last_updated_ts", json!("100.000001")),
        ("last_updated_ts", json!("99.999999")),
        ("last_updated_ts", json!("bad")),
        ("blocks", json!([])),
        ("file_ids", json!(["F1"])),
        ("destinations", json!([{"channel_id":"C2"}])),
        (
            "destinations",
            json!([{"channel_id":"C1","thread_ts":"100.000001","broadcast":true}]),
        ),
        ("date_scheduled", json!(0)),
        ("date_scheduled", json!(501)),
        ("team_id", json!("T2")),
        ("user_id", json!("U2")),
        ("is_deleted", json!(true)),
        ("is_sent", json!(true)),
        ("is_from_composer", json!(false)),
        ("attachments", json!([{"text":"unrepresented"}])),
        ("unknown_lifecycle", json!(false)),
    ] {
        let mut raw = good.clone();
        raw["draft"][field] = value;
        let result = decode_schedule(&op, &f.workspace, raw.clone());
        assert!(result.outcome.is_none(), "{field}");
        assert_eq!(result.raw, Some(raw));
    }
    for raw in [
        json!({"ok":false,"error":"draft_not_found"}),
        json!({"ok":true}),
        json!({"ok":true,"draft":null}),
        json!({"ok":false,"error":"invalid_auth","draft":good["draft"]}),
        json!({"ok":true,"error":"contradiction","draft":good["draft"]}),
    ] {
        assert!(decode_schedule(&op, &f.workspace, raw).outcome.is_none());
    }
    for field in [
        "id",
        "client_msg_id",
        "last_updated_ts",
        "blocks",
        "file_ids",
        "destinations",
        "date_scheduled",
        "is_deleted",
        "is_sent",
    ] {
        let mut raw = good.clone();
        raw["draft"].as_object_mut().unwrap().remove(field);
        assert!(
            decode_schedule(&op, &f.workspace, raw).outcome.is_none(),
            "missing {field}"
        );
    }
}

#[test]
fn preserve_existing_optional_composer_flag_and_numeric_string_time_semantics() {
    let (f, op, mut raw) = fixture();
    raw["draft"]
        .as_object_mut()
        .unwrap()
        .remove("is_from_composer");
    raw["draft"]["date_scheduled"] = json!("500");
    assert!(matches!(
        decode_schedule(&op, &f.workspace, raw).outcome,
        Some(Outcome::Scheduled { post_at: 500, .. })
    ));
}

#[tokio::test]
async fn unknown_id_foreign_scope_and_invalid_consume_never_dispatch() {
    let (f, op, _) = fixture();
    for field in 0..8 {
        let mut wrong = op.clone();
        match field {
            0 => wrong.base = None,
            1 => wrong.scope.account_id = "00000000-0000-4000-8000-000000000099".into(),
            2 => wrong.scope.key.team = "T2".into(),
            3 => wrong.consume_id = None,
            4 => wrong.kind = OperationKind::Update,
            5 => wrong.kind = OperationKind::Schedule { post_at: 0 },
            6 => wrong.snapshot.as_mut().unwrap().file_ids.push("F1".into()),
            _ => {
                wrong.base.as_mut().unwrap().snapshot.destinations[0]["unknown_routing"] =
                    json!(true)
            }
        }
        let result = f
            .reader()
            .check(&wrong, u64::MAX, |_| async { panic!("invalid read") })
            .await;
        assert!(!result.requested, "{field}");
        assert!(result.outcome.is_none());
    }
}

#[tokio::test]
async fn missing_result_or_transport_error_is_not_negative_proof_or_a_retry() {
    for api_error in [false, true] {
        let (f, op, _) = fixture();
        let calls = Cell::new(0);
        let result = f
            .reader()
            .check(&op, 900_000, |_| {
                calls.set(calls.get() + 1);
                std::future::ready(if api_error {
                    Ok(json!({"ok":false,"error":"draft_not_found"}))
                } else {
                    Err(Error::Transport("synthetic lost read".into()))
                })
            })
            .await;
        assert_eq!(calls.get(), 1);
        assert!(result.outcome.is_none());
        assert!(result.requested);
    }
}

#[tokio::test]
async fn rate_limit_is_retained_and_shared_cooldown_blocks_the_next_explicit_read() {
    let (f, op, _) = fixture();
    let calls = Cell::new(0);
    let result = f
        .reader()
        .check(&op, 900_000, |_| {
            calls.set(calls.get() + 1);
            std::future::ready(Err(Error::RateLimited {
                retry_after_secs: Some(120),
            }))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert!(result.outcome.is_none());
    assert_eq!(result.issue, Some(Issue::RateLimited));
    let deadline = result.cooldown_until_ms.unwrap();
    assert!(deadline >= 1_020_000);
    let blocked = f
        .reader()
        .check(&op, 900_001, |_| async { panic!("cooldown") })
        .await;
    assert!(!blocked.requested);
    assert_eq!(blocked.cooldown_until_ms, Some(deadline));
}
