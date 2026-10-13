use super::*;
use serde_json::json;

fn scope() -> ModalScope {
    ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 7,
    }
}
fn event(value: Value) -> RawEvent {
    serde_json::from_value(value).unwrap()
}
fn open(token: &str, id: &str, previous: Option<&str>) -> RawEvent {
    event(json!({"type":"view_opened","view_id":id,"app_id":"A1",
        "view_type":"modal","client_token":token,"previous_view_id":previous,
        "timeout_range":500,"view":{"id":id,"type":"modal","app_id":"A1",
        "previous_view_id":previous,"blocks":[{"type":"input","unknown_future_field":{"x":1}}]}}))
}
fn update(id: &str) -> RawEvent {
    event(json!({"type":"view_updated","view_id":id,"app_id":"A1",
        "view":{"id":id,"type":"modal","app_id":"A1","hash":"opaque",
        "blocks":[{"type":"section","text":{"type":"plain_text","text":"Updated"}}]}}))
}
fn queued() -> ModalState {
    let mut state = ModalState::new(scope());
    assert!(state.enqueue(&scope(), "token".into(), "A1".into(), 100, 1_000));
    state
}

fn view(id: &str, previous: Option<&str>) -> Value {
    json!({"id":id,"type":"modal","app_id":"A1","previous_view_id":previous,
        "root_view_id":"V1","blocks":[],"opaque":{"keep":"value"}})
}

#[test]
fn hydration_requires_current_revision_and_preserves_exact_content() {
    let mut state = queued();
    let mut frame = open("token", "V1", None);
    frame.rest.remove("view");
    state.apply(&scope(), &frame, 101);
    let revision = state.revision();
    let payload = view("V1", None);
    assert!(state.hydrate_current(&scope(), "V1", revision, payload.clone()));
    assert_eq!(state.stack()[0].payload.as_ref(), Some(&payload));
    assert!(!state.stack()[0].needs_content);
    assert_eq!(state.token_count(), 1);
    assert_eq!(state.revision(), revision + 1);
    assert!(!state.hydrate_current(&scope(), "V1", revision, payload));
}

#[test]
fn hidden_parent_updates_keep_the_child_submission_revision_current() {
    for inline in [false, true] {
        let mut state = queued();
        state.apply(&scope(), &open("token", "V1", None), 101);
        state.apply(&scope(), &open("token", "V2", Some("V1")), 102);
        let requested = state.revision();
        let mut changed = update("V1");
        if !inline {
            changed.rest.remove("view");
        }
        assert_eq!(state.apply(&scope(), &changed, 103), ModalChange::Updated);
        assert_eq!(state.revision(), requested);
        assert_eq!(state.stack()[0].needs_content, !inline);
        if inline {
            assert_eq!(state.stack()[0].payload.as_ref().unwrap()["hash"], "opaque");
        }
        assert!(state.apply_submission(
            &scope(),
            "V2",
            requested,
            &BlockViewResponseAction::Absent
        ));
        assert_eq!(state.stack().last().unwrap().id, "V1");
        assert!(state.revision() > requested);
        assert_eq!(state.stack()[0].needs_content, !inline);
        assert!(!state.hydrate_current(&scope(), "V1", requested, view("V1", None)));
    }
}

#[test]
fn parent_update_does_not_invalidate_current_child_hydration_but_child_update_does() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    let mut child = open("token", "V2", Some("V1"));
    child.rest.remove("view");
    state.apply(&scope(), &child, 102);
    let requested = state.revision();
    state.apply(&scope(), &update("V1"), 103);
    assert!(state.hydrate_current(&scope(), "V2", requested, view("V2", Some("V1"))));
    let requested = state.revision();
    state.apply(&scope(), &update("V2"), 104);
    assert!(!state.hydrate_current(&scope(), "V2", requested, view("V2", Some("V1"))));
    assert!(!state.apply_submission(&scope(), "V2", requested, &BlockViewResponseAction::Clear));
}

#[test]
fn hydration_cannot_overwrite_realtime_updates_or_different_active_identity() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    let requested = state.revision();
    state.apply(&scope(), &update("V1"), 102);
    let before = state.stack().to_vec();
    assert!(!state.hydrate_current(&scope(), "V1", requested, view("V1", None)));
    let revision = state.revision();
    assert!(!state.hydrate_current(&scope(), "V2", revision, view("V1", None)));
    let wrong = ModalScope {
        account_epoch: 2,
        ..scope()
    };
    assert!(!state.hydrate_current(&wrong, "V1", revision, view("V1", None)));
    for (key, value) in [
        ("id", json!("V9")),
        ("type", json!("home")),
        ("app_id", json!("A2")),
        ("previous_view_id", json!("V0")),
        ("root_view_id", json!("V0")),
        ("blocks", json!({})),
    ] {
        let mut payload = view("V1", None);
        payload[key] = value;
        assert!(!state.hydrate_current(&scope(), "V1", revision, payload));
    }
    assert_eq!(state.stack(), before);
    assert_eq!(state.revision(), revision);
}

