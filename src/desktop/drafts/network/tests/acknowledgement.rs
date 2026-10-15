use super::*;

#[tokio::test]
async fn server_offline_sentinel_cannot_claim_transport_predispatch() {
    let binding = binding();
    let op = operation(OperationKind::Create);
    let raw = json!({"ok":false,"error":"draft_mutation_not_sent_offline"});
    let calls = Cell::new(0);
    let result = binding
        .execute_with(&op, 1000, |_| {
            calls.set(calls.get() + 1);
            std::future::ready(Ok(raw.clone()))
        })
        .await;
    assert_eq!(calls.get(), 1);
    assert!(result.dispatched);
    assert!(matches!(result.outcome, Outcome::Uncertain));
    assert_eq!(result.issue, Some(Issue::Uncertain));
    assert_eq!(result.raw, Some(raw));
    let trusted = binding
        .execute_with(&op, 1000, |_| async {
            Err(Error::Api("draft_mutation_not_sent_offline".into()))
        })
        .await;
    assert!(!trusted.dispatched);
    assert_eq!(trusted.issue, Some(Issue::Blocked(Block::Offline)));
    assert!(trusted.raw.is_none());
}

#[tokio::test]
async fn only_clean_known_raw_denial_can_release_an_ordinary_operation() {
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Delete,
    ] {
        let binding = binding();
        let op = operation(kind);
        let clean = json!({"ok":false,"error":"not_in_channel","needed":"chat:write",
            "provided":"identify","warning":"fixture","warnings":["fixture"],
            "response_metadata":{"warnings":["fixture"],"messages":["fixture"]}});
        let result = binding
            .execute_with(&op, 1000, |_| std::future::ready(Ok(clean.clone())))
            .await;
        assert!(result.dispatched);
        assert!(matches!(result.outcome, Outcome::Rejected));
        assert_eq!(result.raw, Some(clean));
        for (key, extra) in [
            ("draft", json!({"id":"Dr1"})),
            ("draft_id", json!("Dr1")),
            ("message", json!({"ts":"200.000001"})),
            ("is_deleted", json!(true)),
            ("future_outcome", Value::Null),
            ("needed", json!({"draft_id":"Dr1"})),
            ("response_metadata", json!({"draft_id":"Dr1"})),
            (
                "response_metadata",
                json!({"warnings":[{"draft_id":"Dr1"}]}),
            ),
        ] {
            let mut raw = json!({"ok":false,"error":"invalid_auth"});
            raw[key] = extra;
            let calls = Cell::new(0);
            let result = binding
                .execute_with(&op, 1000, |_| {
                    calls.set(calls.get() + 1);
                    std::future::ready(Ok(raw.clone()))
                })
                .await;
            assert_eq!(calls.get(), 1);
            assert!(result.dispatched);
            assert!(
                matches!(result.outcome, Outcome::Uncertain),
                "{kind:?} {key}"
            );
            assert_eq!(result.raw, Some(raw));
        }
    }
}

#[tokio::test]
async fn malformed_unknown_denials_and_rate_limits_retain_raw_uncertainty() {
    for raw in [
        json!({"ok":false}),
        json!({"ok":false,"error":null}),
        json!({"ok":false,"error":["invalid_auth"]}),
        json!({"ok":false,"error":"future_error"}),
        json!({"ok":"false","error":"invalid_auth"}),
        json!({"error":"invalid_auth"}),
        json!({"ok":false,"error":"ratelimited"}),
        json!({"ok":false,"error":"rate_limited","draft":{"id":"Dr1"}}),
    ] {
        let binding = binding();
        let op = operation(OperationKind::Update);
        let result = binding
            .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
            .await;
        assert!(result.dispatched);
        assert!(matches!(result.outcome, Outcome::Uncertain));
        assert_eq!(result.raw, Some(raw.clone()));
        if matches!(raw["error"].as_str(), Some("ratelimited" | "rate_limited")) {
            assert!(
                result
                    .cooldown_until_ms
                    .is_some_and(|until| until >= 31_000)
            );
        }
    }
}

#[tokio::test]
async fn contradictory_positive_envelopes_never_acknowledge_ordinary_mutations() {
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Delete,
    ] {
        let binding = binding();
        let op = operation(kind);
        let clean = if kind == OperationKind::Delete {
            json!({"ok":true})
        } else {
            draft_response(&op)
        };
        for (key, extra) in [
            ("error", json!("invalid_auth")),
            ("error", json!(false)),
            ("is_sent", json!(true)),
            ("is_deleted", json!(false)),
            ("message", json!({"ts":"200.000001"})),
            ("future_outcome", Value::Null),
            ("team_id", json!("T2")),
            ("user", json!("U2")),
            ("user_id", Value::Null),
            ("client_msg_id", json!(new_id())),
            ("draft_id", json!("Other")),
            ("response_metadata", json!({"draft":{"id":"Dr1"}})),
        ] {
            let mut raw = clean.clone();
            raw[key] = extra;
            let result = binding
                .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                .await;
            assert!(result.dispatched);
            assert!(
                matches!(result.outcome, Outcome::Uncertain),
                "{kind:?} {key}"
            );
            assert_eq!(result.raw, Some(raw));
        }
        if kind == OperationKind::Delete {
            let raw = json!({"ok":true,"draft":{"id":"Dr1","is_deleted":false}});
            let result = binding
                .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                .await;
            assert!(matches!(result.outcome, Outcome::Uncertain));
            assert_eq!(result.raw, Some(raw));
        }
    }
}

