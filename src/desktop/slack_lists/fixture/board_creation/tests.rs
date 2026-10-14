use super::*;
use super_platinum_core::slack_lists::CreationIntent;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), "channel");
    install(&mut shell, "slack-list-board-add");
    shell
}

#[test]
fn static_fixture_has_populated_and_empty_lanes_without_check_configuration() {
    let shell = shell();
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    assert_eq!(shell.self_account.user_id, "U00000001");
    let scope = shell.slack_lists.scope.as_ref().unwrap();
    assert!(scope.current(&shell));
    assert_eq!(scope.stable_owner().2, "U00000001");
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.layout.selected.as_deref(), Some(SELECT_VIEW));
    assert!(
        view.layout
            .lanes
            .iter()
            .any(|lane| lane.key == "OPT_PROGRESS")
    );
    assert!(view.layout.lanes.iter().any(|lane| lane.key.is_empty()));
    assert!(requests(&shell).is_empty());
}

#[test]
fn fixture_seed_oracle_matches_independent_core_capture_for_three_lane_shapes() {
    let shell = shell();
    let current = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    for (view, lane, checkbox, empty) in [
        (SELECT_VIEW, "OPT_PROGRESS", false, false),
        (SELECT_VIEW, "", false, true),
        (CHECK_VIEW, "", true, true),
    ] {
        let intent = CreationIntent::capture_board(
            &current.document,
            current.allow_lists.as_deref(),
            current.personal_task_lists.as_ref(),
            view,
            lane,
            &shell.self_account.user_id,
        )
        .unwrap();
        assert_eq!(
            intent.initial_fields(),
            Some(&expected_fields(
                &shell.self_account.user_id,
                checkbox,
                empty
            ))
        );
        assert_eq!(intent.position(), "");
        assert_eq!(intent.source_record_id(), None);
    }
}

#[test]
fn backend_only_mutation_leaves_visible_source_and_refuses_preflight() {
    let mut shell = shell();
    let visible = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    let intent = CreationIntent::capture_board(
        &visible.document,
        visible.allow_lists.as_deref(),
        visible.personal_task_lists.as_ref(),
        CHECK_VIEW,
        "",
        &shell.self_account.user_id,
    )
    .unwrap();
    change_backend(&mut shell, true);
    let canonical = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    assert_ne!(canonical.document.raw, visible.document.raw);
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document
            .raw,
        visible.document.raw
    );
    assert!(
        intent
            .revalidate(
                &canonical.document,
                canonical.allow_lists.as_deref(),
                canonical.personal_task_lists.as_ref(),
                Some(CHECK_VIEW)
            )
            .is_err()
    );
    assert!(requests(&shell).is_empty());
}

#[test]
fn request_oracle_refuses_invented_routing_reordered_seeds_and_retry() {
    let expected = expected_fields("U00000001", false, false);
    let fields = json!([
        ["token", "fixture-not-a-token"],
        ["list_id", "F_LIST_FIXTURE"],
        ["position", ""],
        ["updated_timestamp", "1800000000000-0001-0123456789abcdef"],
        ["_x_reason", "create-record"],
        ["initial_fields", expected.to_string()]
    ]);
    let valid = json!({"method":"POST","url":"https://example.test/api/lists.records.create","retrySafe":false,"fields":fields});
    assert!(validate_request(&valid, &expected));
    for malformed in [
        json!([]),
        json!(["token"]),
        json!(["token", "value", "extra"]),
    ] {
        let mut changed = valid.clone();
        changed["fields"][0] = malformed;
        assert!(!validate_request(&changed, &expected));
    }
    for key in [
        "view_id",
        "parent_record_id",
        "duplicated_record_id",
        "fields",
    ] {
        let mut changed = valid.clone();
        changed["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!([key, "invented"]));
        assert!(!validate_request(&changed, &expected));
    }
    let mut changed = valid.clone();
    changed["retrySafe"] = json!(true);
    assert!(!validate_request(&changed, &expected));
    let mut reversed = expected.clone();
    reversed.as_array_mut().unwrap().reverse();
    let mut changed = valid;
    changed["fields"][5][1] = json!(reversed.to_string());
    assert!(!validate_request(&changed, &expected));
}