#[test]
fn submission_absent_pops_one_even_with_clear_on_close_and_clear_drops_all() {
    for clear in [false, true] {
        let mut state = queued();
        state.apply(&scope(), &open("token", "V1", None), 101);
        let mut child = open("token", "V2", Some("V1"));
        child.rest.get_mut("view").unwrap()["clear_on_close"] = json!(true);
        state.apply(&scope(), &child, 102);
        let revision = state.revision();
        let action = if clear {
            BlockViewResponseAction::Clear
        } else {
            BlockViewResponseAction::Absent
        };
        assert!(state.apply_submission(&scope(), "V2", revision, &action));
        assert_eq!(state.stack().len(), if clear { 0 } else { 1 });
        assert_eq!(state.token_count(), 0);
        assert_eq!(state.revision(), revision + 1);
        assert_eq!(state.apply(&scope(), &child, 103), ModalChange::Ignored);
        if !clear {
            let revision = state.revision();
            assert!(state.apply_submission(
                &scope(),
                "V1",
                revision,
                &BlockViewResponseAction::Absent
            ));
            assert!(state.stack().is_empty());
        }
    }
}

#[test]
fn submission_update_preserves_stack_and_tokens_but_rejects_stale_or_cross_app_content() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    state.apply(&scope(), &open("token", "V2", Some("V1")), 102);
    let revision = state.revision();
    let before = state.stack().to_vec();
    for (key, value) in [
        ("id", json!("V3")),
        ("type", json!("home")),
        ("app_id", json!("A2")),
        ("previous_view_id", json!("V9")),
        ("root_view_id", json!("V9")),
        ("blocks", Value::Null),
    ] {
        let mut payload = view("V2", Some("V1"));
        payload[key] = value;
        assert!(!state.apply_submission(
            &scope(),
            "V2",
            revision,
            &BlockViewResponseAction::Update(payload)
        ));
    }
    assert_eq!(state.stack(), before);
    assert_eq!(state.revision(), revision);
    let payload = view("V2", Some("V1"));
    assert!(state.apply_submission(
        &scope(),
        "V2",
        revision,
        &BlockViewResponseAction::Update(payload.clone())
    ));
    assert_eq!(state.stack().len(), 2);
    assert_eq!(state.token_count(), 1);
    assert_eq!(state.stack()[1].payload.as_ref(), Some(&payload));
    assert!(!state.apply_submission(&scope(), "V2", revision, &BlockViewResponseAction::Clear));
}

#[test]
fn submission_push_requires_explicit_current_parent_and_compatible_app_type() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    let revision = state.revision();
    let before = state.stack().to_vec();
    for (key, value) in [
        ("id", json!("V1")),
        ("type", json!("home")),
        ("app_id", json!("A2")),
        ("previous_view_id", Value::Null),
        ("previous_view_id", json!("OTHER")),
        ("root_view_id", json!("OTHER")),
        ("blocks", json!({})),
    ] {
        let mut payload = view("V2", Some("V1"));
        payload[key] = value;
        assert!(!state.apply_submission(
            &scope(),
            "V1",
            revision,
            &BlockViewResponseAction::Push(payload)
        ));
    }
    for key in ["app_id", "previous_view_id"] {
        let mut payload = view("V2", Some("V1"));
        payload.as_object_mut().unwrap().remove(key);
        assert!(!state.apply_submission(
            &scope(),
            "V1",
            revision,
            &BlockViewResponseAction::Push(payload)
        ));
    }
    assert_eq!(state.stack(), before);
    assert_eq!(state.revision(), revision);
    let payload = view("V2", Some("V1"));
    assert!(state.apply_submission(
        &scope(),
        "V1",
        revision,
        &BlockViewResponseAction::Push(payload.clone())
    ));
    assert_eq!(state.stack()[1].payload.as_ref(), Some(&payload));
    assert_eq!(state.stack()[1].previous_view_id.as_deref(), Some("V1"));
    assert_eq!(state.token_count(), 1);
    assert!(state.tokens[0].opened.contains(&"V2".into()));
    assert_eq!(
        state.apply(&scope(), &open("token", "V2", Some("V1")), 103),
        ModalChange::Ignored
    );
}

