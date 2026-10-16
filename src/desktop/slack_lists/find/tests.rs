use super::*;
use serde_json::json;
fn shell() -> ShellState {
    ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list-find")
}
fn set(shell: &mut ShellState, text: &str) {
    let visit = generation(shell).unwrap();
    let serial = begin(shell, text.into(), visit).unwrap();
    assert!(apply(shell, visit, serial));
}
#[test]
fn query_is_visit_owned_and_debounce_refuses_superseded_and_other_views() {
    let mut shell = shell();
    let visit = generation(&shell).unwrap();
    let first = begin(&mut shell, "first".into(), visit).unwrap();
    let last = begin(&mut shell, "72".into(), visit).unwrap();
    assert!(!apply(&mut shell, visit, first));
    assert!(apply(&mut shell, visit, last));
    let layout = presentation(&shell).unwrap().layout_generation;
    assert!(layout::select_shell(&mut shell, None, visit, layout));
    assert!(!apply(&mut shell, visit, last));
    assert!(begin(&mut shell, "x".into(), visit).is_none());
    assert!(presentation(&shell).unwrap().find_input.is_empty());
    close_shell(&mut shell);
    assert!(!apply(&mut shell, visit, last));
}
#[test]
fn hidden_field_find_preserves_loaded_count_cursor_and_draft_when_filtered() {
    let mut shell = shell();
    set(&mut shell, "");
    let visit = generation(&shell).unwrap();
    editor::begin_shell(&mut shell, "R_LIST_000", "C_NAME", visit).unwrap();
    let before = editor::presentation(&shell).unwrap();
    let raw = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    set(&mut shell, "72");
    let view = presentation(&shell).unwrap();
    assert_eq!(view.total_rows, 73);
    assert!(view.incomplete);
    assert!(!view.columns.iter().any(|c| c.id == "C_NUMBER"));
    assert!(view.rows.iter().any(|r| r.id == "R_LIST_072"));
    let after = view.editor.unwrap();
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.original, after.original);
    assert!(after.disabled);
    set(&mut shell, "");
    assert!(!editor::presentation(&shell).unwrap().disabled);
    assert_eq!(
        raw,
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
    );
    assert!(shell.core.transport.is_none() && shell.core.session.is_none());
}
#[test]
fn normalized_name_precedence_is_not_raw_display_precedence() {
    let mut user:User=serde_json::from_value(json!({"id":"U1","real_name":"Raw real","profile":{"display_name":"Profile display"},"_real_name_normalized_lc":"normalized real","_display_name_normalized_lc":"normalized display"})).unwrap();
    assert_eq!(user_name(&user), "normalized real");
    user.extra.remove("_real_name_normalized_lc");
    assert_eq!(user_name(&user), "normalized display");
    user.extra.remove("_display_name_normalized_lc");
    assert_eq!(user_name(&user), "Profile display");
    user.profile = None;
    assert_eq!(user_name(&user), "");
}
#[test]
fn six_named_find_fixtures_are_disconnected_and_keep_boundary() {
    for name in [
        "slack-list-find",
        "slack-list-find-narrow",
        "slack-list-find-board",
        "slack-list-find-board-narrow",
        "slack-list-find-empty",
        "slack-list-find-empty-narrow",
    ] {
        let shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let view = presentation(&shell).unwrap();
        assert!(view.selected_view.is_some());
        assert!(view.incomplete);
        assert!(view.view_notice.is_none(), "{name}: {:?}", view.view_notice);
        assert_eq!(view.matched_rows == 0, name.contains("empty"));
        assert_eq!(
            view.layout_kind == ProjectionLayout::Board,
            name.contains("board")
        );
        if name.contains("board") {
            assert!(view.lanes.len() > 4);
            assert!(view.lanes.iter().any(|lane| lane.rows.len() > 10));
        }
        assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    }
}
#[test]
fn clearing_query_gets_a_new_page_identity_even_when_text_returns_to_empty() {
    let mut shell = shell();
    set(&mut shell, "");
    let first = presentation(&shell).unwrap().find_revision;
    set(&mut shell, "72");
    let filtered = presentation(&shell).unwrap().find_revision;
    set(&mut shell, "");
    let cleared = presentation(&shell).unwrap().find_revision;
    assert_ne!(first, filtered);
    assert_ne!(first, cleared);
    assert_ne!(filtered, cleared);
}
#[test]
fn same_display_name_with_new_normalized_name_reprojects_hidden_user_match() {
    use super_platinum_core::slack::events::RtEvent;
    let mut shell = shell();
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.rt_generation = 7;
    let user = workspace.users.get_mut("U1").unwrap();
    user.extra
        .insert("_real_name_normalized_lc".into(), json!("needle-before"));
    set(&mut shell, "needle-before");
    assert!(presentation(&shell).unwrap().matched_rows > 0);
    let mut user = shell.core.workspaces["T1"].users["U1"].clone();
    user.extra
        .insert("_real_name_normalized_lc".into(), json!("needle-after"));
    let change = labels::prepare(&shell, "T1", 7, &RtEvent::UserChanged(user.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("U1".into(), user);
    assert!(change.apply(&mut shell));
    assert_eq!(presentation(&shell).unwrap().matched_rows, 0);
    assert_eq!(presentation(&shell).unwrap().find_applied, "needle-before");
    let mut restored = shell.core.workspaces["T1"].users["U1"].clone();
    restored
        .extra
        .insert("_real_name_normalized_lc".into(), json!("needle-before"));
    assert!(labels::prepare(&shell, "OTHER", 7, &RtEvent::UserChanged(restored.clone())).is_none());
    assert!(labels::prepare(&shell, "T1", 6, &RtEvent::UserChanged(restored.clone())).is_none());
    let change = labels::prepare(&shell, "T1", 7, &RtEvent::UserChanged(restored.clone())).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .insert("U1".into(), restored);
    assert!(change.apply(&mut shell));
    assert!(presentation(&shell).unwrap().matched_rows > 0);
}
#[test]
fn refreshed_rows_and_schema_recompute_without_query_or_draft_mutation() {
    let mut shell = shell();
    set(&mut shell, "72");
    fixture::find::change_backend(&mut shell, 1).unwrap();
    fixture::refresh(&mut shell);
    assert!(
        !presentation(&shell)
            .unwrap()
            .rows
            .iter()
            .any(|row| row.id == "R_LIST_072")
    );
    assert_eq!(presentation(&shell).unwrap().find_applied, "72");
    fixture::find::change_backend(&mut shell, 2).unwrap();
    fixture::refresh(&mut shell);
    assert_eq!(presentation(&shell).unwrap().matched_rows, 0);
    assert_eq!(fixture::attempt_count(&shell), 0);
}
