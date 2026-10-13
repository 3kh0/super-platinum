use super::*;

#[test]
fn locale_reprojection_preserves_expansion_and_editor_revision() {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::set_number_precision(&mut shell, Some(2));
    let visit = generation(&shell).unwrap();
    assert!(toggle_expanded_shell(&mut shell, "R_LIST_010", visit));
    editor::begin_shell(&mut shell, "R_LIST_005", "C_NUMBER", visit).unwrap();
    let before = editor::presentation(&shell).unwrap();
    let revision = shell.slack_lists.view.as_ref().unwrap().revision;
    let rows = shell.slack_lists.view.as_ref().unwrap().rows.clone();
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().locale = Some("de-DE".into());
    locale_changed(&mut shell, "OTHER_TEAM");
    assert!(std::sync::Arc::ptr_eq(
        &rows,
        &shell.slack_lists.view.as_ref().unwrap().rows
    ));
    locale_changed(&mut shell, &team);
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.revision, revision);
    assert_eq!(view.generation, visit);
    assert_eq!(view.read, 0);
    assert!(
        view.rows
            .iter()
            .any(|row| row.id == "R_LIST_010" && row.expanded)
    );
    let row = view.rows.iter().find(|row| row.id == "R_LIST_005").unwrap();
    assert_eq!(
        row.cells
            .iter()
            .find(|cell| cell.column_id == "C_NUMBER")
            .unwrap()
            .text,
        "12,50"
    );
    assert!(editor::presentation(&shell).unwrap() == before);
    let attempt = editor::begin_save(
        &mut shell,
        before.generation,
        CellEditValue::Number(Some("12.75".parse().unwrap())),
    )
    .unwrap();
    assert_eq!(
        attempt.edit.cells()[0]["number"],
        serde_json::json!([12.75])
    );
    assert_eq!(fixture::attempt_count(&shell), 0);
}
