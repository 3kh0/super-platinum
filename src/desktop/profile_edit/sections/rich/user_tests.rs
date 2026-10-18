use super::*;
use serde_json::json;
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    config::Session as AuthSession,
};

const USER: &str = "UMENTION1";
const LABEL: &str = "@  Cafe\u{301} Harbor  ";
fn blocks(tokens: Vec<Value>) -> Vec<Value> {
    vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":tokens}]})]
}
fn token(n: u64) -> Value {
    json!({"type":"user","user_id":USER,"future":{"occurrence":n},"tab_id":"opaque-user-field"})
}
fn seed(shell: &mut ShellState, row: Value) {
    let id = row["id"].as_str().unwrap().to_owned();
    let owner = Owner::capture(shell).unwrap();
    let session = shell.core.session.as_ref().unwrap().workspaces[&owner.team].clone();
    let epoch = shell.core.account_epoch;
    let w = shell.core.workspaces.get_mut(&owner.team).unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    w.directory_members
        .enqueue_member_info(
            &owner.account,
            &session,
            vec![row],
            json!({(id.clone()):true}),
            vec![id],
            vec![],
        )
        .unwrap();
    let pending = w
        .directory_members
        .prepare(
            &owner.account,
            &session,
            epoch,
            "user-test",
            flags.clone(),
            &w.users,
        )
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    let (_, users) = w
        .directory_members
        .adopt(
            pending,
            reply,
            &flags,
            &owner.account,
            &session,
            epoch,
            "user-test",
            &w.users,
        )
        .unwrap()
        .into_parts();
    w.users = users;
}
fn prepared(row: Value) -> (ShellState, u64, u64) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_interaction(&mut shell);
    let owner = Owner::capture(&shell).unwrap();
    let session = crate::profile_edit::fixture::session(&owner);
    shell.core.session = Some(AuthSession {
        d_cookie: "synthetic-user-mention".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(owner.team.clone(), session.clone())]),
    });
    let current = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = current.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = current;
    let w = shell.core.workspaces.get_mut(&owner.team).unwrap();
    // Start an actually empty synthetic native collection; only the accepted
    // raw observation below may establish member-source facts for this test.
    w.users.clear();
    w.directory_members
        .begin(&owner.account, &session, &BTreeSet::new())
        .unwrap();
    seed(&mut shell, row);
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (d.visit, d.serial);
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(blocks(vec![token(1), token(2)]))
    ));
    (shell, visit, serial)
}
fn row() -> Value {
    json!({"id":USER,"name":"must-not-be-label","real_name":"Top name",
        "profile":{"display_name":"  Cafe\u{301} Harbor  ","real_name":"Profile name"}})
}

#[test]
fn explicit_retry_selects_only_unavailable_known_references() {
    let (mut shell, _, _) = prepared(row());
    let token = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .token
        .clone();
    let ids = vec![USER.into(), "UABSENT01".into()];
    assert!(
        opening::known_recovery_ids(&shell, &token, 10, &ids)
            .unwrap()
            .is_empty()
    );
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .directory_members
        .invalidate(USER);
    assert_eq!(
        opening::known_recovery_ids(&shell, &token, 10, &ids).unwrap(),
        vec![USER.to_owned()]
    );
}

#[test]
fn explicit_retry_detects_native_drift_and_terminal_source_without_fake_absence() {
    let (mut shell, _, _) = prepared(row());
    let token = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .token
        .clone();
    let ids = vec![USER.into()];
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .users
        .get_mut(USER)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .display_name = Some("uncertified native name".into());
    assert_eq!(
        opening::known_recovery_ids(&shell, &token, 11, &ids).unwrap(),
        ids
    );

    let (shell, _, _) = prepared(json!({"id":USER,"name":"","updated":0,
        "isNonExistent":true,"profile":{"display_name":"","real_name":""}}));
    let token = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .token
        .clone();
    assert_eq!(
        opening::known_recovery_ids(&shell, &token, 12, &ids).unwrap(),
        ids
    );
    let w = &shell.core.workspaces[&token.owner.team];
    let session = &shell.core.session.as_ref().unwrap().workspaces[&token.owner.team];
    assert!(
        w.directory_members
            .observations(
                &token.owner.account,
                session,
                shell.core.account_epoch,
                "still-known"
            )
            .unwrap()
            .source_ids()
            .any(|id| id == USER)
    );
}

