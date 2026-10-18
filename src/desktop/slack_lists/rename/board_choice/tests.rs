use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn seed() -> (ShellState, Cleanup) {
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-choice",
    );
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn begin_check(shell: &ShellState, serial: u64) -> Option<Guard> {
    let key = Key::current(shell)?;
    let e = entry(shell)?;
    if e.serial != serial || !checkable(shell, &e) || !owner::readable(shell) {
        return None;
    }
    let guard = Guard::capture_read(shell, key.clone(), serial)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(guard.job);
        e.stage = Stage::Checking;
        e.review = None;
    });
    Some(guard)
}
fn edit(shell: &mut ShellState) -> registry::Entry {
    let choice = available(shell, Some("OPT_PROGRESS"), Kind::Rename).unwrap();
    assert!(open_shell(shell, choice));
    entry(shell).unwrap()
}
fn metadata(shell: &ShellState, document: Document) -> network::Metadata {
    let l = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    network::Metadata {
        document,
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: l.list_experiments,
    }
}
#[test]
fn draft_input_stays_owned_and_cancel_has_no_intent_or_request() {
    let (mut shell, _cleanup) = seed();
    let e = edit(&mut shell);
    let visit = generation(&shell).unwrap();
    let Input::BoardChoice(original) = e.input.clone() else {
        panic!()
    };
    let canonical = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        original.clone(),
        Some("I".into()),
        None
    ));
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        original.clone(),
        Some("In motion".into()),
        None
    ));
    let current = entry(&shell).unwrap();
    assert!(ready(&shell, &current));
    assert!(current.intent.is_none());
    assert!(current.job.is_none());
    assert!(close_entry(&mut shell, e.serial));
    assert!(entry(&shell).is_none());
    let after = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(after.document, canonical.document);
    assert_eq!(after.records, canonical.records);
}
#[test]
fn add_identity_is_stable_across_label_and_color_changes_and_stale_visit_refuses() {
    let (mut shell, _cleanup) = seed();
    let choice = available(&shell, None, Kind::Add).unwrap();
    assert!(open_shell(&mut shell, choice));
    let e = entry(&shell).unwrap();
    let visit = generation(&shell).unwrap();
    let Input::BoardChoice(original) = e.input.clone() else {
        panic!()
    };
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        original.clone(),
        Some(" New group ".into()),
        None
    ));
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        original.clone(),
        None,
        Some("purple".into())
    ));
    let Input::BoardChoice(BoardChoiceAction::Add {
        value,
        label,
        color,
    }) = entry(&shell).unwrap().input
    else {
        panic!()
    };
    let BoardChoiceAction::Add { value: before, .. } = original.clone() else {
        panic!()
    };
    assert_eq!(value, before);
    assert_eq!(label, " New group ");
    assert_eq!(color, "purple");
    assert!(!set_input(
        &shell,
        e.serial,
        visit + 1,
        original,
        Some("wrong".into()),
        None
    ));
}
#[test]
fn pending_find_retires_preparation() {
    let (mut shell, _cleanup) = seed();
    let e = edit(&mut shell);
    let visit = generation(&shell).unwrap();
    let Input::BoardChoice(action) = e.input else {
        panic!()
    };
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        action,
        Some("In motion".into()),
        None
    ));
    shell.slack_lists.view.as_mut().unwrap().layout.find.input = "pending".into();
    assert!(prepare(&mut shell, e.serial).is_none());
    assert_eq!(entry(&shell).unwrap().stage, Stage::Editing);
}
#[test]
fn current_readback_requires_authoritative_records_and_preserves_their_raw_shape() {
    let (mut shell, _cleanup) = seed();
    let e = edit(&mut shell);
    let visit = generation(&shell).unwrap();
    let Input::BoardChoice(action) = e.input else {
        panic!()
    };
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        action,
        Some("In motion".into()),
        None
    ));
    let (guard, intent) = prepare(&mut shell, e.serial).unwrap();
    let original = intent.original().clone();
    let permit = admitted(&shell, &guard, &intent, &metadata(&shell, original.clone())).unwrap();
    drop(permit);
    completed(
        &mut shell,
        &guard,
        Err(
            super_platinum_core::slack::transport::ListColumnRenameFailure::Unconfirmed(
                Error::Transport("lost reply".into()),
            ),
        ),
    );
    let check_guard = begin_check(&shell, e.serial).unwrap();
    let Intent::BoardChoice(intent) = intent else {
        panic!()
    };
    let mut raw = original.raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == intent.column_id())
        .unwrap()["options"] = intent.options().clone();
    let fresh = Document::parse(raw).unwrap();
    let meta = metadata(&shell, fresh.clone());
    assert!(!checked(&mut shell, &check_guard, Ok(meta)));
    assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
    let foreign_guard = begin_check(&shell, e.serial).unwrap();
    let mut foreign = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    foreign["list_id"] = json!("F_OTHER");
    let foreign = RecordsPage::parse(foreign).unwrap();
    let meta = metadata(&shell, fresh.clone());
    assert!(!checked_with_records(
        &mut shell,
        &foreign_guard,
        Ok(meta),
        Some(foreign)
    ));
    assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
    let check_guard = begin_check(&shell, e.serial).unwrap();
    let mut page = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    page["opaque_current_page"] = json!({"fresh":true});
    let records = RecordsPage::parse(page.clone()).unwrap();
    let meta = metadata(&shell, fresh.clone());
    assert!(checked_with_records(
        &mut shell,
        &check_guard,
        Ok(meta),
        Some(records)
    ));
    assert_eq!(entry(&shell).unwrap().stage, Stage::Observed);
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(loaded.document, fresh);
    assert_eq!(loaded.records.raw, page);
    assert_eq!(
        loaded.records.raw["response_metadata"]["next_cursor"],
        "precision-opaque+/="
    );
}