#[test]
fn async_submission_guards_scope_active_id_revision_and_stack_budget() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    let requested = state.revision();
    state.apply(&scope(), &open("token", "V2", Some("V1")), 102);
    assert!(!state.apply_submission(&scope(), "V1", requested, &BlockViewResponseAction::Clear));
    let revision = state.revision();
    for wrong in [
        ModalScope {
            account_epoch: 2,
            ..scope()
        },
        ModalScope {
            team: "T2".into(),
            ..scope()
        },
        ModalScope {
            realtime_generation: 8,
            ..scope()
        },
    ] {
        assert!(!state.apply_submission(&wrong, "V2", revision, &BlockViewResponseAction::Absent));
    }
    for i in 3..=STACK_LIMIT {
        let parent = format!("V{}", i - 1);
        let payload = view(&format!("V{i}"), Some(&parent));
        let revision = state.revision();
        assert!(state.apply_submission(
            &scope(),
            &parent,
            revision,
            &BlockViewResponseAction::Push(payload)
        ));
    }
    let revision = state.revision();
    let parent = format!("V{STACK_LIMIT}");
    assert!(!state.apply_submission(
        &scope(),
        &parent,
        revision,
        &BlockViewResponseAction::Push(view("OVER", Some(&parent)))
    ));
    assert_eq!(state.stack().len(), STACK_LIMIT);
    assert_eq!(state.revision(), revision);
}

#[test]
fn correlated_open_preserves_opaque_view_and_rejects_duplicate_replays() {
    let mut state = queued();
    let frame = open("token", "V1", None);
    assert_eq!(state.apply(&scope(), &frame, 101), ModalChange::Opened);
    assert_eq!(state.stack()[0].payload.as_ref(), frame.rest.get("view"));
    let revision = state.revision();
    assert_eq!(state.apply(&scope(), &frame, 102), ModalChange::Ignored);
    assert_eq!(state.revision(), revision);
    assert_eq!(state.stack().len(), 1);
    state.clear_local(&scope());
    assert_eq!(state.apply(&scope(), &frame, 103), ModalChange::Ignored);
    assert!(state.stack().is_empty());
}

#[test]
fn wrong_account_team_or_reconnect_cannot_mutate_or_reset_scope() {
    let mut state = queued();
    for wrong in [
        ModalScope {
            account_epoch: 2,
            ..scope()
        },
        ModalScope {
            team: "T2".into(),
            ..scope()
        },
        ModalScope {
            realtime_generation: 8,
            ..scope()
        },
    ] {
        assert_eq!(
            state.apply(&wrong, &open("token", "V1", None), 101),
            ModalChange::Ignored
        );
        assert!(!state.enqueue(&wrong, "other".into(), "A1".into(), 101, 1_000));
        state.clear_local(&wrong);
        assert_eq!(state.token_count(), 1);
        assert_eq!(state.scope(), &scope());
    }
    let next = ModalScope {
        realtime_generation: 8,
        ..scope()
    };
    state.reset_scope(next.clone());
    assert_eq!(state.token_count(), 0);
    assert_eq!(
        state.apply(&next, &open("token", "V1", None), 101),
        ModalChange::Ignored
    );
}

#[test]
fn token_and_remote_timeout_boundaries_and_clock_regression_are_rejected() {
    for now in [99, 600, 1_100] {
        let mut state = queued();
        assert_eq!(
            state.apply(&scope(), &open("token", "V1", None), now),
            ModalChange::Ignored
        );
    }
    let mut state = queued();
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 599),
        ModalChange::Opened
    );
    let mut state = queued();
    let mut frame = open("token", "V1", None);
    frame.rest.remove("timeout_range");
    assert_eq!(state.apply(&scope(), &frame, 1_100), ModalChange::Ignored);
    assert_eq!(state.apply(&scope(), &frame, 1_099), ModalChange::Opened);
}

#[test]
fn only_ten_newest_tokens_are_retained_and_duplicate_registration_does_not_refresh_age() {
    let mut state = queued();
    assert!(!state.enqueue(&scope(), "token".into(), "A1".into(), 500, 9_000));
    for i in 0..10 {
        assert!(state.enqueue(&scope(), format!("next-{i}"), "A1".into(), 101, 1_000));
    }
    assert_eq!(state.token_count(), 10);
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 102),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("next-9", "V1", None), 102),
        ModalChange::Opened
    );
}

