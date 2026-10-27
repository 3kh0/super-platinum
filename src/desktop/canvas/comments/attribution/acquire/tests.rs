use super::super::tests::{EDITOR, adopt, setup};
use super::*;
use serde_json::json;

fn observe(
    shell: &ShellState,
    model: &Comments,
    ticket: Ticket,
    token: Arc<()>,
    prefs: Value,
) -> Observation {
    let fence = Fence::capture(shell, model, ticket, model.root.clone().unwrap(), token).unwrap();
    let raw_user = json!({"ok":true,"prefs":prefs});
    let mut policy = policy::Snapshot::new(
        policy::OrdinaryClientV2,
        fence.membership.clone(),
        policy::UserPrefs::decode(&raw_user).unwrap(),
        None,
    )
    .unwrap();
    for id in policy.missing_team_preferences() {
        policy
            .observe_team(policy::TeamPrefs::decode(&id, &json!({"ok":true,"prefs":{}})).unwrap())
            .unwrap();
    }
    Observation {
        ticket: fence.ticket,
        token: fence.token,
        root: fence.root,
        member: fence.member,
        socket: fence.socket,
        revision: fence.revision,
        authority: fence.authority,
        membership: fence.membership,
        enterprise: fence.enterprise,
        policy,
        raw_user,
        raw_enterprise: None,
        raw_teams: Default::default(),
    }
}
#[test]
fn current_policy_and_member_name_preserve_spaces_hidden_and_real_preference() {
    for (prefs, expected) in [
        (json!({}), "  Canonical  "),
        (json!({"display_real_names_override":1}), "Real"),
        (json!({"hidden_users":EDITOR}), "Name hidden"),
    ] {
        let (shell, mut model, ticket, token) = setup();
        let root = model.root.clone().unwrap();
        let original = serde_json::to_value(&root).unwrap();
        model.attribution.observed = Some(Arc::new(observe(&shell, &model, ticket, token, prefs)));
        match presentation(&shell, &model, &root) {
            Presentation::Ready {
                id,
                name,
                interaction: Some(interaction),
                ..
            } => {
                assert_eq!(id, EDITOR);
                assert_eq!(name, Name::Known(expected.into()));
                assert!(interaction.current(&shell, &model));
                assert_eq!(interaction.message_ts(), root.ts.as_deref().unwrap());
            }
            _ => panic!("expected original canonical human"),
        }
        assert_eq!(
            serde_json::to_value(model.root.as_ref().unwrap()).unwrap(),
            original
        );
    }
}
#[test]
fn observed_unknown_nonexistent_and_known_empty_never_fabricate_id_name() {
    for (row, name, interactive) in [
        (
            json!({"id":EDITOR,"profile":{},"isUnknown":true}),
            Name::Unknown,
            false,
        ),
        (
            json!({"id":EDITOR,"isNonExistent":true,"real_name":"","profile":{"display_name":"","real_name":""}}),
            Name::Nonexistent,
            false,
        ),
        (
            json!({"id":EDITOR,"name":"not-a-label","real_name":"","profile":{"display_name":"","real_name":""}}),
            Name::Known(String::new()),
            true,
        ),
    ] {
        let (mut shell, mut model, ticket, token) = setup();
        adopt(&mut shell, row);
        let root = model.root.clone().unwrap();
        model.attribution.observed =
            Some(Arc::new(observe(&shell, &model, ticket, token, json!({}))));
        match presentation(&shell, &model, &root) {
            Presentation::Ready {
                name: actual,
                interaction,
                ..
            } => {
                assert_eq!(actual, name);
                assert_eq!(interaction.is_some(), interactive);
            }
            _ => panic!("expected observed name branch"),
        }
    }
}
#[test]
fn original_interaction_retires_on_root_read_member_policy_or_socket_change() {
    for change in 0..6 {
        let (mut shell, mut model, ticket, token) = setup();
        let observed = observe(&shell, &model, ticket, token, json!({}));
        assert!(observed.current(&shell, &model));
        match change {
            0 => {
                model
                    .root
                    .as_mut()
                    .unwrap()
                    .extra
                    .get_mut("document_comment")
                    .unwrap()["most_recent_editor"] = json!("UATTR0002")
            }
            1 => model.next_serial(),
            2 => adopt(
                &mut shell,
                json!({"id":EDITOR,"profile":{"display_name":"Changed"}}),
            ),
            3 => {
                let team = shell.core.active_team.clone().unwrap();
                let socket = shell.core.workspaces[&team].rt_generation;
                let event = super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                    name: "display_real_names_override".into(),
                    value: json!(1),
                };
                crate::app_home::realtime(&mut shell, &team, socket, &event);
            }
            4 => {
                let team = shell.core.active_team.clone().unwrap();
                shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
            _ => {
                let team = shell.core.active_team.clone().unwrap();
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "replacement".into();
            }
        }
        assert!(!observed.current(&shell, &model));
    }
}
#[test]
fn invisible_or_stream_root_suppresses_even_previously_captured_attribution() {
    let (shell, mut model, ticket, token) = setup();
    model.attribution.observed = Some(Arc::new(observe(&shell, &model, ticket, token, json!({}))));
    let mut root = model.root.clone().unwrap();
    root.extra.get_mut("document_comment").unwrap()["is_visible"] = json!(false);
    assert!(matches!(
        presentation(&shell, &model, &root),
        Presentation::Hidden
    ));
    root.extra
        .get_mut("document_comment")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("is_visible");
    model.query = Some(Query::Stream(super::super::super::Filter::Recent));
    assert!(matches!(
        presentation(&shell, &model, &root),
        Presentation::Hidden
    ));
}