#[tokio::test]
async fn unknown_or_conflicting_draft_lifecycle_is_preserved_without_acknowledgement() {
    for kind in [OperationKind::Create, OperationKind::Update] {
        let binding = binding();
        let op = operation(kind);
        for (key, extra) in [
            ("is_from_composer", json!(true)),
            ("is_from_composer", json!("false")),
            ("is_from_composer", Value::Null),
            ("is_deleted", json!(true)),
            ("is_sent", json!(true)),
            ("date_scheduled", json!(1000)),
            ("future_lifecycle", json!(false)),
            ("date_stashed", json!(1)),
            ("file_permissions", json!({})),
            ("mpdm_name", json!("")),
            ("deleted_ts", json!("200.000001")),
            ("date_created", json!({"invalid":true})),
        ] {
            let mut raw = draft_response(&op);
            raw["draft"][key] = extra;
            let result = binding
                .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                .await;
            assert!(
                matches!(result.outcome, Outcome::Uncertain),
                "{kind:?} {key}"
            );
            assert_eq!(result.raw, Some(raw));
        }
    }
}

#[test]
fn clean_positive_ordinary_shapes_and_schedule_flag_remain_supported() {
    let binding = binding();
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Schedule {
            post_at: 1_800_001_000,
        },
    ] {
        let op = operation(kind);
        let mut raw = draft_response(&op);
        raw["error"] = Value::Null;
        raw["response_metadata"] = json!({"warnings":["fixture"]});
        raw["draft"]["is_from_composer"] = json!(matches!(kind, OperationKind::Schedule { .. }));
        raw["draft"]["date_created"] = json!(100);
        assert!(!matches!(
            decode::response(&op, &binding.workspace, &raw).0,
            Outcome::Uncertain
        ));
    }
    let op = operation(OperationKind::Delete);
    assert!(matches!(decode::response(&op, &binding.workspace, &json!({
        "ok":true,"draft_id":"Dr1","client_msg_id":op.client_msg_id,"team_id":"T1","user_id":"U1"
    })).0, Outcome::Deleted { .. }));
    let op = operation(OperationKind::Send);
    let mut raw = send_response(&op);
    raw["error"] = json!("invalid_auth");
    assert!(matches!(
        decode::response(&op, &binding.workspace, &raw).0,
        Outcome::Uncertain
    ));
}

fn captured() -> (ShellState, Binding, String) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let mut source = binding();
    let team = shell.core.active_team.clone().unwrap();
    source.workspace.team_id = team.clone();
    source.workspace.user_id = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.active_account = Some(source.account.clone());
    shell.core.account_epoch = source.epoch;
    shell.core.transport = Some(source.transport.clone());
    shell.core.session = Some(super_platinum_core::config::Session {
        d_cookie: source.cookie,
        workspaces: [(team.clone(), source.workspace)].into_iter().collect(),
    });
    let binding = Binding::capture(&shell, &team).unwrap();
    (shell, binding, team)
}

#[test]
fn captured_binding_rejects_each_credential_route_and_workspace_identity_change() {
    for change in 0..13 {
        let (mut shell, binding, team) = captured();
        assert!(binding.current(&shell));
        match change {
            0 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-replacement"),
            1 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .url
                .push_str("/replacement"),
            2 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .enterprise_id = Some("E2".into())
            }
            3 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .team_id = "T2".into()
            }
            4 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .user_id = "U2".into()
            }
            5 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push_str("-replacement"),
            6 => shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "U2".into(),
            7 => shell.core.workspaces.get_mut(&team).unwrap().team_id = "T2".into(),
            8 => {
                shell.core.workspaces.remove(&team);
            }
            9 => {
                shell.core.transport =
                    Some(Arc::new(Transport::new("fixture-replacement").unwrap()))
            }
            10 => shell.core.active_account = Some(new_id()),
            11 => shell.core.account_epoch += 1,
            12 => shell.core.session = None,
            _ => unreachable!(),
        }
        assert!(!binding.current(&shell), "change {change}");
    }
}

#[test]
fn capture_requires_cookie_and_matching_workspace_identity_without_binding_navigation() {
    let (mut shell, binding, team) = captured();
    shell.core.active_team = Some("OTHER".into());
    assert!(binding.current(&shell));
    shell.core.session.as_mut().unwrap().d_cookie.clear();
    assert!(matches!(
        Binding::capture(&shell, &team),
        Err(Block::Session)
    ));
    shell.core.session.as_mut().unwrap().d_cookie = "fixture-cookie".into();
    shell.core.workspaces.get_mut(&team).unwrap().team_id = "OTHER".into();
    assert!(matches!(
        Binding::capture(&shell, &team),
        Err(Block::Session)
    ));
}
