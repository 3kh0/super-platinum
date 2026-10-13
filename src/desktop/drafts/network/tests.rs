use super::*;
use serde_json::json;
use std::cell::Cell;
use super_platinum_core::drafts::{
    DraftKey,
    sync::{OperationKind, RemoteDraft, Scope, Snapshot},
};

fn new_id() -> String {
    static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(100);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn binding() -> Binding {
    Binding {
        account: "00000000-0000-4000-8000-000000000041".into(),
        epoch: 7,
        transport: Arc::new(Transport::new("test-only-cookie").unwrap()),
        client: SlackClient::default(),
        workspace: WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://fixture.slack.com".into(),
            token: "test-token".into(),
        },
        cooldown_until_ms: Arc::new(Mutex::new(0)),
    }
}
fn operation(kind: OperationKind) -> Operation {
    let scope = Scope {
        account_id: binding().account,
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("100.000001".into()),
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"日本語 🚀\nKeep *literal*","style":{"bold":true}}]}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1","thread_ts":"100.000001","broadcast":false})],
    };
    let client_msg_id = new_id();
    Operation {
        id: new_id(),
        scope: scope.clone(),
        client_msg_id: client_msg_id.clone(),
        local_revision: 3,
        kind,
        snapshot: (kind != OperationKind::Delete).then_some(snapshot.clone()),
        base: (kind != OperationKind::Create).then_some(RemoteDraft {
            scope,
            id: "Dr1".into(),
            client_msg_id,
            last_updated_ts: "100.000001".into(),
            snapshot,
        }),
        consume_id: matches!(kind, OperationKind::Send | OperationKind::Schedule { .. })
            .then(|| new_id()),
    }
}
fn draft_response(operation: &Operation) -> Value {
    let snapshot = operation.snapshot.as_ref().unwrap();
    json!({"ok":true,"draft":{
        "id":"Dr1","client_msg_id":operation.client_msg_id,"last_updated_ts":"200.000001",
        "blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations,
        "is_deleted":false,"is_sent":false,"date_scheduled":match operation.kind { OperationKind::Schedule { post_at } => post_at, _ => 0 },
        "user_id":"U1","team_id":"T1"
    }})
}
fn send_response(operation: &Operation) -> Value {
    json!({"ok":true,"channel":"C1","ts":"200.000001","message":{
        "ts":"200.000001","channel":"C1","user":"U1","client_msg_id":operation.client_msg_id,"thread_ts":"100.000001"
    }})
}

#[tokio::test]
async fn one_prepared_operation_calls_fake_transport_once_and_keeps_raw_content() {
    let binding = binding();
    let op = operation(OperationKind::Create);
    let value = draft_response(&op);
    let calls = Cell::new(0);
    let attempt = binding
        .execute_with(&op, 1_800_000_000_000, |request| {
            calls.set(calls.get() + 1);
            assert!(!request.retry_safe());
            assert!(request.url.contains("drafts.create"));
            std::future::ready(Ok(value.clone()))
        })
        .await;
    assert_eq!(calls.get(), 1);
    let Outcome::Draft(remote) = attempt.outcome else {
        panic!("expected draft receipt")
    };
    assert_eq!(remote.scope, op.scope);
    assert_eq!(Some(remote.snapshot), op.snapshot);
    assert_eq!(attempt.raw, Some(value));
    assert_eq!(attempt.issue, None);
    assert!(attempt.dispatched);
}

#[tokio::test]
async fn account_and_team_mismatch_cannot_reach_transport() {
    let binding = binding();
    for account in [false, true] {
        let mut op = operation(OperationKind::Create);
        if account {
            op.scope.account_id = new_id();
        } else {
            op.scope.key.team = "T2".into();
        }
        let attempt = binding
            .execute_with(&op, 10, |_| async { panic!("wrong scope dispatched") })
            .await;
        assert!(!attempt.dispatched);
        assert_eq!(attempt.issue, Some(Issue::Blocked(Block::Scope)));
    }
}

