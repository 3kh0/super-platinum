use super::super::tests::{TS, generation, load, raw, setup};
use super::*;

fn ready() -> (ShellState, Arc<Loaded>) {
    let mut shell = setup();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(4))]));
    let loaded = crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    (shell, loaded)
}
fn selection(loaded: Arc<Loaded>, name: &str) -> Selection {
    Selection {
        name: name.into(),
        query: name.into(),
        loaded,
    }
}
fn start(shell: &mut ShellState, loaded: Arc<Loaded>, name: &str) -> Work {
    let generation = generation(shell);
    let token = open_shell(shell, generation, TS).unwrap();
    let choice = admit(shell, generation, token, &selection(loaded, name)).unwrap();
    let final_name = choice.intent.name().to_owned();
    let work = model::begin_with(shell, generation, TS, &final_name, Some(choice)).unwrap();
    assert!(close_shell(shell, generation, token));
    work
}
fn intent(shell: &ShellState, work: &Work) -> ReactionIntent {
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    ReactionIntent::capture_choice(
        &entry.target,
        entry.page.as_ref().unwrap().root().unwrap(),
        message(entry, TS).unwrap(),
        json!({"id":entry.target.channel_id()}),
        &work.user,
        work.choice.as_ref().unwrap().intent.clone(),
    )
    .unwrap()
}
#[test]
fn first_picker_add_and_exact_remove_do_not_remove_owned_siblings() {
    let (mut shell, _) = ready();
    let user = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .owner
        .scope
        .stable_owner()
        .2;
    let mut row = raw(vec![]);
    row["reactions"] = json!([
        {"name":"wave::skin-tone-2","users":[user],"count":1},
        {"name":"thumbsup::skin-tone-4","users":[user],"count":1}]);
    load(&mut shell, row);
    let loaded = crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    let work = start(&mut shell, loaded.clone(), "wave");
    assert_eq!(work.name, "wave::skin-tone-4");
    assert!(work.adding && work.family.is_none());
    let current = intent(&shell, &work);
    work.update(|a| {
        a.record = Some(current.record().clone());
        a.phase = Phase::Sent;
    });
    settle(&work, Ok(json!({"ok":true})));
    finish(&mut shell, &work);
    let removal = start(&mut shell, loaded, "wave");
    assert!(!removal.adding && removal.family.is_none());
    let current = intent(&shell, &removal);
    removal.update(|a| {
        a.record = Some(current.record().clone());
        a.phase = Phase::Sent;
    });
    settle(&removal, Ok(json!({"ok":true})));
    finish(&mut shell, &removal);
    let row = message(shell.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap();
    assert!(member(row, &user, "wave::skin-tone-2"));
    assert!(member(row, &user, "thumbsup::skin-tone-4"));
    assert!(!member(row, &user, "wave::skin-tone-4"));
}
#[test]
fn cold_or_partial_preferences_are_not_selection_authority() {
    let mut shell = setup();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("preferred_skin_tone", json!(4));
    let loaded = crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    let generation = generation(&shell);
    let token = open_shell(&mut shell, generation, TS).unwrap();
    assert!(!presentation(&shell).unwrap().2);
    assert!(admit(&shell, generation, token, &selection(loaded, "wave")).is_none());
    assert!(close_shell(&mut shell, generation, token));
}
#[test]
fn catalog_allocation_preferences_and_original_row_changes_refuse_open_selection() {
    for change in 0..6 {
        let (mut shell, loaded) = ready();
        let generation = generation(&shell);
        let token = open_shell(&mut shell, generation, TS).unwrap();
        assert!(
            admit(
                &shell,
                generation,
                token,
                &selection(loaded.clone(), "wave")
            )
            .is_some()
        );
        match change {
            0 => {
                crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
            }
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .emoji_preferences
                    .apply_change("preferred_skin_tone", json!(3));
            }
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .custom_emoji_revision += 1;
            }
            3 => {
                shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
            }
            4 => {
                shell.core.account_epoch += 1;
            }
            _ => {
                shell
                    .slack_lists
                    .comments
                    .entry
                    .as_mut()
                    .unwrap()
                    .deleted
                    .insert(TS.into());
            }
        }
        assert!(
            admit(&shell, generation, token, &selection(loaded, "wave")).is_none(),
            "change {change}"
        );
    }
}
#[test]
fn choice_observation_remains_required_through_preflight_not_just_callback() {
    for change in 0..4 {
        let (mut shell, loaded) = ready();
        let work = start(&mut shell, loaded, "wave");
        assert!(work.current(&shell));
        match change {
            0 => {
                crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
            }
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .emoji_preferences
                    .apply_change("preferred_skin_tone", json!(2));
            }
            2 => {
                shell.core.account_epoch += 1;
            }
            _ => {
                shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
            }
        }
        assert!(!work.current(&shell), "change {change}");
        assert!(work.attempt().unwrap().raw.is_none());
    }
}
#[test]
fn stale_close_and_choice_never_consume_successor_picker() {
    let (mut shell, loaded) = ready();
    let generation = generation(&shell);
    let first = open_shell(&mut shell, generation, TS).unwrap();
    assert!(close_shell(&mut shell, generation, first));
    let second = open_shell(&mut shell, generation, TS).unwrap();
    assert_ne!(first, second);
    assert!(!close_shell(&mut shell, generation, first));
    assert!(
        admit(
            &shell,
            generation,
            first,
            &selection(loaded.clone(), "wave")
        )
        .is_none()
    );
    assert!(admit(&shell, generation, second, &selection(loaded, "wave")).is_some());
}
#[test]
fn unknown_picker_tone_blocks_subsequent_family_removal_without_catalog_in_debt() {
    let (mut shell, loaded) = ready();
    let work = start(&mut shell, loaded, "wave");
    let current = intent(&shell, &work);
    work.update(|a| {
        a.record = Some(current.record().clone());
        a.phase = Phase::Unknown;
    });
    finish(&mut shell, &work);
    assert!(!can_dispatch(&shell, generation(&shell), TS, &work.name));
    let mut row = raw(vec![]);
    row["reactions"] = json!([
        {"name":"wave::skin-tone-2","users":[work.user],"count":1},
        {"name":"wave::skin-tone-4","users":["U_OTHER"],"count":1}]);
    load(&mut shell, row);
    assert!(!super::super::available(
        &shell,
        generation(&shell),
        TS,
        "wave::skin-tone-2"
    ));
    let serialized = snapshot(&shell);
    assert!(serialized["notice"].is_null()); // picker dismissed; action debt stays separate.
    assert_eq!(
        super::super::snapshot(&shell)["attempts"][0]["name"],
        "wave::skin-tone-4"
    );
}