#[test]
fn old_rendered_confirmation_cannot_submit_new_label() {
    let (mut shell, _cleanup) = seed();
    let e = edit(&mut shell);
    let visit = generation(&shell).unwrap();
    let Input::BoardChoice(old) = e.input else {
        panic!()
    };
    assert!(set_input(
        &shell,
        e.serial,
        visit,
        old.clone(),
        Some("In motion".into()),
        None
    ));
    assert!(prepare_choice(&mut shell, e.serial, visit, &old).is_none());
    let Input::BoardChoice(current) = entry(&shell).unwrap().input else {
        panic!()
    };
    assert!(prepare_choice(&mut shell, e.serial, visit, &current).is_some());
    assert!(prepare_choice(&mut shell, e.serial, visit, &current).is_none());
}

#[test]
fn stale_control_cannot_open_after_raw_option_support_changes() {
    for change in 0..4 {
        let (mut shell, _cleanup) = seed();
        let choice = available(&shell, Some("OPT_PROGRESS"), Kind::Rename).unwrap();
        let loaded = shell
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        let mut raw = loaded.document.raw.clone();
        let column = raw["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["key"] == "status")
            .unwrap();
        let option = column["options"]["choices"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|c| c["value"] == "OPT_PROGRESS")
            .unwrap();
        match change {
            0 => {
                option.as_object_mut().unwrap().remove("label");
            }
            1 => {
                option["label"] = json!(17);
            }
            2 => {
                column["options"]["default_value_typed"] = json!({"unknown_default":true});
            }
            3 => {
                loaded.allow_lists = Some("false".into());
            }
            _ => unreachable!(),
        }
        loaded.document = Document::parse(raw).unwrap();
        let before = loaded.clone();
        assert!(choice.current(&shell), "layout ownership stays unchanged");
        assert!(available(&shell, Some("OPT_PROGRESS"), Kind::Rename).is_none());
        assert!(!open_shell(&mut shell, choice));
        assert!(entry(&shell).is_none());
        let after = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(after.document, before.document);
        assert_eq!(after.records, before.records);
    }
}

#[test]
fn captured_choice_cannot_rebind_to_a_replaced_scope_with_identical_loaded_data() {
    for kind in [Kind::Add, Kind::Rename, Kind::Color] {
        for case in 0..3 {
            let (mut shell, _cleanup) = seed();
            let value = (kind != Kind::Add).then_some("OPT_PROGRESS");
            let choice = available(&shell, value, kind).unwrap();
            let view = shell.slack_lists.view.as_ref().unwrap();
            let before = (
                view.loaded.as_ref().unwrap().document.clone(),
                view.generation,
                view.layout.generation,
            );
            match case {
                0 => shell.core.account_epoch += 1,
                1 => {}
                _ => shell.core.active_account = Some("replacement-choice-account".into()),
            }
            shell.slack_lists.scope = Scope::capture(&shell);
            assert!(available(&shell, value, kind).is_some());
            let view = shell.slack_lists.view.as_ref().unwrap();
            assert_eq!(view.loaded.as_ref().unwrap().document, before.0);
            assert_eq!(view.generation, before.1);
            assert_eq!(view.layout.generation, before.2);
            assert!(!choice.current(&shell));
            assert!(!open_shell(&mut shell, choice));
            assert!(entry(&shell).is_none());
        }
    }
}
