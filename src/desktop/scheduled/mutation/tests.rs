use super::*;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    shell.core.active_account = Some("fixture-account".into());
    shell.core.account_epoch = 7;
    shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
    shell.core.session = Some(Session {
        d_cookie: "fixture-only-cookie".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team.clone(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only-token".into(),
            },
        )]
        .into(),
    });
    shell.core.scheduled.ensure_scope(7, &team);
    shell
}

type Handles = (Signal<Operations>, Lease);

#[test]
fn dropping_dispatched_run_during_signal_borrow_eventually_retains_uncertainty() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let handles = Rc::new(RefCell::new(None::<Handles>));
            let mut dom = VirtualDom::new_with_props(
                |handles: Rc<RefCell<Option<Handles>>>| {
                    let state = use_signal(shell);
                    let mut operations = use_signal(Operations::default);
                    use_hook(move || {
                        let owner = Owner::capture(&state.peek(), "D1").unwrap();
                        let lease = operations.write().reserve_other(owner).unwrap();
                        let mut run =
                            Run::editor(state, operations, lease.clone(), Action::Edit).unwrap();
                        // Model a write already handed to transport, without any I/O.
                        run.dispatched = true;
                        let borrowed = operations.read();
                        drop(run);
                        assert_eq!(
                            borrowed.other_snapshot(&lease).unwrap().phase,
                            Phase::Running
                        );
                        drop(borrowed);
                        *handles.borrow_mut() = Some((operations, lease));
                    });
                    rsx! { div {} }
                },
                handles.clone(),
            );
            dom.rebuild_in_place();
            for _ in 0..8 {
                let _ = tokio::time::timeout(Duration::from_millis(10), dom.wait_for_work()).await;
                dom.render_immediate(&mut dioxus::core::NoOpMutations);
            }
            let (operations, lease) = handles.borrow().as_ref().unwrap().clone();
            assert_eq!(
                operations.peek().other_snapshot(&lease).unwrap().phase,
                Phase::Uncertain
            );
        });
}

#[test]
fn preflight_rotation_clears_same_owner_busy_but_never_a_replacement_queue() {
    let mut dom = VirtualDom::new(|| {
        let mut state = use_signal(shell);
        let mut operations = use_signal(Operations::default);
        use_hook(move || {
            let owner = Owner::capture(&state.peek(), "D1").unwrap();
            let lease = operations.write().reserve_other(owner).unwrap();
            let run = Run::editor(state, operations, lease.clone(), Action::Edit).unwrap();
            state.write().core.scheduled.busy.insert("D1".into());
            let team = state.peek().core.active_team.clone().unwrap();
            state
                .write()
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token = "rotated-fixture-token".into();
            state.write().core.transport =
                Some(Arc::new(Transport::new("rotated-fixture-cookie").unwrap()));
            assert!(run.check(state).is_err());
            run.clear_busy(state);
            assert!(!state.peek().core.scheduled.busy.contains("D1"));
            state.write().core.scheduled.ensure_scope(8, &team);
            state.write().core.scheduled.busy.insert("D1".into());
            run.clear_busy(state);
            assert!(state.peek().core.scheduled.busy.contains("D1"));
            drop(run);
            assert_eq!(
                operations.peek().other_snapshot(&lease).unwrap().phase,
                Phase::Idle
            );
        });
        rsx! { div {} }
    });
    dom.rebuild_in_place();
}

fn legacy_raw() -> serde_json::Value {
    serde_json::json!({"ok":true,"draft":{"id":"D1","date_scheduled":0,
        "blocks":[{"type":"rich_text","elements":[]}],"destinations":[{"channel_id":"C1"}],
        "file_ids":[],"is_deleted":false,"is_sent":false}})
}

#[test]
fn actual_run_preflight_accepts_legacy_metadata_and_refreshes_complete_conflicts() {
    let mut dom = VirtualDom::new(|| {
        let mut state = use_signal(shell);
        let mut operations = use_signal(Operations::default);
        use_hook(move || {
            let raw = legacy_raw();
            let expected: ScheduledMessage = serde_json::from_value(raw["draft"].clone()).unwrap();
            state.write().core.scheduled.accept_remote(expected.clone());
            let owner = Owner::capture(&state.peek(), "D1").unwrap();
            let lease = operations.write().reserve_other(owner).unwrap();
            let run = Run::editor(state, operations, lease, Action::Edit).unwrap();
            let info = api::decode_drafts_info("D1", raw.clone()).unwrap();
            let mut accepted = run.preflight_response(state, &expected, &info).unwrap();
            assert!(accepted.client_msg_id.is_empty());
            ensure_send_identity(&mut accepted);
            let frozen = accepted.client_msg_id.clone();
            assert!(!frozen.is_empty());
            ensure_send_identity(&mut accepted);
            assert_eq!(accepted.client_msg_id, frozen);
            assert!(expected.client_msg_id.is_empty());
            for (field, value) in [
                ("blocks", serde_json::json!([])),
                ("client_msg_id", serde_json::json!("new-id")),
                ("last_updated_ts", serde_json::json!("2.000001")),
            ] {
                state
                    .write()
                    .core
                    .scheduled
                    .items
                    .insert("D1".into(), expected.clone());
                let mut changed = raw.clone();
                changed["draft"][field] = value.clone();
                let info = api::decode_drafts_info("D1", changed).unwrap();
                assert!(
                    matches!(run.preflight_response(state, &expected, &info), Err(Error::Api(code)) if code == "draft_has_conflict")
                );
                let current = &state.peek().core.scheduled.items["D1"];
                match field {
                    "blocks" => assert!(current.blocks.is_empty()),
                    "client_msg_id" => assert_eq!(current.client_msg_id, "new-id"),
                    _ => assert_eq!(current.last_updated_ts, "2.000001"),
                }
            }
            state
                .write()
                .core
                .scheduled
                .items
                .insert("D1".into(), expected.clone());
            let mut sent = raw;
            sent["draft"]["is_sent"] = serde_json::json!(true);
            sent["draft"]["last_updated_ts"] = serde_json::json!("3.000001");
            let info = api::decode_drafts_info("D1", sent).unwrap();
            assert!(run.preflight_response(state, &expected, &info).is_err());
            assert!(!state.peek().core.scheduled.items.contains_key("D1"));
            assert!(!run.unknown());
        });
        rsx! { div {} }
    });
    dom.rebuild_in_place();
}

