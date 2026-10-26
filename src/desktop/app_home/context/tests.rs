use super::*;

fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    s
}
#[test]
fn source_fixture_boot_has_valid_identity_and_real_pending_member_ingress() {
    let mut s = setup();
    let team = s.core.active_team.clone().unwrap();
    let session = s.core.session.as_ref().unwrap().workspaces[&team].clone();
    let account = s.core.active_account.clone().unwrap();
    let mut unsupported = super_platinum_core::state::directory_members::Store::default();
    assert!(
        unsupported
            .observe_boot(
                &account,
                &session,
                &s.core.workspaces[&team].users.keys().cloned().collect(),
                &json!({"ok":true,"users":[{"id":"U0"}]}),
            )
            .is_err()
    );
    assert!(unsupported.has_pending(&account, &session).is_err());

    fixture::install(&mut s);
    let session = &s.core.session.as_ref().unwrap().workspaces[&team];
    let w = &s.core.workspaces[&team];
    assert_eq!(session.user_id, "UHOMESELF");
    assert_eq!(w.self_user_id, session.user_id);
    assert_eq!(w.users.len(), 2);
    assert!(w.users.contains_key("UHOMESELF"));
    assert!(w.users.contains_key("UHOME0001"));
    assert_eq!(w.channels.len(), 1);
    assert!(
        w.channels
            .contains_key(s.core.active_channel.as_ref().unwrap())
    );
    assert!(s.app_home.surface.as_ref().unwrap().owner.current(&s));
    let saved = w.directory_members.cached().unwrap();
    assert_eq!(saved["records"], json!([]));
    assert_eq!(saved["pending"][0]["origin"], "Boot");
    assert_eq!(saved["pending"][0]["rows"][0]["id"], "UHOMESELF");
    assert!(w.directory_members.has_pending(&account, session).unwrap());
}
#[test]
fn nonunified_requires_full_observed_configuration_and_current_account() {
    let mut s = setup();
    let initial = Witness::capture(&s).unwrap();
    assert_eq!(initial.team_id(), None);
    assert!(initial.current(&s));
    let flags = s
        .core
        .session
        .as_ref()
        .unwrap()
        .unified_user_client_enabled
        .clone();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert!(Witness::capture(&s).is_err());
    assert!(!initial.current(&s));
    s.core.session.as_mut().unwrap().unified_user_client_enabled = flags;
    assert!(Witness::capture(&s).is_ok());
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert("TOTHER".into(), true);
    assert!(Witness::capture(&s).is_err());
    assert!(!initial.current(&s));
}
#[test]
fn warm_payload_values_and_native_member_do_not_certify_context() {
    let mut s = setup();
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team.clone(), true);
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.cfm_payload.internal_workspace_ids = Some(json!(["TWORK0002"]));
    w.cfm_payload.default_workspace = Some(json!("TWORK0002"));
    w.cfm_payload
        .user_prefs
        .insert("workspace_filter_selected".into(), json!("TWORK0002"));
    assert!(Witness::capture(&s).is_err());
    assert!(Choice::capture(&s).is_err());
}
#[test]
fn existing_override_revision_socket_and_account_custody_are_independent_of_form() {
    let mut s = setup();
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team.clone(), true);
    let account = Arc::new(SourceContext::capture(&s, &team).unwrap());
    let channel = s.core.active_channel.clone().unwrap();
    // Unit setup of an already selected private override; native fixtures prove
    // creation through the real source-owned chooser and option inventory.
    let k = key(&account, &channel);
    s.app_home.context.overrides.insert(
        k.clone(),
        Override {
            account: account.clone(),
            team: "TWORK0003".into(),
            revision: 11,
        },
    );
    let before = inputs::form(&s).unwrap().state().unwrap();
    let old = Witness::capture(&s).unwrap();
    assert_eq!(old.team_id(), Some("TWORK0003"));
    assert!(old.current(&s));
    s.app_home.context.overrides.get_mut(&k).unwrap().revision = 12;
    assert!(!old.current(&s));
    let fresh = Witness::capture(&s).unwrap();
    assert!(fresh.current(&s));
    assert_eq!(inputs::form(&s).unwrap().state().unwrap(), before);
    s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    assert!(!fresh.current(&s));
    assert!(override_for(&s, &account, &channel).is_some());
    s.core.account_epoch += 1;
    assert!(override_for(&s, &account, &channel).is_none());
}

#[test]
fn outside_override_display_keeps_exact_route_without_minting_menu_membership() {
    let rows = vec![
        ("TWORK0002".into(), "Design".into()),
        ("TWORK0003".into(), "Engineering".into()),
    ];
    let names = [(
        "TREMOVED".into(),
        json!({"id":"TREMOVED","name":"Retained workspace"}),
    )]
    .into();
    assert_eq!(
        outside_option(Some("TREMOVED"), &rows, &names),
        Some(("TREMOVED".into(), "Retained workspace".into()))
    );
    assert_eq!(
        outside_option(Some("TUNKNOWN"), &rows, &names),
        Some(("TUNKNOWN".into(), "Workspace name unavailable".into()))
    );
    assert_eq!(outside_option(Some("TWORK0002"), &rows, &names), None);
    assert_eq!(rows.len(), 2);
}
