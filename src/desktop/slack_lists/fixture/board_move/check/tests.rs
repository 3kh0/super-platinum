use super::*;
#[test]
fn gate_requires_exact_alias_and_flag() {
    for alias in [
        "slack-list-board-move",
        "slack-list-board-move-narrow",
        "slack-list-board-move-checkbox",
        "slack-list-board-move-checkbox-narrow",
    ] {
        assert!(gate(Some("1"), Some(alias)));
        assert!(!gate(None, Some(alias)));
        assert!(!gate(Some("true"), Some(alias)));
    }
    assert!(!gate(Some("1"), Some("slack-list-board")));
    assert!(!gate(Some("1"), None));
}
#[test]
fn expected_pages_preserve_every_unrelated_member_and_require_genuine_missing_checkbox() {
    for checkbox in [false, true] {
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-board-move",
        );
        let f = shell.slack_lists.fixture.as_mut().unwrap();
        let row = f.records["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == ROW)
            .unwrap();
        if checkbox {
            row["fields"]
                .as_array_mut()
                .unwrap()
                .retain(|f| f["key"] != "done");
        }
        let original = f.records.clone();
        let p = pages(&original, checkbox).unwrap();
        assert_eq!(p[0], original);
        for i in [1, 2] {
            let mut restored = p[i].clone();
            let fields = restored["records"][0]["fields"].as_array_mut().unwrap();
            if checkbox {
                fields.retain(|f| f["key"] != "done");
            } else {
                fields.iter_mut().find(|f| f["key"] == "status").unwrap()["select"] =
                    json!(["OPT_PROGRESS"]);
            }
            assert_eq!(restored, original);
        }
        if checkbox {
            let mut malformed = original.clone();
            malformed["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"done","checkbox":null}));
            assert!(pages(&malformed, true).is_err());
        } else {
            let mut malformed = original.clone();
            malformed["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"status","select":[]}));
            assert!(pages(&malformed, false).is_err());
        }
    }
}
#[test]
fn actual_named_board_projection_tracks_empty_checked_and_filtered_destinations() {
    for checkbox in [false, true] {
        let name = if checkbox {
            "slack-list-board-move-checkbox"
        } else {
            "slack-list-board-move"
        };
        let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let generation = crate::slack_lists::editor::presentation(&shell)
            .unwrap()
            .generation;
        assert!(crate::slack_lists::editor::cancel_shell(
            &mut shell, generation
        ));
        let f = shell.slack_lists.fixture.as_mut().unwrap();
        if checkbox {
            f.records["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["key"] != "done");
        } else {
            f.document["list_metadata"]["views"][0]["filters"] =
                json!([{"key":"status","operator":"does_not_contain","values":["OPT_DONE"]}]);
        }
        let expected = pages(&f.records, checkbox).unwrap();
        let document = f.document.clone();
        for stage in 0..=2 {
            shell.slack_lists.fixture.as_mut().unwrap().records = expected[stage].clone();
            super::super::super::refresh(&mut shell);
            let v = shell.slack_lists.view.as_mut().unwrap();
            v.layout.find.input = "Launch".into();
            v.layout.find.applied = "Launch".into();
            v.layout.find.revision = next_id();
            crate::slack_lists::rebuild_rows(&mut shell);
            let p = crate::slack_lists::presentation(&shell).unwrap();
            assert_eq!(p.layout_kind, ProjectionLayout::Board);
            assert!(p.view_notice.is_none());
            let memberships = p
                .lanes
                .iter()
                .filter(|l| l.rows.iter().any(|r| r.id == ROW))
                .map(|l| l.key.as_str())
                .collect::<Vec<_>>();
            let want = if checkbox {
                vec![if stage == 1 { "true" } else { "" }]
            } else {
                match stage {
                    0 => vec!["OPT_PROGRESS"],
                    1 => vec![""],
                    _ => vec![],
                }
            };
            assert_eq!(memberships, want);
            assert_eq!(
                p.lanes.iter().map(|l| l.count).sum::<usize>(),
                if !checkbox && stage == 2 { 69 } else { 70 }
            );
            let loaded = shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap();
            assert_eq!(loaded.document.raw, document);
            assert_eq!(loaded.records.raw, expected[stage]);
        }
    }
}
fn armed_fixture(checkbox: bool) -> ShellState {
    let alias = if checkbox {
        "slack-list-board-move-checkbox"
    } else {
        "slack-list-board-move"
    };
    let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
    let generation = crate::slack_lists::editor::presentation(&shell)
        .unwrap()
        .generation;
    assert!(crate::slack_lists::editor::cancel_shell(
        &mut shell, generation
    ));
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    if checkbox {
        f.records["records"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .retain(|f| f["key"] != "done");
    } else {
        f.document["list_metadata"]["views"][0]["filters"] =
            json!([{"key":"status","operator":"does_not_contain","values":["OPT_DONE"]}]);
    }
    super::super::super::refresh(&mut shell);
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.layout.find.input = "Launch".into();
    v.layout.find.applied = "Launch".into();
    v.layout.find.revision = next_id();
    crate::slack_lists::rebuild_rows(&mut shell);
    arm_owned(&mut shell, checkbox).unwrap();
    state_mut(&mut shell).test_only = true;
    super::super::super::refresh(&mut shell);
    assert_eq!(
        snapshot(&shell).unwrap()["reads"].as_array().unwrap().len(),
        1
    );
    shell
}
fn begin_actual(shell: &mut ShellState, value: CellEditValue) -> EditAttempt {
    let v = shell.slack_lists.view.as_ref().unwrap();
    let row = v
        .layout
        .lanes
        .iter()
        .flat_map(|l| l.rows.iter())
        .find(|r| r.id == ROW)
        .unwrap();
    let (instance, visit, layout) = (row.instance_id.clone(), v.generation, v.layout.generation);
    crate::slack_lists::board_move::open_shell(shell, &instance, visit, layout).unwrap();
    let generation = crate::slack_lists::editor::presentation(shell)
        .unwrap()
        .generation;
    crate::slack_lists::editor::begin_save(shell, generation, value).unwrap()
}
fn request_for(shell: &ShellState, a: &EditAttempt) -> PreparedRequest {
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    api::list_cell_update(&shell.core.client, &session, &a.edit)
}
#[test]
fn armed_actual_worker_two_attempts_readback_and_refused_extra_hooks() {
    for checkbox in [false, true] {
        let mut shell = armed_fixture(checkbox);
        let baseline = snapshot(&shell).unwrap();
        assert!(queue_missing(&mut shell).is_err());
        assert!(duplicate_consumed_read(&mut shell).is_err());
        let first = begin_actual(
            &mut shell,
            if checkbox {
                CellEditValue::Boolean(true)
            } else {
                CellEditValue::Select(vec![])
            },
        );
        let form = request_for(&shell, &first);
        assert!(request(&mut shell, &first, &form).is_err()); // before actual validation/preflight
        super::super::super::dispatch(&mut shell, first.clone());
        assert!(request(&mut shell, &first, &form).is_err()); // duplicate actual form
        let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
        assert!(preflight(&mut shell, &first, &canonical).is_err());
        super::super::super::set_outcome_uncertain(&mut shell);
        super::super::super::complete_pending(&mut shell);
        let lost = snapshot(&shell).unwrap();
        assert_eq!(lost["loaded"], baseline["pages"][0]);
        assert_eq!(lost["operation"]["phase"], "Uncertain");
        assert_eq!(lost["server"], baseline["pages"][1]);
        duplicate_consumed_read(&mut shell).unwrap();
        let duplicate = snapshot(&shell).unwrap();
        assert_eq!(duplicate["loaded"], lost["loaded"]);
        assert_eq!(duplicate["operation"], lost["operation"]);
        assert_eq!(duplicate["duplicate"]["ignored"], true);
        assert!(duplicate_consumed_read(&mut shell).is_err());
        if checkbox {
            queue_missing(&mut shell).unwrap();
            assert!(queue_missing(&mut shell).is_err());
            super::super::super::refresh(&mut shell);
            let missing = snapshot(&shell).unwrap();
            assert_eq!(missing["loaded"], baseline["pages"][0]);
            assert_eq!(missing["operation"], lost["operation"]);
            assert_eq!(missing["reads"][1]["readback"], Value::Null);
            assert_eq!(missing["attemptCount"], 1);
        }
        super::super::super::refresh(&mut shell);
        let observed = snapshot(&shell).unwrap();
        assert_eq!(observed["loaded"], baseline["pages"][1]);
        assert!(observed["operation"].is_null());
        assert!(observed["editor"].is_null());
        let second = begin_actual(
            &mut shell,
            if checkbox {
                CellEditValue::Boolean(false)
            } else {
                CellEditValue::Select(vec!["OPT_DONE".into()])
            },
        );
        super::super::super::dispatch(&mut shell, second.clone());
        super::super::super::complete_pending(&mut shell);
        let ack = snapshot(&shell).unwrap();
        assert_eq!(ack["loaded"], baseline["pages"][1]);
        assert_eq!(ack["operation"]["phase"], "Uncertain");
        assert_eq!(ack["completed"][1]["result"], "positive_ack");
        super::super::super::refresh(&mut shell);
        let final_state = snapshot(&shell).unwrap();
        assert_eq!(final_state["loaded"], baseline["pages"][2]);
        assert_eq!(final_state["attemptCount"], 2);
        assert_eq!(final_state["requests"].as_array().unwrap().len(), 2);
        assert_eq!(final_state["completed"].as_array().unwrap().len(), 2);
        assert_eq!(
            final_state["reads"].as_array().unwrap().len(),
            if checkbox { 4 } else { 3 }
        );
        assert!(final_state["operation"].is_null());
        let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
        let form = request_for(&shell, &second);
        assert!(preflight(&mut shell, &second, &canonical).is_err());
        assert!(request(&mut shell, &second, &form).is_err());
        assert!(completion(&mut shell, &second, false).is_err());
        assert_eq!(snapshot(&shell).unwrap(), final_state);
        let read = begin_read(&mut shell).unwrap();
        let mut data = Ok(canonical);
        assert!(read_input(&mut shell, &read, &mut data).is_err());
    }
}