#[test]
fn actual_run_preflight_never_projects_partial_or_stale_owner_readbacks() {
    let mut dom = VirtualDom::new(|| {
        let mut state = use_signal(shell);
        let mut operations = use_signal(Operations::default);
        use_hook(move || {
            let raw = legacy_raw();
            let expected: ScheduledMessage = serde_json::from_value(raw["draft"].clone()).unwrap();
            state.write().core.scheduled.accept_remote(expected.clone());
            let owner = Owner::capture(&state.peek(), "D1").unwrap();
            let lease = operations.write().reserve_other(owner).unwrap();
            let run = Run::editor(state, operations, lease, Action::Delete).unwrap();
            for field in [
                "blocks",
                "destinations",
                "file_ids",
                "date_scheduled",
                "is_deleted",
                "is_sent",
            ] {
                let mut partial = raw.clone();
                partial["draft"].as_object_mut().unwrap().remove(field);
                let info = api::decode_drafts_info("D1", partial).unwrap();
                assert!(
                    matches!(run.preflight_response(state, &expected, &info), Err(Error::Api(code)) if code == "draft_info_unresolved")
                );
                assert_eq!(
                    state.peek().core.scheduled.items["D1"].blocks,
                    expected.blocks
                );
            }
            let team = state.peek().core.active_team.clone().unwrap();
            state
                .write()
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token = "rotated".into();
            let mut changed = raw;
            changed["draft"]["blocks"] = serde_json::json!([]);
            let info = api::decode_drafts_info("D1", changed).unwrap();
            assert!(
                matches!(run.preflight_response(state, &expected, &info), Err(Error::Api(code)) if code == "draft_account_changed")
            );
            assert_eq!(
                state.peek().core.scheduled.items["D1"].blocks,
                expected.blocks
            );
            assert!(!run.unknown());
        });
        rsx! { div {} }
    });
    dom.rebuild_in_place();
}

#[test]
fn actual_run_refuses_old_info_after_changed_or_retired_queue_evidence() {
    let mut dom = VirtualDom::new(|| {
        let mut state = use_signal(shell);
        let mut operations = use_signal(Operations::default);
        use_hook(move || {
            for case in ["newer", "equal", "unstamped", "sent", "deleted"] {
                state.write().core.scheduled = Default::default();
                state.write().core.scheduled.ensure_scope(7, "T1");
                let mut raw = legacy_raw();
                raw["draft"]["last_updated_ts"] =
                    serde_json::json!(if case == "unstamped" { "" } else { "1.000001" });
                let expected: ScheduledMessage =
                    serde_json::from_value(raw["draft"].clone()).unwrap();
                let mut newer = expected.clone();
                if case == "newer" {
                    newer.last_updated_ts = "2.000001".into();
                }
                newer.blocks.clear();
                newer.is_sent = case == "sent";
                newer.is_deleted = case == "deleted";
                state.write().core.scheduled.accept_remote(newer);
                let owner = Owner::capture(&state.peek(), "D1").unwrap();
                let lease = operations.write().reserve_other(owner).unwrap();
                let run = Run::editor(state, operations, lease.clone(), Action::Send).unwrap();
                let info = api::decode_drafts_info("D1", raw).unwrap();
                assert!(
                    matches!(run.preflight_response(state, &expected, &info), Err(Error::Api(code)) if code == "draft_has_conflict"),
                    "{case}"
                );
                if matches!(case, "sent" | "deleted") {
                    assert!(!state.peek().core.scheduled.items.contains_key("D1"));
                } else {
                    assert!(state.peek().core.scheduled.items["D1"].blocks.is_empty());
                }
                assert!(!run.unknown());
                drop(run);
                operations.write().release_other(&lease).unwrap();
            }
        });
        rsx! { div {} }
    });
    dom.rebuild_in_place();
}