#[test]
fn previous_view_must_match_top_and_stack_cannot_cross_apps_or_types() {
    let mut state = queued();
    assert_eq!(
        state.apply(&scope(), &open("token", "V2", Some("missing")), 101),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 101),
        ModalChange::Opened
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V2", None), 102),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V2", Some("V1")), 103),
        ModalChange::Opened
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V3", Some("V1")), 104),
        ModalChange::Ignored
    );
    assert!(state.enqueue(&scope(), "other-app".into(), "A2".into(), 100, 1_000));
    let mut frame = open("other-app", "V3", Some("V2"));
    frame.rest.insert("app_id".into(), json!("A2"));
    frame.rest.get_mut("view").unwrap()["app_id"] = json!("A2");
    assert_eq!(state.apply(&scope(), &frame, 104), ModalChange::Ignored);
    assert_eq!(state.stack().len(), 2);
}

#[test]
fn updates_only_known_views_and_close_does_not_allow_reopening() {
    let mut state = queued();
    assert_eq!(
        state.apply(&scope(), &update("V1"), 101),
        ModalChange::Ignored
    );
    state.apply(&scope(), &open("token", "V1", None), 101);
    assert_eq!(
        state.apply(&scope(), &update("V1"), 102),
        ModalChange::Updated
    );
    assert_eq!(state.stack()[0].payload.as_ref().unwrap()["hash"], "opaque");
    let metadata_only = event(json!({"type":"view_updated","view_id":"V1","app_id":"A1"}));
    assert_eq!(
        state.apply(&scope(), &metadata_only, 103),
        ModalChange::Updated
    );
    assert!(state.stack()[0].needs_content);
    assert_eq!(state.stack()[0].payload.as_ref().unwrap()["hash"], "opaque");
    assert!(state.pop_local(&scope()));
    assert_eq!(
        state.apply(&scope(), &update("V1"), 104),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&scope(), &open("token", "V1", None), 104),
        ModalChange::Ignored
    );
    assert!(!state.pop_local(&scope()));
}

#[test]
fn local_back_pops_one_view_and_clear_on_close_clears_the_root() {
    let mut state = queued();
    state.apply(&scope(), &open("token", "V1", None), 101);
    state.apply(&scope(), &open("token", "V2", Some("V1")), 102);
    assert!(state.pop_local(&scope()));
    assert_eq!(state.stack()[0].id, "V1");
    assert!(state.enqueue(&scope(), "new".into(), "A1".into(), 103, 1_000));
    let mut frame = open("new", "V3", Some("V1"));
    frame.rest.get_mut("view").unwrap()["clear_on_close"] = json!(true);
    state.apply(&scope(), &frame, 104);
    assert!(state.pop_local(&scope()));
    assert!(state.stack().is_empty());
}

#[test]
fn malformed_conflicting_and_nonmodal_frames_do_not_consume_valid_correlation() {
    let mut state = queued();
    let base = open("token", "V1", None);
    for (key, value) in [
        ("timeout_range", json!("500")),
        ("previous_view_id", json!([])),
        ("view_type", json!("home")),
        ("app_id", json!("A2")),
        ("view", json!([])),
        ("view", json!({"id":"OTHER","type":"modal"})),
        (
            "view",
            json!({"id":"V1","type":"modal","root_view_id":"OTHER"}),
        ),
    ] {
        let mut frame = base.clone();
        frame.rest.insert(key.into(), value);
        assert_eq!(state.apply(&scope(), &frame, 101), ModalChange::Ignored);
    }
    assert_eq!(state.apply(&scope(), &base, 102), ModalChange::Opened);
    let mut frame = update("V1");
    frame.rest.get_mut("view").unwrap()["type"] = json!("home");
    let before = state.stack().to_vec();
    assert_eq!(state.apply(&scope(), &frame, 103), ModalChange::Ignored);
    assert_eq!(state.stack(), before);
    assert_eq!(
        state.apply(
            &scope(),
            &event(json!({"type":"view_closed","view_id":"V1"})),
            104
        ),
        ModalChange::Ignored
    );
}

#[test]
fn metadata_only_open_requires_content_and_local_stack_budget_is_bounded() {
    let mut state = queued();
    let mut frame = open("token", "V0", None);
    frame.rest.remove("view");
    assert_eq!(state.apply(&scope(), &frame, 101), ModalChange::Opened);
    assert!(state.stack()[0].needs_content);
    assert!(state.stack()[0].payload.is_none());
    for i in 1..STACK_LIMIT {
        assert_eq!(
            state.apply(
                &scope(),
                &open("token", &format!("V{i}"), Some(&format!("V{}", i - 1))),
                102
            ),
            ModalChange::Opened
        );
    }
    assert_eq!(
        state.apply(&scope(), &open("token", "OVER", Some("V9")), 102),
        ModalChange::Ignored
    );
    assert_eq!(state.stack().len(), STACK_LIMIT);
}
