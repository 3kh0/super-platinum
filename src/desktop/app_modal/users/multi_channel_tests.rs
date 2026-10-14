use super::super::{multi_channel_fixture as fixture, requests};
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
fn inline_channels_allow_max_correction_optional_clear_and_preserve_dirty_arrays() {
    let mut s = setup();
    choose(&mut s, "reviewers", "C00000002");
    choose(&mut s, "reviewers", "C00000001");
    assert!(requests::prepare_submission(&s).is_err());
    let t = Ticket::capture(&s, &key("reviewers")).unwrap();
    assert!(remove_local(&mut s, &t, "C00000001"));
    choose(&mut s, "optional", "C00000001");
    let t = Ticket::capture(&s, &key("optional")).unwrap();
    assert!(remove_local(&mut s, &t, "C00000001"));
    let mut update = fixture::view();
    update["blocks"][1]["element"]["initial_channels"] = json!(["C00000001"]);
    update["blocks"][2]["element"]["initial_channels"] = json!(["C00000001"]);
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
        json!({"type":"multi_channels_select","selected_channels":["C00000006","C00000002"]})
    );
    assert_eq!(
        wire["values"]["optional"]["value"],
        json!({"type":"multi_channels_select","selected_channels":[]})
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
#[test]
fn all_remote_choices_survive_query_replacement_but_current_archival_blocks_submit() {
    let mut s = setup();
    let t = Ticket::capture(&s, &key("optional")).unwrap();
    assert!(open_local(&mut s, t.clone()));
    let remote = serde_json::from_value(
        json!({"id":"C99999999","name":"remote","is_channel":true,"updated":1}),
    )
    .unwrap();
    let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, rev) = (p.serial, p.query_revision);
    assert!(complete_entities(
        &mut s,
        serial,
        rev,
        Ok((vec![], vec![remote]))
    ));
    assert!(choose_local(&mut s, &t, serial, rev, "C99999999"));
    assert!(complete_entities(&mut s, serial, rev, Ok((vec![], vec![]))));
    assert!(requests::prepare_submission(&s).is_ok());
    let remote = serde_json::from_value(
        json!({"id":"C99999999","name":"remote","is_channel":true,"is_archived":true,"updated":2}),
    )
    .unwrap();
    assert!(complete_entities(
        &mut s,
        serial,
        rev,
        Ok((vec![], vec![remote]))
    ));
    assert!(requests::prepare_submission(&s).is_err());
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key("optional")].value,
        InputValue::Channels(vec!["C99999999".into()])
    );
    assert!(remove_local(&mut s, &t, "C99999999"));
    assert!(requests::prepare_submission(&s).is_ok());
}
#[test]
fn initial_selection_outside_first_page_and_enterprise_are_supported_without_origin_default() {
    let mut s = setup();
    let w = s.core.workspaces.get_mut("T1").unwrap();
    for n in 100..140 {
        let id = format!("C{n:08}");
        w.channels.insert(
            id.clone(),
            serde_json::from_value(json!({"id":id,"name":format!("aaa{n}"),"is_channel":true}))
                .unwrap(),
        );
    }
    let ws = super_platinum_core::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U00000001".into(),
        enterprise_id: Some("E_MULTI".into()),
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    s.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "synthetic".into(),
        workspaces: [("T1".into(), ws)].into(),
    });
    let t = Ticket::capture(&s, &key("reviewers")).unwrap();
    assert!(open_local(&mut s, t));
    assert!(
        !s.app_modal
            .as_ref()
            .unwrap()
            .users
            .as_ref()
            .unwrap()
            .candidates
            .iter()
            .any(|c| c.id == "C00000006")
    );
    assert!(requests::prepare_submission(&s).is_ok());
    let wire = requests::prepare_submission(&s).unwrap();
    assert_eq!(
        wire["values"]["optional"]["value"]["selected_channels"],
        json!([])
    );
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000006")
        .unwrap()
        .is_private = true;
    assert!(requests::prepare_submission(&s).is_err());
}
#[test]
fn stale_choice_owner_and_uncertain_submission_never_admit() {
    for change in [
        |s: &mut ShellState| s.core.account_epoch += 1,
        |s: &mut ShellState| s.app_modal.as_mut().unwrap().uncertain = true,
    ] {
        let mut s = setup();
        let t = Ticket::capture(&s, &key("optional")).unwrap();
        assert!(open_local(&mut s, t.clone()));
        let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
        let (serial, rev) = (p.serial, p.query_revision);
        change(&mut s);
        assert!(!choose_local(&mut s, &t, serial, rev, "C00000002"));
        assert!(requests::prepare_submission(&s).is_err());
        assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
    }
}

#[test]
fn discovery_in_other_field_invalidates_retained_selection_even_after_other_field_removes_it() {
    let mut s = setup();
    let remote = |archived, updated| {
        serde_json::from_value(json!({"id":"C99999999","name":"remote","is_channel":true,"is_archived":archived,"updated":updated})).unwrap()
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
            Ok((vec![], vec![remote(false, 1)]))
        ));
        assert!(choose_local(&mut s, &t, serial, rev, "C99999999"));
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
        Ok((vec![], vec![remote(true, 2)]))
    ));
    assert!(requests::prepare_submission(&s).is_ok());
    assert!(complete_entities(
        &mut s,
        serial,
        rev,
        Ok((vec![], vec![remote(true, 2)]))
    ));
    assert!(remove_local(&mut s, &t, "C99999999"));
    let Err(super::super::FormError::Validation(errors)) = requests::prepare_submission(&s) else {
        panic!("retained first-field selection must fail");
    };
    assert!(errors.iter().any(|(k, _)| k == &key("reviewers")));
    assert!(!errors.iter().any(|(k, _)| k == &key("optional")));
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key("reviewers")].value,
        InputValue::Channels(vec!["C00000006".into(), "C99999999".into()])
    );
}
