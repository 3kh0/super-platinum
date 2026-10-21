use super::*;

fn hover(shell: &mut ShellState) -> u64 {
    shell.show_profile_hover("U1".into(), 120.0, 160.0);
    shell.hold_profile_hover();
    shell.profile_hover_generation
}

#[test]
fn successful_admission_dismisses_hover_without_changing_rich_seed_or_source() {
    let mut shell = shell();
    let original = loaded(&shell);
    let generation = hover(&mut shell);
    assert!(!cell_editor_blocks_profile_hover(&shell));
    let editor = open(&mut shell, "name");
    assert!(cell_editor_blocks_profile_hover(&shell));
    assert!(shell.profile_hover.is_none());
    assert!(!shell.profile_hover_card_active);
    assert_ne!(shell.profile_hover_generation, generation);
    let presentation = presentation(&shell).unwrap();
    assert_eq!(presentation.generation, editor);
    let CellEditValue::RichText(blocks) = presentation.original.as_ref() else {
        panic!("rich editor seed");
    };
    assert_eq!(
        json!(blocks),
        original.records.records[0].raw["fields"][0]["rich_text"]
    );
    assert_eq!(loaded(&shell).document.raw, original.document.raw);
    assert_eq!(
        loaded(&shell).records.records[0].raw,
        original.records.records[0].raw
    );
    assert!(shell.slack_lists.operations.is_empty());
    assert!(cancel_shell(&mut shell, editor));
    assert!(!cell_editor_blocks_profile_hover(&shell));
    hover(&mut shell);
    assert_eq!(shell.profile_hover.as_ref().unwrap().user_id, "U1");
}

#[test]
fn rejected_editor_admission_leaves_existing_hover_unchanged() {
    let mut shell = shell();
    let hover_generation = hover(&mut shell);
    let visit = super::super::super::generation(&shell).unwrap();
    assert!(begin_shell(&mut shell, "row", "missing", visit).is_err());
    assert!(begin_shell(&mut shell, "row", "name", visit.wrapping_add(1)).is_err());
    let hover = shell.profile_hover.as_ref().unwrap();
    assert_eq!(
        (&hover.user_id, hover.x, hover.y),
        (&"U1".to_owned(), 120.0, 160.0)
    );
    assert_eq!(shell.profile_hover_generation, hover_generation);
    assert!(shell.profile_hover_card_active);
    assert!(!cell_editor_blocks_profile_hover(&shell));
}

#[test]
fn stale_disabled_and_saving_editors_keep_modal_hover_ownership() {
    let mut shell = shell();
    let generation = open(&mut shell, "number");
    shell.slack_lists.view.as_mut().unwrap().stale = true;
    assert!(presentation(&shell).unwrap().disabled);
    assert!(cell_editor_blocks_profile_hover(&shell));
    shell.slack_lists.view.as_mut().unwrap().stale = false;
    begin_save(
        &mut shell,
        generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    assert!(presentation(&shell).unwrap().saving);
    assert!(cell_editor_blocks_profile_hover(&shell));
    assert!(cancel_shell(&mut shell, generation));
    assert!(!cell_editor_blocks_profile_hover(&shell));
    assert!(shell.slack_lists.operations.is_empty());
}

#[test]
fn hidden_or_foreign_list_editor_does_not_suppress_other_surface_hover() {
    let mut shell = shell();
    open(&mut shell, "name");
    assert!(cell_editor_blocks_profile_hover(&shell));
    shell.overlay = Some(Overlay::Canvas);
    assert!(!cell_editor_blocks_profile_hover(&shell));
    shell.overlay = Some(Overlay::SlackList);
    assert!(cell_editor_blocks_profile_hover(&shell));
    shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
    assert!(!cell_editor_blocks_profile_hover(&shell));
}