#[test]
fn opening_cannot_adopt_changed_observations_after_native_await() {
    let (mut shell, _) = ready();
    let generation = generation(&shell);
    let captured = capture_picker(&shell, generation, TS).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("preferred_skin_tone", json!(5));
    assert!(install_picker(&mut shell, captured).is_none());
    assert!(!has_picker(&shell));
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn open_cancel_and_retirement_preserve_original_draft_inventory() {
    let (mut shell, binding, _local) = draft::tests::setup();
    load(&mut shell, raw(vec!["U_OTHER".into()]));
    let before = {
        let e = draft::entry(&shell, binding.id()).unwrap();
        json!({"blocks":e.blocks,"generation":e.generation,"capture":e.capture,"sequence":e.sequence,
            "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let generation = generation(&shell);
    let token = open_shell(&mut shell, generation, TS).unwrap();
    assert!(close_shell(&mut shell, generation, token));
    let next = open_shell(&mut shell, generation, TS).unwrap();
    controller::close_shell(&mut shell);
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .reaction_picker
            .is_none()
    );
    assert!(!close_shell(&mut shell, generation, next));
    let e = draft::entry(&shell, binding.id()).unwrap();
    assert_eq!(
        before,
        json!({"blocks":e.blocks,"generation":e.generation,"capture":e.capture,"sequence":e.sequence,
        "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    );
}
