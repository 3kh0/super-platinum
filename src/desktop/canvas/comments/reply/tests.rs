use super::*;
pub(super) fn setup() -> (ShellState, State, Binding) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(fixture::ACCOUNT.into());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Canvas".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    let owner = Owner::capture(&shell).unwrap();
    let binding = Binding {
        id: "entry".into(),
        owner: owner.clone(),
        read: 7,
        root: fixture::TS.into(),
    };
    let disk = Disk {
        account: fixture::ACCOUNT.into(),
        scope: DiskScope {
            workspace: owner.scope.team.clone(),
            user: shell.self_account.user_id.clone(),
            file: owner.file.clone(),
        },
        path: None,
    };
    shell.canvas_checklist.comment_replies.entries.push(Entry {
        id: binding.id.clone(),
        disk,
        root: binding.root.clone(),
        text: "original".into(),
        blocks: None,
        native: None,
        serial: 5,
        capture: uuid::Uuid::new_v4().to_string(),
        inventory: None,
        loading: false,
        saving: false,
        sending: true,
        composing: false,
        native_busy: false,
        error: None,
        reviewed: false,
        outcome: None,
        locked: None,
        history: None,
        retired_text: None,
        retired_blocks: None,
    });
    let model = State {
        owner: Some(owner),
        serial: 7,
        query: Some(Query::Replies(fixture::TS.into())),
        ..Default::default()
    };
    (shell, model, binding)
}
#[test]
fn request_fences_query_visit_account_and_newer_edit() {
    let (mut shell, mut model, b) = setup();
    assert!(delivery::current(&shell, &model, &b, 5));
    model.serial += 1;
    assert!(!delivery::current(&shell, &model, &b, 5));
    model.serial -= 1;
    entry_mut(&mut shell, &b.id).unwrap().serial += 1;
    assert!(!delivery::current(&shell, &model, &b, 5));
    entry_mut(&mut shell, &b.id).unwrap().serial = 5;
    shell.shortcut_generation += 1;
    assert!(!delivery::current(&shell, &model, &b, 5));
    shell.shortcut_generation -= 1;
    shell.core.account_epoch += 1;
    assert!(!delivery::current(&shell, &model, &b, 5));
}
#[test]
fn pending_owner_survives_epoch_rotation_but_not_account_replacement() {
    let (mut shell, _, _) = setup();
    assert!(pending_for_current_file(&shell));
    shell.core.account_epoch += 1;
    shell.shortcut_generation += 1;
    assert!(pending_for_current_file(&shell));
    shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    assert!(!pending_for_current_file(&shell));
}
#[test]
fn root_custody_cannot_evict_unsaved_failed_or_inflight_content() {
    let (shell, _, b) = setup();
    let e = entry(&shell, &b.id).unwrap();
    assert!(!durably_idle(e));
    let (mut shell, _, b) = setup();
    let e = entry_mut(&mut shell, &b.id).unwrap();
    e.sending = false;
    e.error = Some("retained");
    assert!(!durably_idle(e));
}
#[test]
fn retirement_does_not_reclassify_newer_text() {
    let (mut shell, _, b) = setup();
    let e = entry_mut(&mut shell, &b.id).unwrap();
    delivery::retire_exact(e, 5, "original");
    assert_eq!(e.retired_text.as_deref(), Some("original"));
    e.retired_text = None;
    e.text = "newer".into();
    e.serial = 6;
    delivery::retire_exact(e, 5, "original");
    assert!(e.retired_text.is_none());
}

#[test]
fn changed_native_receipt_is_owned_after_navigation_and_refuses_newer_serial() {
    let (mut shell, _, b) = setup();
    shell.shortcut_generation += 1;
    let e = entry_mut(&mut shell, &b.id).unwrap();
    assert!(delivery::adopt_dom(
        e,
        5,
        "old-field",
        crate::view::composer::ScheduledClearReceipt::Changed {
            scope: "old-field".into(),
            text: "DOM ahead".into()
        }
    ));
    assert_eq!(e.text, "DOM ahead");
    assert!(!delivery::adopt_dom(
        e,
        5,
        "old-field",
        crate::view::composer::ScheduledClearReceipt::Cleared {
            scope: "old-field".into()
        }
    ));
    assert_eq!(e.text, "DOM ahead");
    e.composing = true;
    let serial = e.serial;
    assert!(!delivery::adopt_dom(
        e,
        serial,
        "old-field",
        crate::view::composer::ScheduledClearReceipt::Changed {
            scope: "old-field".into(),
            text: "provisional".into()
        }
    ));
    assert_eq!(e.text, "DOM ahead");
}

#[test]
fn shell_replacement_preserves_unsaved_tail_and_exact_late_native_custody() {
    let (mut previous, model, binding) = setup();
    let original_capture = {
        let e = entry_mut(&mut previous, &binding.id).unwrap();
        e.text = "newer input while an older disk write is in flight".into();
        e.serial = 6;
        e.saving = true;
        e.native_busy = true;
        e.error = Some("local save failed");
        e.capture.clone()
    };
    let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
    loaded.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    super::super::carry_pending(&mut previous, &mut loaded);
    assert!(previous.canvas_checklist.comment_replies.entries.is_empty());
    let e = entry(&loaded, &binding.id).unwrap();
    assert_eq!(e.text, "newer input while an older disk write is in flight");
    assert_eq!(e.capture, original_capture);
    assert_eq!(e.serial, 6);
    assert!(e.saving && e.native_busy && e.sending);
    assert_eq!(e.error, Some("local save failed"));
    assert!(!binding.current(&loaded, &model));
    assert!(!delivery::current(&loaded, &model, &binding, 6));
    assert!(!pending_for_current_file(&loaded));
    // A late result from the original exact node still has a retained destination.
    let e = entry_mut(&mut loaded, &binding.id).unwrap();
    assert!(delivery::adopt_dom(
        e,
        6,
        "old-native-scope",
        crate::view::composer::ScheduledClearReceipt::Changed {
            scope: "old-native-scope".into(),
            text: "late captured original tail".into(),
        }
    ));
    assert_eq!(e.text, "late captured original tail");
    let mut again = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::carry_pending(&mut loaded, &mut again);
    assert_eq!(
        entry(&again, &binding.id).unwrap().text,
        "late captured original tail"
    );
    assert!(loaded.canvas_checklist.comment_replies.entries.is_empty());
}

#[test]
fn carry_keeps_distinct_retained_reply_entries_without_replacing_loaded_state() {
    let (mut previous, _, old) = setup();
    let (mut loaded, _, _) = setup();
    loaded.canvas_checklist.comment_replies.entries[0].id = "already-retained-other".into();
    loaded.canvas_checklist.comment_replies.entries[0].text = "other retained text".into();
    loaded.shortcut_generation = 91;
    super::super::carry_pending(&mut previous, &mut loaded);
    assert_eq!(loaded.shortcut_generation, 91);
    assert_eq!(loaded.canvas_checklist.comment_replies.entries.len(), 2);
    assert_eq!(entry(&loaded, &old.id).unwrap().text, "original");
    assert_eq!(
        entry(&loaded, "already-retained-other").unwrap().text,
        "other retained text"
    );
}
