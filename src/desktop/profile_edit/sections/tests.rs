use super::*;
use serde_json::json;

#[test]
fn pronunciation_uses_existing_owner_preflight_readback_and_never_becomes_generic_header() {
    let mut s = shell();
    let d = s.profile_edit.sections.draft.as_ref().unwrap();
    let (token, visit, serial) = (d.token.clone(), d.visit, d.serial);
    let snapshot = fixture::pronunciation_snapshot(&token.owner, "Original");
    network::finish_load(&mut s, &token, visit, serial, Ok(snapshot.clone()));
    assert_eq!(
        presentation(&s).unwrap().sections,
        vec![("H1".into(), "Name pronunciation".into())]
    );
    s.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(open_form(&snapshot, "H1").unwrap());
    assert!(!change_shell(
        &mut s,
        visit,
        serial,
        "title",
        SectionValue::Text("Not allowed".into())
    ));
    assert!(change_shell(
        &mut s,
        visit,
        serial,
        "pronunciation",
        SectionValue::Text(" New ".into())
    ));
    let a = begin(&mut s, visit, serial).unwrap();
    assert!(network::preflight(&mut s, &a, &snapshot));
    assert!(network::finish_write(&mut s, &a, Ok(())));
    assert!(begin(&mut s, visit, serial).is_none());
    let read = begin_check(&mut s, visit, serial).unwrap();
    network::finish_check(
        &mut s,
        &read,
        Ok(fixture::pronunciation_snapshot(&token.owner, " New ")),
    );
    assert!(!pending(&s));
    let view = presentation(&s).unwrap();
    assert!(!view.can_save);
    let form = view.form.unwrap();
    assert_eq!(form.fields().len(), 1);
    assert_eq!(form.id(), "H1");
    assert!(!change_shell(
        &mut s,
        visit,
        serial,
        "pronunciation",
        SectionValue::Text("Late".into())
    ));
}

#[test]
fn pronunciation_uncertain_write_retains_closed_scope_across_reopen_and_owner_change() {
    let mut s = shell();
    let d = s.profile_edit.sections.draft.as_ref().unwrap();
    let (token, visit, serial) = (d.token.clone(), d.visit, d.serial);
    let snapshot = fixture::pronunciation_snapshot(&token.owner, "Original");
    network::finish_load(&mut s, &token, visit, serial, Ok(snapshot.clone()));
    s.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(open_form(&snapshot, "H1").unwrap());
    assert!(change_shell(
        &mut s,
        visit,
        serial,
        "pronunciation",
        SectionValue::Text(String::new())
    ));
    let a = begin(&mut s, visit, serial).unwrap();
    assert!(network::preflight(&mut s, &a, &snapshot));
    network::finish_write(
        &mut s,
        &a,
        Err(Error::Transport("synthetic lost reply".into())),
    );
    assert!(pending(&s));
    assert!(!intercept_close(&mut s, Some(visit)));
    assert!(open_shell(&mut s, visit).is_none());
    let view = presentation(&s).unwrap();
    assert!(view.uncertain);
    assert_eq!(view.form.unwrap().fields().len(), 1);
    s.core.account_epoch += 1;
    poll(&mut s);
    assert!(presentation(&s).is_none());
    assert!(
        s.profile_edit.sections.entries[&token.owner]
            .pending
            .is_some()
    );
}

#[test]
fn cooldown_expiry_notifies_visible_draft_and_pending_check_once_without_retry() {
    for uncertain in [false, true] {
        let mut s = shell();
        let a = changed(&mut s);
        let entry = s
            .profile_edit
            .sections
            .entries
            .get_mut(&a.token.owner)
            .unwrap();
        if uncertain {
            entry.pending.as_mut().unwrap().phase = Phase::Uncertain;
        } else {
            entry.pending = None;
        }
        let expires = now() + 60;
        entry.retry_at = expires;
        assert!(!presentation(&s).unwrap().can_save);
        assert!(!presentation(&s).unwrap().can_check);
        assert!(!poll_at(&mut s, expires - 1));
        assert!(poll_at(&mut s, expires));
        assert!(!poll_at(&mut s, expires + 1));
        let view = presentation(&s).unwrap();
        assert_eq!(view.can_save, !uncertain);
        assert_eq!(view.can_check, uncertain);
        let entry = &s.profile_edit.sections.entries[&a.token.owner];
        assert_eq!(entry.retry_at, 0);
        assert_eq!(entry.pending.is_some(), uncertain);
        assert!(
            entry
                .pending
                .as_ref()
                .is_none_or(|p| p.phase == Phase::Uncertain && p.read.is_none())
        );
        assert!(view.form.unwrap().dirty());
    }
}