#[tokio::test]
async fn rate_limit_survives_binding_clone_and_blocks_another_dispatch() {
    let binding = binding();
    let op = operation(OperationKind::Create);
    let attempt = binding
        .execute_with(&op, 1_000, |_| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(45),
            })
        })
        .await;
    assert!(matches!(attempt.outcome, Outcome::Uncertain));
    assert_eq!(attempt.issue, Some(Issue::RateLimited));
    let deadline = attempt.cooldown_until_ms.unwrap();
    assert!(deadline >= 46_000);
    binding.defer_until(1);
    let blocked = binding
        .clone()
        .execute_with(&op, 2_000, |_| async { panic!("cooldown dispatched") })
        .await;
    assert!(!blocked.dispatched);
    assert_eq!(blocked.cooldown_until_ms, Some(deadline));
}

#[test]
fn only_explicit_known_rejections_are_definite() {
    assert!(matches!(
        failure(&Error::Api("not_in_channel".into()), 0).outcome,
        Outcome::Rejected
    ));
    for error in [
        Error::Transport("response lost".into()),
        Error::Offline("reset".into()),
        Error::Api("internal_error".into()),
        Error::Api("fatal_error".into()),
        Error::Api("unknown_future_error".into()),
        Error::HttpStatus {
            status: 503,
            retry_after_secs: None,
        },
    ] {
        assert!(matches!(failure(&error, 0).outcome, Outcome::Uncertain));
    }
    assert_eq!(
        failure(
            &Error::HttpStatus {
                status: 429,
                retry_after_secs: None
            },
            10
        )
        .cooldown_until_ms,
        Some(30_010)
    );
}

#[test]
fn draft_receipt_requires_raw_identity_lifecycle_revision_and_destination() {
    let binding = binding();
    let op = operation(OperationKind::Update);
    let good = draft_response(&op);
    for (key, value) in [
        ("id", json!("other")),
        ("client_msg_id", json!(new_id())),
        ("last_updated_ts", json!("garbage")),
        ("is_sent", json!(true)),
        ("is_deleted", json!(true)),
        ("user_id", json!("U2")),
        ("team_id", json!("T2")),
        ("date_scheduled", json!(1000)),
        ("destinations", json!([{"channel_id":"C2"}])),
        ("blocks", json!(null)),
    ] {
        let mut malformed = good.clone();
        malformed["draft"][key] = value;
        assert!(
            matches!(
                decode::response(&op, &binding.workspace, &malformed).0,
                Outcome::Uncertain
            ),
            "{key}"
        );
    }
    for key in [
        "blocks",
        "file_ids",
        "is_sent",
        "is_deleted",
        "date_scheduled",
        "last_updated_ts",
    ] {
        let mut missing = good.clone();
        missing["draft"].as_object_mut().unwrap().remove(key);
        assert!(matches!(
            decode::response(&op, &binding.workspace, &missing).0,
            Outcome::Uncertain
        ));
    }
    let mut changed = good.clone();
    changed["draft"]["blocks"][0]["future_metadata"] = json!({"preserved":true});
    let (Outcome::Draft(remote), _) = decode::response(&op, &binding.workspace, &changed) else {
        panic!("raw remote conflict evidence")
    };
    assert_eq!(
        remote.snapshot.blocks[0]["future_metadata"]["preserved"],
        true
    );
    let mut attachments = good;
    attachments["draft"]["attachments"] = json!([{"text":"Do not discard"}]);
    assert_eq!(
        decode::response(&op, &binding.workspace, &attachments).1,
        Some(Issue::UnsupportedResponse)
    );
}

#[test]
fn schedule_and_delete_receipts_bind_the_exact_original_operation() {
    let binding = binding();
    let op = operation(OperationKind::Schedule {
        post_at: 1_800_001_000,
    });
    assert!(matches!(
        decode::response(&op, &binding.workspace, &draft_response(&op)).0,
        Outcome::Scheduled {
            post_at: 1_800_001_000,
            ..
        }
    ));
    let mut wrong = draft_response(&op);
    wrong["draft"]["date_scheduled"] = json!(1_800_001_001);
    assert!(matches!(
        decode::response(&op, &binding.workspace, &wrong).0,
        Outcome::Uncertain
    ));
    let op = operation(OperationKind::Delete);
    assert!(
        matches!(decode::response(&op, &binding.workspace, &json!({"ok":true})).0, Outcome::Deleted { draft_id } if draft_id == "Dr1")
    );
    assert!(matches!(
        decode::response(
            &op,
            &binding.workspace,
            &json!({"ok":true,"draft_id":"other"})
        )
        .0,
        Outcome::Uncertain
    ));
}

