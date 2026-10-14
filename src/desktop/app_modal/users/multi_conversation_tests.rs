use super::super::{multi_conversation_fixture as fixture, requests};
use super::*;
use serde_json::json;
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn choose(s: &mut ShellState, block: &str, id: &str) {
    let t = Ticket::capture(s, &key(block)).unwrap();
    assert!(open_local(s, t.clone()));
    let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, revision) = (p.serial, p.query_revision);
    assert!(choose_local(s, &t, serial, revision, id));
}
#[test]
fn inline_conversations_allow_max_correction_optional_clear_and_preserve_dirty_arrays() {
    let mut s = setup();
    choose(&mut s, "reviewers", "U00000002");
    choose(&mut s, "reviewers", "G00000003");
    assert!(requests::prepare_submission(&s).is_err());
    let t = Ticket::capture(&s, &key("reviewers")).unwrap();
    assert!(remove_local(&mut s, &t, "G00000003"));
    choose(&mut s, "optional", "G00000003");
    let t = Ticket::capture(&s, &key("optional")).unwrap();
    assert!(remove_local(&mut s, &t, "G00000003"));
    let mut update = fixture::view();
    update["blocks"][1]["element"]["initial_conversations"] = json!(["G00000003"]);
    update["blocks"][2]["element"]["initial_conversations"] = json!(["G00000003"]);
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&update)
        .unwrap();
    let wire = requests::prepare_submission(&s).unwrap();
    assert_eq!(
        wire["values"]["reviewers"]["value"],
        json!({"type":"multi_conversations_select","selected_conversations":["C00000006","U00000002"]})
    );
    assert_eq!(
        wire["values"]["optional"]["value"],
        json!({"type":"multi_conversations_select","selected_conversations":[]})
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
#[test]
fn discovery_in_other_field_invalidates_retained_selection_even_after_other_field_removes_it() {
    let mut s = setup();
    let remote = |archived, updated| {
        serde_json::from_value(
            json!({"id":"U99999999","name":"remote","deleted":archived,"updated":updated}),
        )
        .unwrap()
    };
    for block in ["reviewers", "optional"] {
        let t = Ticket::capture(&s, &key(block)).unwrap();
        assert!(open_local(&mut s, t.clone()));
        let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
        let (serial, rev) = (p.serial, p.query_revision);
        assert!(complete_entities(
            &mut s,
            serial,
            rev,
            Ok((vec![remote(false, 1)], vec![]))
        ));
        assert!(choose_local(&mut s, &t, serial, rev, "U99999999"));
    }
    assert!(requests::prepare_submission(&s).is_ok());
    let t = Ticket::capture(&s, &key("optional")).unwrap();
    let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, rev) = (p.serial, p.query_revision);
    // Stale query metadata cannot invalidate another field either.
    assert!(!complete_entities(
        &mut s,
        serial,
        rev + 1,
        Ok((vec![remote(true, 2)], vec![]))
    ));
    assert!(requests::prepare_submission(&s).is_ok());
    assert!(complete_entities(
        &mut s,
        serial,
        rev,
        Ok((vec![remote(true, 2)], vec![]))
    ));
    assert!(remove_local(&mut s, &t, "U99999999"));
    let Err(super::super::FormError::Validation(errors)) = requests::prepare_submission(&s) else {
        panic!("retained first-field selection must fail");
    };
    assert!(errors.iter().any(|(k, _)| k == &key("reviewers")));
    assert!(!errors.iter().any(|(k, _)| k == &key("optional")));
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key("reviewers")].value,
        InputValue::Conversations(vec!["C00000006".into(), "U99999999".into()])
    );
}

#[test]
fn multi_conversation_origin_true_is_unsupported_and_never_defaults_from_modal_origin() {
    let mut s = setup();
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key("optional")].value,
        InputValue::Conversations(vec![])
    );
    let mut raw = fixture::view();
    raw["blocks"][1]["element"]["default_to_current_conversation"] = json!(true);
    s.app_modal = Some(super::super::Session::fixture_with_origin(
        raw,
        Some("C00000001".into()),
    ));
    assert!(
        !s.app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .unsupported
            .is_empty()
    );
    assert!(Ticket::capture(&s, &key("reviewers")).is_none());
    assert!(requests::prepare_submission(&s).is_err());
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
#[test]
fn per_field_filter_is_rechecked_for_all_selected_member_and_channel_records() {
    let mut s = setup();
    choose(&mut s, "reviewers", "G00000003");
    choose(&mut s, "optional", "U00000002");
    assert!(requests::prepare_submission(&s).is_ok());
    // Updated canonical member cannot be forwarded through a previously selected chip.
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("U00000002")
        .unwrap()
        .is_bot = true;
    assert!(requests::prepare_submission(&s).is_err());
    let ticket = Ticket::capture(&s, &key("optional")).unwrap();
    assert!(remove_local(&mut s, &ticket, "U00000002"));
    assert!(requests::prepare_submission(&s).is_ok());
    // The optional field excludes externally shared channels even if another field permits them.
    assert!(open_local(&mut s, ticket.clone()));
    let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, rev) = (p.serial, p.query_revision);
    assert!(!choose_local(&mut s, &ticket, serial, rev, "C00000006"));
}
#[test]
fn stale_context_query_and_unknown_outcome_keep_conversation_arrays_without_dispatch() {
    let mut s = setup();
    choose(&mut s, "optional", "U00000002");
    let t = Ticket::capture(&s, &key("optional")).unwrap();
    assert!(open_local(&mut s, t.clone()));
    let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, rev) = (p.serial, p.query_revision);
    s.app_modal.as_mut().unwrap().uncertain = true;
    assert!(!choose_local(&mut s, &t, serial, rev, "G00000003"));
    assert!(requests::prepare_submission(&s).is_err());
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key("optional")].value,
        InputValue::Conversations(vec!["U00000002".into()])
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