#[test]
fn person_write_preflight_requires_positive_complete_unambiguous_envelope() {
    let ids = vec!["U00000002".to_owned()];
    for raw in [
        json!({"ok":true,"results":[{"id":"U00000002"}]}),
        json!({"ok":true,"error":null,"failed_ids":[],"results":[{"id":"U00000002"}]}),
    ] {
        assert!(network::decode_people(&ids, raw).is_ok());
    }
    for raw in [
        json!({"results":[{"id":"U00000002"}]}),
        json!({"ok":false,"results":[{"id":"U00000002"}]}),
        json!({"ok":true,"error":"partial_failure","results":[{"id":"U00000002"}]}),
        json!({"ok":true,"failed_ids":["U00000002"],"results":[{"id":"U00000002"}]}),
        json!({"ok":true,"failed_ids":null,"results":[{"id":"U00000002"}]}),
        json!({"ok":true,"failed_ids":false,"results":[{"id":"U00000002"}]}),
        json!({"ok":true,"results":[{"id":"U00000002"},{"id":"U00000002"}]}),
        json!({"ok":true,"results":[{"id":"U00000002"},{"id":"U00000003"}]}),
        json!({"ok":true,"results":[{"id":"U00000003"}]}),
        json!({"ok":true,"results":null}),
        json!({"ok":true}),
    ] {
        assert!(network::decode_people(&ids, raw).is_err());
    }
    // Envelope success cannot turn synthetic or malformed IDs into identities.
    for id in [
        "U2",
        "W2",
        "D00000002",
        "u00000002",
        "U0000000_",
        "USLACKBOT",
    ] {
        assert!(
            network::decode_people(&[id.into()], json!({"ok":true,"results":[{"id":id}]})).is_err()
        );
    }
}
fn response(owner: &Owner, text: &str) -> api::OwnProfileSections {
    api::OwnProfileSections::from_response(&super::super::fixture::session(owner),&json!({"ok":true,"result":{"data":{"user":{
        "id":owner.user,"profileSections":[{"sectionId":"S1","id":"U0:section:S1","type":"CUSTOM","label":"About","canEdit":true,
        "profileElements":[{"elementId":"E1","id":"U0:section:S1:E1","__typename":"ProfileTextElement","label":"Favorite","text":text,"permissions":{"ui":true}},
        {"elementId":"Readonly","id":"U0:section:S1:Readonly","__typename":"ProfileTextElement","text":"Preserved","permissions":{"ui":false}}]}]
    }}}})).unwrap()
}
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::fixture::install(&mut shell);
    let visit = shell.profile_edit.draft.as_ref().unwrap().visit;
    let (token, serial) = open_shell(&mut shell, visit).unwrap();
    let raw = response(&token.owner, "Original");
    network::finish_load(&mut shell, &token, visit, serial, Ok(raw.clone()));
    shell.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(protocol::SectionDraft::open(&raw, "S1").unwrap());
    shell
}
fn changed(shell: &mut ShellState) -> SectionAttempt {
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (v, s) = (d.visit, d.serial);
    assert!(change_shell(
        shell,
        v,
        s,
        "E1",
        SectionValue::Text("Changed".into())
    ));
    begin(shell, v, s).unwrap()
}
#[test]
fn current_visit_owns_edit_and_close_cancels_only_unsent_preflight() {
    let mut s = shell();
    let a = changed(&mut s);
    assert!(begin(&mut s, a.visit, a.serial).is_none());
    assert!(!change_shell(
        &mut s,
        a.visit,
        a.serial + 1,
        "E1",
        SectionValue::Text("Late".into())
    ));
    assert!(!intercept_close(&mut s, Some(a.visit)));
    assert!(!pending(&s));
    assert!(!network::preflight(
        &mut s,
        &a,
        &response(&a.token.owner, "Original")
    ));
}
#[test]
fn owner_epoch_and_schema_drift_refuse_dispatch_without_dropping_draft() {
    for kind in 0..2 {
        let mut s = shell();
        let a = changed(&mut s);
        let raw = if kind == 0 {
            s.core.account_epoch += 1;
            response(&a.token.owner, "Original")
        } else {
            response(&a.token.owner, "New server value")
        };
        assert!(!network::preflight(&mut s, &a, &raw));
        assert!(!pending(&s));
        assert!(
            s.profile_edit
                .sections
                .draft
                .as_ref()
                .unwrap()
                .form
                .as_ref()
                .unwrap()
                .dirty()
        );
    }
}
#[test]
fn accepted_write_requires_positive_readback_and_never_replays() {
    let mut s = shell();
    let a = changed(&mut s);
    assert!(network::preflight(
        &mut s,
        &a,
        &response(&a.token.owner, "Original")
    ));
    assert!(intercept_close(&mut s, Some(a.visit)));
    assert!(network::finish_write(&mut s, &a, Ok(())));
    assert!(pending(&s));
    assert!(begin(&mut s, a.visit, a.serial).is_none());
    let read = begin_check(&mut s, a.visit, a.serial).unwrap();
    network::finish_check(&mut s, &read, Ok(response(&a.token.owner, "Changed")));
    assert!(!pending(&s));
    assert!(
        !s.profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .dirty()
    );
    assert!(s.profile_edit.fixture.as_ref().unwrap().attempts.is_empty());
}
#[test]
fn unknown_outcome_survives_close_and_reopen_with_read_only_recovery() {
    let mut s = shell();
    let a = changed(&mut s);
    assert!(network::preflight(
        &mut s,
        &a,
        &response(&a.token.owner, "Original")
    ));
    assert!(!network::finish_write(
        &mut s,
        &a,
        Err(Error::Transport("synthetic lost response".into()))
    ));
    assert!(!intercept_close(&mut s, Some(a.visit)));
    assert!(pending(&s));
    assert!(open_shell(&mut s, a.visit).is_none());
    let d = s.profile_edit.sections.draft.as_ref().unwrap();
    let serial = d.serial;
    assert!(begin(&mut s, a.visit, serial).is_none());
    let read = begin_check(&mut s, a.visit, serial).unwrap();
    network::finish_check(&mut s, &read, Ok(response(&a.token.owner, "Different")));
    assert!(pending(&s));
    assert!(presentation(&s).unwrap().can_keep);
    assert!(s.profile_edit.fixture.as_ref().unwrap().attempts.is_empty());
}
#[test]
fn late_checks_cannot_retire_new_owner_and_cooldown_survives_reopen() {
    let mut s = shell();
    let a = changed(&mut s);
    assert!(network::preflight(
        &mut s,
        &a,
        &response(&a.token.owner, "Original")
    ));
    network::finish_write(&mut s, &a, Ok(()));
    let read = begin_check(&mut s, a.visit, a.serial).unwrap();
    s.core.account_epoch += 1;
    network::finish_check(&mut s, &read, Ok(response(&a.token.owner, "Changed")));
    assert!(pending(&s));
    assert!(
        s.profile_edit.sections.entries[&a.token.owner]
            .checked
            .is_none()
    );
    let mut s = shell();
    let d = s.profile_edit.sections.draft.as_ref().unwrap();
    let token = d.token.clone();
    let visit = d.visit;
    s.profile_edit
        .sections
        .entries
        .entry(token.owner.clone())
        .or_default()
        .retry_at = now() + 60;
    intercept_close(&mut s, Some(visit));
    assert!(open_shell(&mut s, visit).is_none());
    assert!(!s.profile_edit.sections.draft.as_ref().unwrap().loading);
}
#[test]
fn directory_accepts_profile_only_but_rejects_guests_bots_external_and_ambiguous_info() {
    use super_platinum_core::slack::models::User;
    let user: User =
        serde_json::from_value(json!({"id":"U00000002","is_profile_only_user":true})).unwrap();
    assert!(directory::eligible(&user));
    for flag in [
        "deleted",
        "is_bot",
        "is_restricted",
        "is_ultra_restricted",
        "is_stranger",
        "is_external",
    ] {
        let mut raw = serde_json::to_value(&user).unwrap();
        raw[flag] = json!(true);
        assert!(!directory::eligible(&serde_json::from_value(raw).unwrap()));
    }
    assert!(directory::validates(
        &["U00000002".into()],
        std::slice::from_ref(&user)
    ));
    assert!(!directory::validates(
        &["U00000002".into()],
        &[user.clone(), user]
    ));
    for id in [
        "U2",
        "W2",
        "D00000002",
        "u00000002",
        "U0000000_",
        "USLACKBOT",
    ] {
        let invalid: User =
            serde_json::from_value(json!({"id":id,"is_profile_only_user":true})).unwrap();
        assert!(!directory::eligible(&invalid));
    }
    let enterprise: User =
        serde_json::from_value(json!({"id":"W00000002","is_profile_only_user":true})).unwrap();
    assert!(directory::eligible(&enterprise));
}