#[test]
fn sent_receipt_requires_exact_sender_uuid_channel_thread_and_valid_timestamp() {
    let binding = binding();
    let op = operation(OperationKind::Send);
    let good = send_response(&op);
    assert!(matches!(
        decode::response(&op, &binding.workspace, &good).0,
        Outcome::Sent { .. }
    ));
    for (key, value) in [
        ("user", json!("U2")),
        ("client_msg_id", json!(new_id())),
        ("channel", json!("C2")),
        ("thread_ts", json!("101.000001")),
        ("ts", json!("200.000002")),
    ] {
        let mut wrong = good.clone();
        wrong["message"][key] = value;
        assert!(matches!(
            decode::response(&op, &binding.workspace, &wrong).0,
            Outcome::Uncertain
        ));
    }
    for flag in [Value::Null, json!(false), json!("true")] {
        let mut wrong = good.clone();
        wrong["ok"] = flag;
        assert!(matches!(
            decode::response(&op, &binding.workspace, &wrong).0,
            Outcome::Uncertain
        ));
    }
    let mut wrong = good;
    wrong["ts"] = json!("200.1");
    wrong["message"]["ts"] = json!("200.1");
    assert!(matches!(
        decode::response(&op, &binding.workspace, &wrong).0,
        Outcome::Uncertain
    ));
}

#[test]
fn sparse_file_send_response_stays_uncertain_until_exact_file_receipt() {
    let binding = binding();
    let mut op = operation(OperationKind::Send);
    op.snapshot.as_mut().unwrap().file_ids = vec!["F1".into(), "F2".into()];
    let sparse = json!({"ok":true,"file_msg_ts":"200.000001"});
    assert!(matches!(
        decode::response(&op, &binding.workspace, &sparse).0,
        Outcome::Uncertain
    ));
    let exact = json!({"ok":true,"file_msg_ts":"200.000001","files":[{"id":"F2"},{"id":"F1"}]});
    assert!(matches!(
        decode::response(&op, &binding.workspace, &exact).0,
        Outcome::Sent { .. }
    ));
}

#[test]
fn captured_session_rejects_replacement_but_response_decode_remains_original_scope() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let mut binding = binding();
    let team = shell.core.active_team.clone().unwrap();
    binding.workspace.team_id = team.clone();
    binding.workspace.user_id = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.active_account = Some(binding.account.clone());
    shell.core.account_epoch = binding.epoch;
    shell.core.transport = Some(binding.transport.clone());
    shell.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "test-only-cookie".into(),
        workspaces: [(team.clone(), binding.workspace.clone())]
            .into_iter()
            .collect(),
    });
    let captured = Binding::capture(&shell, &team).unwrap();
    assert!(captured.current(&shell));
    shell.core.active_account = Some(new_id());
    assert!(!captured.current(&shell));
    shell.core.active_account = Some(binding.account);
    shell.core.account_epoch += 1;
    assert!(!captured.current(&shell));
    shell.core.account_epoch = captured.epoch;
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .user_id = "other-user".into();
    assert!(matches!(
        Binding::capture(&shell, &team),
        Err(Block::Session)
    ));
}

#[tokio::test]
async fn delivery_check_reads_only_one_bounded_original_channel_or_thread_page() {
    for thread in [false, true] {
        let binding = binding();
        let mut op = operation(OperationKind::Send);
        if !thread {
            op.scope.key.thread = None;
            op.snapshot.as_mut().unwrap().destinations = vec![json!({"channel_id":"C1"})];
            let base = op.base.as_mut().unwrap();
            base.scope = op.scope.clone();
            base.snapshot = op.snapshot.clone().unwrap();
        }
        let mut message = send_response(&op)["message"].clone();
        if !thread {
            message.as_object_mut().unwrap().remove("thread_ts");
        }
        let calls = Cell::new(0);
        let readback = binding
            .check_send_with(&op, 1_000, |request| {
                calls.set(calls.get() + 1);
                assert!(request.retry_safe()); // Transport's retry is read-only.
                assert!(request.url.contains(if thread {
                    "conversations.replies"
                } else {
                    "conversations.history"
                }));
                let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body
                else {
                    panic!("form")
                };
                assert!(fields.contains(&("limit".into(), "100".into())));
                assert!(fields.contains(&("channel".into(), "C1".into())));
                std::future::ready(Ok(json!({"ok":true,"messages":[message],"has_more":false})))
            })
            .await;
        assert_eq!(calls.get(), 1);
        let Some(Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        }) = readback.outcome
        else {
            panic!("exact UUID receipt")
        };
        assert_eq!(client_msg_id, op.client_msg_id);
        assert_eq!(draft_id.as_deref(), Some("Dr1"));
        assert_eq!(ts, "200.000001");
        assert!(!readback.has_more);
        assert!(readback.requested);
        assert!(readback.raw.is_some());
    }
}