#[test]
fn explicit_retry_does_not_turn_pending_member_publication_into_a_read() {
    let (mut shell, _, _) = prepared(row());
    let token = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .token
        .clone();
    let session = shell.core.session.as_ref().unwrap().workspaces[&token.owner.team].clone();
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .directory_members
        .enqueue_member_info(
            &token.owner.account,
            &session,
            vec![row()],
            json!({(USER):true}),
            vec![USER.into()],
            vec![],
        )
        .unwrap();
    assert!(opening::known_recovery_ids(&shell, &token, 13, &[USER.into()]).is_err());
}
fn captured(serial: u64, labels: [&str; 2]) -> Capture {
    serde_json::from_value(json!({
        "snapshot":{"blocks":blocks(vec![token(1),token(2)]),"revision":1,"composing":false,"generation":serial.to_string()},
        "tokens":[{"value":token(1),"label":labels[0]},{"value":token(2),"label":labels[1]}]
    })).unwrap()
}
#[test]
fn user_opening_uses_exact_known_source_name_and_no_channel_hit() {
    for (row, expected) in [
        (row(), LABEL),
        (json!({"id":USER,"name":"username","profile":{}}), "@"),
        (
            json!({"id":USER,"real_name":"Top","profile":{"display_name":"","real_name":"  "}}),
            "@  ",
        ),
        (json!({"id":USER,"real_name":"Top","profile":{}}), "@Top"),
    ] {
        let (mut shell, visit, serial) = prepared(row);
        let (opening, hits) = opening::tracked(&mut shell, visit, serial, "long", 1);
        assert!(hits.is_empty());
        assert!(finish(&mut shell, opening.unwrap()));
        let labels: Value =
            serde_json::from_str(&labels(&shell, visit, serial, "long", 1).unwrap()).unwrap();
        assert_eq!(labels[USER], expected);
    }
}
#[test]
fn source_or_native_drift_retires_opening_and_keeps_original_blocks() {
    for native in [false, true] {
        let (mut shell, visit, serial) = prepared(row());
        let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
        let team = Owner::capture(&shell).unwrap().team;
        let w = shell.core.workspaces.get_mut(&team).unwrap();
        if native {
            w.users.get_mut(USER).unwrap().name = Some("drift".into());
        } else {
            w.directory_members.invalidate(USER);
        }
        assert!(!finish(&mut shell, opening));
        assert!(begin(&mut shell, visit, serial, "long", 2).is_err());
        let d = shell.profile_edit.sections.draft.as_ref().unwrap();
        assert!(d.rich.is_none());
        assert!(
            d.form.as_ref().unwrap().value("long")
                == Some(&SectionValue::LongText(blocks(vec![token(1), token(2)])))
        );
    }
}
#[test]
fn current_mount_keeps_frozen_user_label_through_source_update_and_capture() {
    let (mut shell, visit, serial) = prepared(row());
    let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(finish(&mut shell, opening));
    seed(
        &mut shell,
        json!({"id":USER,"profile":{"display_name":"Later"}}),
    );
    assert!(!accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        1,
        captured(serial, ["@Later", "@Later"])
    ));
    assert!(accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        1,
        captured(serial, [LABEL, LABEL])
    ));
    assert!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value("long")
            == Some(&SectionValue::LongText(blocks(vec![token(1), token(2)])))
    );
    let opening = begin(&mut shell, visit, serial, "long", 2).unwrap();
    assert!(finish(&mut shell, opening));
    let labels: Value =
        serde_json::from_str(&labels(&shell, visit, serial, "long", 2).unwrap()).unwrap();
    assert_eq!(labels[USER], "@Later");
}
#[test]
fn pending_missing_and_replaced_credentials_never_supply_user_labels() {
    let (mut shell, visit, serial) = prepared(row());
    let owner = Owner::capture(&shell).unwrap();
    let session = shell.core.session.as_ref().unwrap().workspaces[&owner.team].clone();
    shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .directory_members
        .enqueue_member_info(
            &owner.account,
            &session,
            vec![row()],
            json!({}),
            vec![USER.into()],
            vec![],
        )
        .unwrap();
    assert!(begin(&mut shell, visit, serial, "long", 1).is_err());
    let (mut shell, visit, serial) = prepared(row());
    let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
    shell.core.session.as_mut().unwrap().d_cookie = "replacement-synthetic".into();
    assert!(!finish(&mut shell, opening));
    let (mut shell, visit, serial) = prepared(row());
    let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
    shell.core.active_account = Some("replacement-account".into());
    assert!(!finish(&mut shell, opening));
    let (mut shell, visit, serial) = prepared(row());
    let missing = json!({"type":"user","user_id":"UMISSING1"});
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(blocks(vec![missing]))
    ));
    assert!(begin(&mut shell, visit, serial, "long", 1).is_err());
}

