//! Shared emoji-picker ownership exercised with actual Vote creation entries.
use super::*;
use serde_json::json;
fn setup() -> (ShellState, u64, u64, u64, Choice) {
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-vote-field-create",
    );
    let serial = entry(&s).unwrap().serial;
    let visit = super::super::super::super::generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let choice = choice(&s).unwrap();
    (s, serial, visit, selection, choice)
}
#[test]
fn vote_defaults_type_aba_and_same_render_names_keep_typed_picker_custody() {
    let (s, serial, visit, selection, first) = setup();
    assert_eq!(first.kind, ColumnCreateKind::Vote);
    assert_eq!(entry(&s).unwrap().rating_draft.emoji(), ":sparkling_heart:");
    assert_eq!(entry(&s).unwrap().rating_draft.team_id(), None);
    assert!(input_shell(&s, serial, visit, selection, first, "S".into()));
    assert!(input_shell(
        &s,
        serial,
        visit,
        selection,
        first,
        "Support".into()
    ));
    assert!(!quantity_shell(&s, serial, visit, selection, first, "5"));
    assert_eq!(choice(&s), Some(first));
    let token = open_shell(&s, serial, visit, selection, first).unwrap();
    assert!(close_shell(&s, serial, visit, selection, first, token));
    let replacement = open_shell(&s, serial, visit, selection, first).unwrap();
    assert_ne!(token, replacement);
    let team = s.core.active_team.as_deref().unwrap();
    assert!(!apply_emoji(
        &s, serial, visit, selection, first, token, ":heart:", team
    ));
    assert!(apply_emoji(
        &s,
        serial,
        visit,
        selection,
        first,
        replacement,
        ":heart:",
        team
    ));
    let picked = choice(&s).unwrap();
    assert_ne!(picked.revision, first.revision);
    assert!(!input_shell(
        &s,
        serial,
        visit,
        selection,
        first,
        "stale".into()
    ));
    assert!(!kind_shell(&s, serial, visit, selection, picked, "vote"));
    assert_eq!(entry(&s).unwrap().rating_draft.emoji(), ":heart:");
    assert!(kind_shell(&s, serial, visit, selection, picked, "rating"));
    let rating = choice(&s).unwrap();
    assert_eq!(entry(&s).unwrap().rating_draft, RatingOptions::default());
    assert!(kind_shell(&s, serial, visit, selection, rating, "vote"));
    let final_choice = choice(&s).unwrap();
    assert_ne!(final_choice.revision, first.revision);
    assert_eq!(entry(&s).unwrap().rating_draft.emoji(), ":sparkling_heart:");
    assert_eq!(entry(&s).unwrap().rating_draft.team_id(), None);
    assert!(close_entry(&s, serial));
}
#[test]
fn vote_default_standard_custom_intents_own_no_quantity_and_freeze_after_admission() {
    for emoji in [None, Some(":heart:"), Some(":fixture_custom:")] {
        let (mut s, serial, visit, selection, first) = setup();
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let team = s.core.active_team.clone().unwrap();
        if let Some(emoji) = emoji {
            let token = open_shell(&s, serial, visit, selection, first).unwrap();
            assert!(prepare(&mut s, serial).is_none());
            assert!(!apply_emoji(
                &s, serial, visit, selection, first, token, emoji, "OTHER"
            ));
            assert!(apply_emoji(
                &s, serial, visit, selection, first, token, emoji, &team
            ));
        }
        let locked = choice(&s).unwrap();
        let (_, intent) = prepare(&mut s, serial).unwrap();
        let Intent::NewTextField(intent) = intent else {
            panic!("creation")
        };
        let options = match emoji {
            None => json!({"emoji":":sparkling_heart:"}),
            Some(emoji) => json!({"emoji":emoji,"emoji_team_id":team}),
        };
        assert_eq!(intent.kind(), ColumnCreateKind::Vote);
        assert_eq!(intent.options(), Some(options));
        assert_eq!(intent.original_records(), &before.records);
        assert!(!input_shell(
            &s,
            serial,
            visit,
            selection,
            locked,
            "changed".into()
        ));
        assert!(open_shell(&s, serial, visit, selection, locked).is_none());
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        super::super::super::with_registry(|r| {
            r.entries
                .remove(&super::super::super::Key::current(&s).unwrap());
        });
    }
}
#[test]
fn vote_stale_picker_can_dismiss_but_retired_owner_or_token_cannot() {
    let (mut s, serial, visit, selection, c) = setup();
    let token = open_shell(&s, serial, visit, selection, c).unwrap();
    let epoch = s.core.account_epoch;
    s.core.account_epoch += 1;
    assert!(!close_shell(&s, serial, visit, selection, c, token));
    s.core.account_epoch = epoch;
    s.slack_lists.view.as_mut().unwrap().stale = true;
    assert!(prepare(&mut s, serial).is_none());
    assert!(close_shell(&s, serial, visit, selection, c, token));
    assert!(!close_shell(&s, serial, visit, selection, c, token));
    assert!(close_entry(&s, serial));
}
