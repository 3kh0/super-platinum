use super::*;
use serde_json::json;
use super_platinum_core::{
    config::Session as AuthSession,
    slack::models::{UserGroup, UserGroupsPage},
};

const GROUP: &str = "SMENTION01";
fn token(n: u64) -> Value {
    json!({"type":"usergroup","usergroup_id":GROUP,"opaque":{"occurrence":n},"tab_id":"opaque-group-field"})
}
fn blocks() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[token(1),token(2)]}]}),
    ]
}
fn row(handle: &str) -> Value {
    json!({"id":GROUP,"handle":handle})
}
fn prepared(raw: Value) -> (ShellState, u64, u64) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install_interaction(&mut shell);
    let owner = Owner::capture(&shell).unwrap();
    let session = crate::profile_edit::fixture::session(&owner);
    shell.core.session = Some(AuthSession {
        d_cookie: "synthetic-group".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(owner.team.clone(), session)]),
    });
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("synthetic-group").unwrap(),
    ));
    let current = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = current.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = current;
    let page: UserGroupsPage = serde_json::from_value(json!({"results":[raw]})).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .usergroups
        .insert(GROUP.into(), page.results[0].clone());
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (d.visit, d.serial);
    assert!(change_shell(
        &mut shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(blocks())
    ));
    (shell, visit, serial)
}
fn group(shell: &mut ShellState) -> &mut UserGroup {
    let team = shell.core.active_team.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .usergroups
        .get_mut(GROUP)
        .unwrap()
}
fn captured(serial: u64, label: &str) -> Capture {
    serde_json::from_value(json!({"snapshot":{"blocks":blocks(),"revision":1,"composing":false,"generation":serial.to_string()},
        "tokens":[{"value":token(1),"label":label},{"value":token(2),"label":label}]})).unwrap()
}

#[test]
fn group_labels_use_actual_source_normalization_and_exact_empty_token_key() {
    for (raw, label) in [
        (row("@@crew"), "@@crew"),
        (row(""), ""),
        (
            json!({"id":GROUP,"handle":"crew","date_delete":123,"is_visible":false}),
            "@crew",
        ),
        (
            json!({"id":GROUP,"handle":"crew","isUnknown":true}),
            "@unknown-group",
        ),
    ] {
        let (mut shell, visit, serial) = prepared(raw);
        let (opening, hits) = opening::tracked(&mut shell, visit, serial, "long", 1);
        assert!(hits.is_empty());
        assert!(finish(&mut shell, opening.unwrap()));
        let labels: Value =
            serde_json::from_str(&labels(&shell, visit, serial, "long", 1).unwrap()).unwrap();
        assert_eq!(labels[token(1).to_string()], label);
        assert_eq!(labels[token(2).to_string()], label);
        assert!(accept_shell(
            &mut shell,
            visit,
            serial,
            "long",
            1,
            captured(serial, label)
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
                == Some(&SectionValue::LongText(blocks()))
        );
    }
}

#[test]
fn mounted_group_label_stays_bound_while_new_opening_sees_new_source() {
    let (mut shell, visit, serial) = prepared(row("old"));
    let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
    assert!(finish(&mut shell, opening));
    *group(&mut shell) = UserGroup::from_observed(&row("new")).unwrap();
    assert!(!accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        1,
        captured(serial, "@new")
    ));
    assert!(accept_shell(
        &mut shell,
        visit,
        serial,
        "long",
        1,
        captured(serial, "@old")
    ));
    let opening = begin(&mut shell, visit, serial, "long", 2).unwrap();
    assert!(finish(&mut shell, opening));
    let labels: Value =
        serde_json::from_str(&labels(&shell, visit, serial, "long", 2).unwrap()).unwrap();
    assert_eq!(labels[token(1).to_string()], "@new");
}

#[test]
fn group_change_before_mount_refuses_and_keeps_original_description() {
    let (mut shell, visit, serial) = prepared(row("old"));
    let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
    *group(&mut shell) = UserGroup::from_observed(&row("new")).unwrap();
    assert!(!finish(&mut shell, opening));
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
            == Some(&SectionValue::LongText(blocks()))
    );
}

#[test]
fn legacy_placeholder_missing_and_native_drift_cannot_supply_group_label() {
    for mode in 0..4 {
        let (mut shell, visit, serial) = prepared(row("old"));
        match mode {
            0 => group(&mut shell).label_source = None,
            1 => {
                *group(&mut shell) = UserGroup {
                    id: GROUP.into(),
                    ..Default::default()
                }
            }
            2 => group(&mut shell).handle = "uncertified".into(),
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .usergroups
                    .remove(GROUP);
            }
        }
        assert!(begin(&mut shell, visit, serial, "long", 1).is_err());
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
}

#[test]
fn group_opening_retains_original_full_auth_and_account() {
    for mode in 0..3 {
        let (mut shell, visit, serial) = prepared(row("old"));
        let opening = begin(&mut shell, visit, serial, "long", 1).unwrap();
        match mode {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.session.as_mut().unwrap().d_cookie = "replacement".into(),
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut(&shell.core.active_team.clone().unwrap())
                    .unwrap()
                    .rt_generation += 1
            }
        }
        assert!(!finish(&mut shell, opening));
    }
}