#[test]
fn directory_reply_requires_same_owner_visit_section_and_query_generation() {
    let mut s = shell();
    let d = s.profile_edit.sections.draft.as_ref().unwrap();
    let (token, visit, serial) = (d.token.clone(), d.visit, d.serial);
    assert!(directory::request_current(&s, &token, visit, serial, 0));
    assert!(!directory::request_current(
        &s,
        &token,
        visit + 1,
        serial,
        0
    ));
    assert!(!directory::request_current(
        &s,
        &token,
        visit,
        serial + 1,
        0
    ));
    s.profile_edit.sections.draft.as_mut().unwrap().directory = Default::default();
    assert!(!directory::request_current(&s, &token, visit, serial, 1));
    s.core.account_epoch += 1;
    assert!(!directory::request_current(&s, &token, visit, serial, 0));
}

#[test]
fn replaced_dialog_retires_read_admission_but_keeps_unconfirmed_write() {
    let mut s = shell();
    let a = changed(&mut s);
    assert!(network::preflight(
        &mut s,
        &a,
        &response(&a.token.owner, "Original")
    ));
    network::finish_write(&mut s, &a, Ok(()));
    let read = begin_check(&mut s, a.visit, a.serial).unwrap();
    s.profile_edit.sections.draft = None;
    poll(&mut s);
    assert!(
        s.profile_edit.sections.entries[&a.token.owner]
            .pending
            .as_ref()
            .unwrap()
            .phase
            == Phase::Uncertain
    );
    network::finish_check(&mut s, &read, Ok(response(&a.token.owner, "Changed")));
    assert!(pending(&s));
    assert!(
        s.profile_edit.sections.entries[&a.token.owner]
            .checked
            .is_none()
    );
}
