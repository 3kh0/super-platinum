use super::*;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::add_saved_views(&mut shell);
    shell
}
fn attempt(shell: &ShellState) -> Attempt {
    Attempt {
        scope: Scope::capture(shell).unwrap(),
        id: shell.slack_lists.view.as_ref().unwrap().id.clone(),
        visit: generation(shell).unwrap(),
    }
}
#[test]
fn default_completion_is_visit_owned_and_never_overrides_manual_fallback() {
    let mut shell = shell();
    let old = attempt(&shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let layout = view.layout.generation;
    assert!(layout::select_shell(&mut shell, None, old.visit, layout));
    settle(&mut shell, &old, Some("V_LIST_REORDERED"));
    assert!(presentation(&shell).unwrap().selected_view.is_none());
    let file: File = serde_json::from_value(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document
            .raw
            .clone(),
    )
    .unwrap();
    close_shell(&mut shell);
    open_shell(&mut shell, file);
    fixture::refresh(&mut shell);
    settle(&mut shell, &old, Some("V_LIST_REORDERED"));
    assert_ne!(generation(&shell), Some(old.visit));
    assert_ne!(
        presentation(&shell).unwrap().selected_view.as_deref(),
        Some("V_LIST_REORDERED")
    );
}

#[test]
fn async_default_order_is_first_applicable_change_and_never_waits_for_other_result() {
    for personal_first in [true, false] {
        let mut shell = shell();
        let attempt = attempt(&shell);
        if personal_first {
            settle_personal(&mut shell, &attempt, Some("V_LIST_GROUPED"), false);
            assert_eq!(
                presentation(&shell).unwrap().selected_view.as_deref(),
                Some("V_LIST_GROUPED")
            );
            settle(&mut shell, &attempt, Some("V_LIST_REORDERED"));
            assert_eq!(
                presentation(&shell).unwrap().selected_view.as_deref(),
                Some("V_LIST_GROUPED")
            );
        } else {
            settle(&mut shell, &attempt, Some("V_LIST_REORDERED"));
            assert_eq!(
                presentation(&shell).unwrap().selected_view.as_deref(),
                Some("V_LIST_REORDERED")
            );
            settle_personal(&mut shell, &attempt, Some("V_LIST_GROUPED"), true);
            assert_eq!(
                presentation(&shell).unwrap().selected_view.as_deref(),
                Some("V_LIST_REORDERED")
            );
        }
    }
}
#[test]
fn late_defaults_preserve_an_open_editor_and_reject_old_account() {
    let mut shell = shell();
    let attempt = attempt(&shell);
    editor::begin_shell(&mut shell, "R_LIST_005", "C_NUMBER", attempt.visit).unwrap();
    let draft = editor::presentation(&shell).unwrap();
    settle_personal(&mut shell, &attempt, Some("V_LIST_REORDERED"), false);
    settle(&mut shell, &attempt, Some("V_LIST_GROUPED"));
    assert_eq!(
        editor::presentation(&shell).unwrap().generation,
        draft.generation
    );
    assert!(presentation(&shell).unwrap().selected_view.is_none());
    editor::cancel_shell(&mut shell, draft.generation);
    shell.core.account_epoch += 1;
    settle(&mut shell, &attempt, Some("V_LIST_REORDERED"));
    assert!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .selected
            .is_none()
    );
}

#[test]
fn recovered_feature_flags_start_only_the_unattempted_personal_default_request() {
    let mut shell = shell();
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.loaded.as_mut().unwrap().list_experiments = None;
    assert_eq!(claim(view), Some((true, false)));
    assert_eq!(claim(view), None);
    view.loaded.as_mut().unwrap().list_experiments = Some(api::ListExperiments {
        schema_owner_only: false,
        grid_todos: true,
        todos: false,
        todos_rollback: false,
        default_view_dual_write: true,
        sub_tasks: false,
    });
    assert_eq!(claim(view), Some((false, true)));
    assert_eq!(claim(view), None);
    assert!(view.layout.read_started && view.layout.personal_started);
}

#[test]
fn prior_read_response_reprojects_current_view_without_default_selection_change() {
    let mut shell = shell();
    fixture::edit_backend_for_test(&mut shell, |document, _| {
        document["list_metadata"]["schema"].as_array_mut().unwrap().push(serde_json::json!({"id":"C_CREATED","key":"created_time","name":"Created","type":"created_time"}));
        document["list_metadata"]["views"].as_array_mut().unwrap().push(serde_json::json!({
            "id":"V_SINCE_READ","name":"Since prior read","type":"table",
            "columns":[{"id":"C_NAME","key":"NAME","visible":true}],
            "filters":[{"key":"created_time","column_id":"C_CREATED","operator":"equal","values":["since_i_last_viewed"]}]
        }));
    });
    fixture::refresh(&mut shell);
    assert!(date_context::set_override(
        &mut shell,
        super_platinum_core::slack_lists::saved_views::DateContext::from_local(
            "2026-10-13T12:00:00",
            None
        )
        .unwrap()
    ));
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        &mut shell,
        Some("V_SINCE_READ".into()),
        visit,
        layout
    ));
    assert!(presentation(&shell).unwrap().view_notice.is_some());
    let attempt = attempt(&shell);
    accept_open(
        &mut shell,
        &attempt,
        api::ListOpen {
            last_read_timestamp: Some(1_700_000_020),
            resolved_default_view: None,
        },
    );
    let view = presentation(&shell).unwrap();
    assert_eq!(view.selected_view.as_deref(), Some("V_SINCE_READ"));
    assert!(view.view_notice.is_none());
    assert_eq!(view.rows.len(), 52);
    accept_open(
        &mut shell,
        &attempt,
        api::ListOpen {
            last_read_timestamp: Some(1_700_000_060),
            resolved_default_view: None,
        },
    );
    assert_eq!(presentation(&shell).unwrap().rows.len(), 12);
    let rows = presentation(&shell).unwrap().rows;
    accept_open(
        &mut shell,
        &attempt,
        api::ListOpen {
            last_read_timestamp: Some(1_700_000_060),
            resolved_default_view: None,
        },
    );
    assert!(std::sync::Arc::ptr_eq(
        &rows,
        &presentation(&shell).unwrap().rows
    ));
    shell.core.account_epoch += 1;
    accept_open(
        &mut shell,
        &attempt,
        api::ListOpen {
            last_read_timestamp: Some(0),
            resolved_default_view: None,
        },
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .last_read_timestamp,
        Some(1_700_000_060)
    );
}
