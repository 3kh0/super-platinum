use super::super::tests::{TS, generation, load, raw, setup};
use super::*;

fn row(shell: &mut ShellState, name: &str) {
    let mut value = raw(vec![]);
    value["reactions"] = json!([{"name":name,"count":1,"users":["U_OTHER"]}]);
    load(shell, value);
}
fn ready(name: &str) -> ShellState {
    let mut shell = setup();
    row(&mut shell, name);
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(4))]));
    crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    shell
}
fn pending(shell: &mut ShellState, name: &str) -> Pending {
    let p = Pending::capture(shell, generation(shell), TS, name).unwrap();
    assert!(install(shell, &p));
    p
}
fn intent(shell: &ShellState, choice: &Witness) -> ReactionIntent {
    let e = shell.slack_lists.comments.entry.as_ref().unwrap();
    ReactionIntent::capture_choice(
        &e.target,
        e.page.as_ref().unwrap().root().unwrap(),
        message(e, TS).unwrap(),
        json!({"id":e.target.channel_id()}),
        &e.owner.scope.stable_owner().2,
        choice.intent.clone(),
    )
    .unwrap()
}
#[test]
fn pill_resolves_preferred_multi_tone_and_preserves_existing_variants_and_aliases() {
    for (requested, expected) in [
        ("wave", "wave::skin-tone-4"),
        ("wave::skin-tone-3", "wave::skin-tone-3"),
        ("handshake", "handshake::skin-tone-4"),
        ("handshake::skin-tone-2-6", "handshake::skin-tone-2-6"),
        ("thumbsup", "thumbsup::skin-tone-4"),
        ("+1", "+1::skin-tone-4"),
    ] {
        let mut shell = ready(requested);
        let p = pending(&mut shell, requested);
        let choice = resolve(&shell, &p).unwrap().unwrap();
        assert_eq!(choice.intent.name(), expected, "{requested}");
        assert!(intent(&shell, &choice).record().adding());
        let work = model::begin_with(
            &mut shell,
            p.source.owner.serial,
            TS,
            expected,
            Some(choice),
        )
        .unwrap();
        assert!(work.family.is_none() && work.adding && work.current(&shell));
        assert!(
            shell
                .slack_lists
                .comments
                .entry
                .as_ref()
                .unwrap()
                .reaction_addition
                .is_none()
        );
        assert_eq!(work.attempt().unwrap().phase.label(), "preparing");
    }
}
#[test]
fn cold_invocation_requires_complete_observations_and_never_invents_a_request() {
    let mut shell = setup();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences = Preferences::default();
    let p = pending(&mut shell, "wave");
    assert!(resolve(&shell, &p).unwrap().is_none());
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    assert!(resolve(&shell, &p).unwrap().is_none());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(4))]));
    let choice = resolve(&shell, &p).unwrap().unwrap();
    assert_eq!(choice.intent.name(), "wave::skin-tone-4");
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn cold_partial_change_then_complete_load_cannot_authorize_original_invocation() {
    let mut shell = setup();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences = Preferences::default();
    let p = pending(&mut shell, "wave");
    let original_revision = p.seed.preferences.revision();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .apply_change("preferred_skin_tone", json!(2));
    assert_eq!(resolve(&shell, &p).err(), Some(CHANGED));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .replace(&BTreeMap::from([("preferred_skin_tone".into(), json!(4))]));
    crate::emoji_catalog::install_reaction_test_catalog(&mut shell);
    assert_eq!(
        shell.core.workspaces["T1"].emoji_preferences.revision(),
        original_revision.wrapping_add(2)
    );
    assert_eq!(resolve(&shell, &p).err(), Some(CHANGED));
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    p.fail(CHANGED);
    let successor = pending(&mut shell, "wave");
    assert_eq!(
        resolve(&shell, &successor).unwrap().unwrap().intent.name(),
        "wave::skin-tone-4"
    );
}
#[test]
fn original_ready_observation_cannot_be_replaced_while_native_capture_or_acquisition_waits() {
    for change in 0..5 {
        let mut shell = ready("wave");
        let p = Pending::capture(&shell, generation(&shell), TS, "wave").unwrap();
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
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .custom_emoji_revision += 1
            }
            3 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            _ => shell.core.account_epoch += 1,
        }
        assert!(!install(&mut shell, &p), "change {change}");
        assert!(
            super::super::snapshot(&shell)["attempts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn same_ts_membership_change_never_converts_original_addition_into_removal() {
    let mut shell = ready("wave");
    let p = pending(&mut shell, "wave");
    let user = p.source.owner.scope.stable_owner().2;
    let event = RtEvent::ReactionAdded {
        channel: p.source.target.channel_id().into(),
        ts: TS.into(),
        user,
        reaction: "wave::skin-tone-4".into(),
    };
    assert!(reactions::apply(&mut shell, "T1", 7, &event));
    assert!(resolve(&shell, &p).is_err());
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn failure_releases_only_original_slot_and_needs_explicit_retry() {
    let mut shell = ready("wave");
    let first = pending(&mut shell, "wave");
    assert!(busy(&shell));
    assert!(!super::super::available(
        &shell,
        generation(&shell),
        TS,
        "wave"
    ));
    first.fail(FAILED);
    assert!(!busy(&shell));
    assert_eq!(status(&shell, generation(&shell)).as_deref(), Some(FAILED));
    assert!(super::super::available(
        &shell,
        generation(&shell),
        TS,
        "wave"
    ));
    assert!(
        super::super::snapshot(&shell)["attempts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let second = pending(&mut shell, "wave");
    assert_ne!(first.token, second.token);
    first.fail(CHANGED);
    assert!(second.current(&shell));
    assert_eq!(status(&shell, generation(&shell)).as_deref(), Some(LOADING));
}
#[test]
fn dropped_task_and_closed_visit_cannot_strand_or_revive_original_loading() {
    let mut shell = ready("wave");
    let p = pending(&mut shell, "wave");
    drop(Completion(p.clone()));
    assert!(!busy(&shell));
    let next = pending(&mut shell, "wave");
    controller::close_shell(&mut shell);
    assert!(!next.installed(&shell));
    assert!(
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .reaction_addition
            .is_none()
    );
    assert!(snapshot(&shell).is_null());
}
#[test]
fn uncertain_resolved_tone_blocks_base_pill_retry_but_keeps_original_raw_receipt() {
    let mut shell = ready("wave");
    let p = pending(&mut shell, "wave");
    let choice = resolve(&shell, &p).unwrap().unwrap();
    let intent = intent(&shell, &choice);
    let name = choice.intent.name().to_owned();
    let work =
        model::begin_with(&mut shell, p.source.owner.serial, TS, &name, Some(choice)).unwrap();
    work.update(|a| {
        a.record = Some(intent.record().clone());
        a.phase = Phase::Sent;
    });
    let raw = json!({"ok":"true","opaque":"original malformed receipt"});
    settle(&work, Ok(raw.clone()));
    finish(&mut shell, &work);
    assert_eq!(work.attempt().unwrap().phase.label(), "unknown");
    assert!(!super::super::available(
        &shell,
        generation(&shell),
        TS,
        "wave"
    ));
    assert_eq!(work.attempt().unwrap().raw, Some(raw));
    assert!(!member(
        message(shell.slack_lists.comments.entry.as_ref().unwrap(), TS).unwrap(),
        &work.user,
        &name
    ));
}
#[test]
fn native_draft_inventory_and_page_raw_are_not_changed_by_loading_failure_or_retry() {
    let (mut shell, binding, _local) = draft::tests::setup();
    load(&mut shell, raw(vec!["U_OTHER".into()]));
    let draft_value = |s: &ShellState| {
        let e = draft::entry(s, binding.id()).unwrap();
        json!({"blocks":e.blocks,
        "generation":e.generation,"capture":e.capture,"sequence":e.sequence,
        "inventory":e.inventory.as_ref().map(|i|(i.blocks(),i.revision(),i.attempt()))})
    };
    let before = draft_value(&shell);
    let page = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .unwrap()
        .page
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    let first = pending(&mut shell, "wave");
    first.fail(FAILED);
    let _second = pending(&mut shell, "wave");
    assert_eq!(before, draft_value(&shell));
    assert_eq!(
        &page,
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap()
            .raw()
    );
}
