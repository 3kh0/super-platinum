use super::*;

fn setup() -> (ShellState, u64, u64, u64, Choice) {
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-rating-field-create",
    );
    let serial = entry(&s).unwrap().serial;
    let visit = super::super::super::super::generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let choice = choice(&s).unwrap();
    (s, serial, visit, selection, choice)
}
#[test]
fn quantity_type_and_picker_epochs_refuse_old_callbacks_and_keep_same_render_name_events() {
    let (s, serial, visit, selection, first) = setup();
    assert!(input_shell(&s, serial, visit, selection, first, "Q".into()));
    assert!(input_shell(
        &s,
        serial,
        visit,
        selection,
        first,
        "Quality".into()
    ));
    assert_eq!(choice(&s), Some(first));
    for max in [1, 2, 3, 4, 5] {
        let before = choice(&s).unwrap();
        assert!(quantity_shell(
            &s,
            serial,
            visit,
            selection,
            before,
            &max.to_string()
        ));
        assert_eq!(entry(&s).unwrap().rating_draft.max(), max);
        assert!(!input_shell(
            &s,
            serial,
            visit,
            selection,
            before,
            "old".into()
        ));
    }
    let current = choice(&s).unwrap();
    let token = open_shell(&s, serial, visit, selection, current).unwrap();
    assert!(open_shell(&s, serial, visit, selection, current).is_none());
    assert!(!quantity_shell(&s, serial, visit, selection, current, "2"));
    assert!(close_shell(&s, serial, visit, selection, current, token));
    let newer = open_shell(&s, serial, visit, selection, current).unwrap();
    assert_ne!(newer, token);
    let team = s.core.active_team.as_deref().unwrap();
    assert!(!apply_emoji(
        &s, serial, visit, selection, current, token, ":heart:", team
    ));
    assert!(apply_emoji(
        &s,
        serial,
        visit,
        selection,
        current,
        newer,
        ":fixture_custom:",
        team
    ));
    assert_eq!(
        entry(&s).unwrap().rating_draft.wire(),
        serde_json::json!({"emoji":":fixture_custom:","max":5,"emoji_team_id":team})
    );
    assert!(!apply_emoji(
        &s, serial, visit, selection, current, newer, ":heart:", team
    ));
    let custom = choice(&s).unwrap();
    assert!(!kind_shell(&s, serial, visit, selection, custom, "rating"));
    assert!(kind_shell(&s, serial, visit, selection, custom, "text"));
    assert!(kind_shell(
        &s,
        serial,
        visit,
        selection,
        choice(&s).unwrap(),
        "rating"
    ));
    assert_eq!(entry(&s).unwrap().rating_draft, RatingOptions::default());
    assert!(!quantity_shell(&s, serial, visit, selection, custom, "2"));
    assert!(close_entry(&s, serial));
    assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
}
#[test]
fn picker_blocks_admission_and_configured_intent_owns_options_after_prepare() {
    let (mut s, serial, visit, selection, first) = setup();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    assert!(quantity_shell(&s, serial, visit, selection, first, "3"));
    let current = choice(&s).unwrap();
    let token = open_shell(&s, serial, visit, selection, current).unwrap();
    assert!(prepare(&mut s, serial).is_none());
    assert!(close_shell(&s, serial, visit, selection, current, token));
    let token = open_shell(&s, serial, visit, selection, current).unwrap();
    let team = s.core.active_team.clone().unwrap();
    assert!(!apply_emoji(
        &s, serial, visit, selection, current, token, ":heart:", "OTHER"
    ));
    assert!(apply_emoji(
        &s, serial, visit, selection, current, token, ":heart:", &team
    ));
    let locked = choice(&s).unwrap();
    let (_, intent) = prepare(&mut s, serial).unwrap();
    let Intent::NewTextField(intent) = intent else {
        panic!("shared creation intent")
    };
    assert_eq!(
        intent.options(),
        Some(serde_json::json!({"emoji":":heart:","max":3,"emoji_team_id":team}))
    );
    assert_eq!(intent.original_records(), &before.records);
    assert!(!quantity_shell(&s, serial, visit, selection, locked, "4"));
    assert!(open_shell(&s, serial, visit, selection, locked).is_none());
    assert!(!input_shell(
        &s,
        serial,
        visit,
        selection,
        locked,
        "changed".into()
    ));
    // No executor ran: this test ends at retained intent admission custody.
    assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
    super::super::super::with_registry(|r| {
        r.entries
            .remove(&super::super::super::Key::current(&s).unwrap());
    });
}

#[test]
fn exact_picker_dismissal_survives_stale_population_and_selection_without_admitting_writes() {
    for reason in 0..3 {
        let (mut s, serial, visit, selection, rendered) = setup();
        let token = open_shell(&s, serial, visit, selection, rendered).unwrap();
        match reason {
            0 => s.slack_lists.view.as_mut().unwrap().stale = true,
            1 => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = l.records.raw.clone();
                raw["response_metadata"]["next_cursor"] = serde_json::json!("changed");
                l.records = RecordsPage::parse(raw).unwrap();
            }
            _ => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
        }
        let before = entry(&s).unwrap();
        assert!(!quantity_shell(&s, serial, visit, selection, rendered, "2"));
        assert!(prepare(&mut s, serial).is_none());
        assert!(close_shell(&s, serial, visit, selection, rendered, token));
        assert!(!close_shell(&s, serial, visit, selection, rendered, token));
        let after = entry(&s).unwrap();
        assert_eq!(after.rating_picker, None);
        assert_eq!(after.rating_draft, before.rating_draft);
        assert_eq!(after.field_choice, before.field_choice);
        assert!(after.intent.is_none() && after.job.is_none() && after.stage == Stage::Editing);
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        assert!(close_entry(&s, serial));
    }
}
#[test]
fn dismissal_cannot_consume_a_replacement_owner_visit_serial_or_picker_token() {
    let (mut s, serial, visit, selection, rendered) = setup();
    let old = open_shell(&s, serial, visit, selection, rendered).unwrap();
    assert!(close_shell(&s, serial, visit, selection, rendered, old));
    let token = open_shell(&s, serial, visit, selection, rendered).unwrap();
    assert!(!close_shell(&s, serial, visit, selection, rendered, old));
    assert!(!close_shell(
        &s,
        serial + 1,
        visit,
        selection,
        rendered,
        token
    ));
    assert!(!close_shell(
        &s,
        serial,
        visit + 1,
        selection,
        rendered,
        token
    ));
    let epoch = s.core.account_epoch;
    s.core.account_epoch += 1;
    assert!(!close_shell(&s, serial, visit, selection, rendered, token));
    s.core.account_epoch = epoch;
    assert_eq!(entry(&s).unwrap().rating_picker, Some(token));
    assert!(close_shell(&s, serial, visit, selection, rendered, token));
    assert!(close_entry(&s, serial));
}