#[test]
fn admitted_profile_hydration_keeps_pane_but_never_reauthorizes_hover_or_open() {
    let (mut shell, model, ticket, token) = setup();
    let interaction = Interaction(Arc::new(observe(&shell, &model, ticket, token, json!({}))));
    let navigation = interaction.navigation(&shell, &model).unwrap();
    assert!(!navigation.current(&shell, &model));
    shell.profile_user = Some(EDITOR.into());
    assert!(navigation.current(&shell, &model));
    let team = shell.core.active_team.clone().unwrap();
    let member = shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut(EDITOR)
        .unwrap();
    // The same fields assigned by refresh_open_profile after its ProfileVisit
    // checks. No canonical member response or new Canvas read is invented.
    member.real_name = Some("Hydrated real name".into());
    member.profile.as_mut().unwrap().title = Some("Hydrated title".into());
    member.im_mpim_ids = vec!["DPROFILE1".into()];
    member.has_more_mpims = true;
    assert!(!interaction.current(&shell, &model));
    assert!(interaction.navigation(&shell, &model).is_none());
    assert!(navigation.current(&shell, &model));
    adopt(
        &mut shell,
        json!({"id":"UATTR0002","profile":{"display_name":"Unrelated intake"}}),
    );
    assert!(navigation.current(&shell, &model));
    shell.profile_user = Some("UATTR0002".into());
    assert!(!navigation.current(&shell, &model));
}

#[test]
fn admitted_profile_still_retires_for_original_root_read_policy_and_owner_changes() {
    for change in 0..6 {
        let (mut shell, mut model, ticket, token) = setup();
        let interaction = Interaction(Arc::new(observe(&shell, &model, ticket, token, json!({}))));
        let navigation = interaction.navigation(&shell, &model).unwrap();
        shell.profile_user = Some(EDITOR.into());
        assert!(navigation.current(&shell, &model));
        let team = shell.core.active_team.clone().unwrap();
        match change {
            0 => {
                model
                    .root
                    .as_mut()
                    .unwrap()
                    .extra
                    .get_mut("document_comment")
                    .unwrap()["most_recent_editor"] = json!("UATTR0002")
            }
            1 => model.next_serial(),
            2 => {
                let socket = shell.core.workspaces[&team].rt_generation;
                crate::app_home::realtime(
                    &mut shell,
                    &team,
                    socket,
                    &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                        name: "display_real_names_override".into(),
                        value: json!(1),
                    },
                );
            }
            3 => shell.core.active_account = Some("foreign".into()),
            4 => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            _ => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "replacement".into()
            }
        }
        assert!(!navigation.current(&shell, &model));
    }
}

#[test]
fn explicit_recovery_slot_never_revives_old_interaction_even_for_same_member_bytes() {
    let (mut shell, mut model, ticket, token) = setup();
    let original = Interaction(Arc::new(observe(
        &shell,
        &model,
        ticket.clone(),
        token,
        json!({}),
    )));
    let team = ticket.owner.scope.team.clone();
    let original_native = shell.core.workspaces[&team].users[EDITOR].clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut(EDITOR)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .title = Some("Profile hydration".into());
    assert!(!original.current(&shell, &model));
    let fresh_token = Arc::new(());
    model.attribution.token = Some(fresh_token.clone());
    // Deliberately restore exact bytes in this pure slot test: even complete
    // member equality cannot revive an old hover/open from the prior slot.
    // This is not production repair or network/adoption evidence.
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert(EDITOR.into(), original_native);
    let fresh = Interaction(Arc::new(observe(
        &shell,
        &model,
        ticket,
        fresh_token,
        json!({}),
    )));
    assert!(fresh.current(&shell, &model));
    assert!(!original.current(&shell, &model));
    assert!(original.navigation(&shell, &model).is_none());
}