#[test]
fn deactivated_source_refresh_preserves_mounted_occurrence_custody() {
    let (mut shell, visit, serial) = prepared(row());
    let opening = begin(&mut shell, visit, serial, "long", 31).unwrap();
    assert!(finish(&mut shell, opening));
    let before = labels(&shell, visit, serial, "long", 31).unwrap();
    seed(
        &mut shell,
        json!({"id":USER,"deleted":true,
        "profile":{"display_name":"Later deactivated name"}}),
    );
    assert!(
        shell
            .core
            .workspaces
            .values()
            .any(|w| w.users.get(USER).is_some_and(|u| u.deleted))
    );
    assert_eq!(labels(&shell, visit, serial, "long", 31).unwrap(), before);
    assert!(!accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        31,
        captured(
            serial,
            ["@Later deactivated name", "@Later deactivated name"]
        )
    ));
    assert!(accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        31,
        captured(serial, [LABEL, LABEL])
    ));
    let opening = begin(&mut shell, visit, serial, "long", 32).unwrap();
    assert!(finish(&mut shell, opening));
    let actual: Value =
        serde_json::from_str(&labels(&shell, visit, serial, "long", 32).unwrap()).unwrap();
    assert_eq!(actual[USER], "@Later deactivated name");
    assert!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value("long")
            == Some(&SectionValue::LongText(blocks(vec![token(1), token(2)])))
    );
}

fn automatic_prepared() -> (ShellState, u64, u64) {
    let (mut shell, visit, serial) = prepared(row());
    let owner = Owner::capture(&shell).unwrap();
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::disconnected_fixture("synthetic-user-mention"),
    ));
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    shell.core.workspaces.get_mut(&owner.team).unwrap().rt =
        super_platinum_core::state::RealtimeStatus::Connected(
            super_platinum_core::slack::realtime::Connection::from_sender(tx),
        );
    crate::directory_member_fetches::automatic::mode::Boot::capture(
        &json!({"no_login":false,"noSocket":false}),
    )
    .accepted(&mut shell, &owner.team)
    .unwrap();
    let token = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = token.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = token;
    (shell, visit, serial)
}
#[test]
fn actual_member_hits_survive_stale_later_subset_and_keep_duplicate_occurrences() {
    let (mut shell, visit, serial) = automatic_prepared();
    let other = "UMENTION2";
    seed(
        &mut shell,
        json!({"id":other,"profile":{"display_name":"Second"}}),
    );
    let before = blocks(vec![
        token(1),
        json!({"type":"user","user_id":other}),
        token(2),
    ]);
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(before.clone())
    ));
    let owner = Owner::capture(&shell).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .directory_members
        .invalidate(other);
    let (opening, hits) = opening::tracked(&mut shell, visit, serial, "long", 41);
    assert!(opening.is_err());
    assert_eq!(hits.iter().filter(|hit| hit.is_member()).count(), 3);
    assert!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .value("long")
            == Some(&SectionValue::LongText(before))
    );
}
#[test]
fn forced_retry_suppresses_automatic_member_hits_and_unknown_tokens_add_no_authority() {
    let (mut shell, visit, serial) = automatic_prepared();
    let (opening, hits) =
        opening::tracked_with_members(&mut shell, visit, serial, "long", 42, false);
    assert!(opening.is_ok());
    assert!(hits.is_empty());
    let before = blocks(vec![
        token(1),
        json!({"type":"user","user_id":"UABSENT01"}),
        token(2),
    ]);
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(before)
    ));
    let (opening, hits) = opening::tracked(&mut shell, visit, serial, "long", 43);
    assert!(opening.is_err());
    assert_eq!(hits.iter().filter(|hit| hit.is_member()).count(), 2);
    assert!(
        shell
            .profile_edit
            .sections
            .draft
            .as_ref()
            .unwrap()
            .rich
            .is_none()
    );
}