#[test]
fn delivery_check_requires_uuid_sender_channel_thread_and_unique_live_match() {
    let binding = binding();
    let op = operation(OperationKind::Send);
    let good = send_response(&op)["message"].clone();
    for (key, value) in [
        ("user", json!("other-user")),
        ("client_msg_id", json!(new_id())),
        ("channel", json!("other-channel")),
        ("thread_ts", json!("101.000001")),
        ("ts", json!("bad")),
        ("is_deleted", json!(true)),
        ("subtype", json!("tombstone")),
    ] {
        let mut wrong = good.clone();
        wrong[key] = value;
        // Matching text is deliberately irrelevant to delivery identity.
        wrong["text"] = json!("日本語 🚀");
        let result = readback::decode_readback(
            &op,
            &binding.workspace,
            json!({"ok":true,"messages":[wrong],"has_more":false}),
        );
        assert!(result.outcome.is_none(), "{key}");
    }
    for messages in [json!([]), json!([good.clone(), good.clone()])] {
        let result = readback::decode_readback(
            &op,
            &binding.workspace,
            json!({"ok":true,"messages":messages,"has_more":true}),
        );
        assert!(result.outcome.is_none());
        assert!(result.has_more);
    }
    let result = readback::decode_readback(&op, &binding.workspace, json!({"messages":[good]}));
    assert!(result.outcome.is_none());
    assert_eq!(result.issue, Some(Issue::MalformedResponse));
}

#[tokio::test]
async fn readback_cooldown_and_scope_blocks_do_not_issue_any_request_or_confirm_rejection() {
    let binding = binding();
    let op = operation(OperationKind::Send);
    let result = binding
        .check_send_with(&op, 1_000, |_| async {
            Err(Error::HttpStatus {
                status: 429,
                retry_after_secs: Some(60),
            })
        })
        .await;
    assert!(result.outcome.is_none());
    assert_eq!(result.issue, Some(Issue::RateLimited));
    assert!(result.cooldown_until_ms.unwrap() >= 61_000);
    let result = binding
        .check_send_with(&op, 2_000, |_| async {
            panic!("cooldown must prevent read")
        })
        .await;
    assert!(!result.requested);
    let binding = super::tests::binding();
    let mut wrong = op.clone();
    wrong.scope.account_id = new_id();
    let result = binding
        .check_send_with(&wrong, 2_000, |_| async {
            panic!("account must prevent read")
        })
        .await;
    assert_eq!(result.issue, Some(Issue::Blocked(Block::Scope)));
    assert!(!result.requested);
    let result = binding
        .check_send_with(&op, 2_000, |_| async {
            Err(Error::Api("not_in_channel".into()))
        })
        .await;
    assert!(result.outcome.is_none());
    assert_eq!(result.issue, Some(Issue::Uncertain));
    let create = operation(OperationKind::Create);
    let result = binding
        .check_send_with(&create, 2_000, |_| async {
            panic!("non-send must prevent read")
        })
        .await;
    assert!(!result.requested);
}

#[test]
fn definite_offline_preflight_is_not_reported_as_dispatched_or_uncertain() {
    let attempt = failure(&Error::Api("draft_mutation_not_sent_offline".into()), 1000);
    assert!(!attempt.dispatched);
    assert!(matches!(attempt.outcome, Outcome::Rejected));
    assert_eq!(attempt.issue, Some(Issue::Blocked(Block::Offline)));
    assert!(attempt.cooldown_until_ms.is_none());
    assert!(matches!(
        failure(&Error::Transport("lost response".into()), 1000).outcome,
        Outcome::Uncertain
    ));
}
