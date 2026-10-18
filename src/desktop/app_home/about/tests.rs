use super::*;
use serde_json::json;
fn setup() -> (ShellState, Key) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s, false);
    let h = s.app_home.about.surface.as_ref().unwrap();
    let key = Key {
        id: h.id,
        revision: h.revision,
    };
    (s, key)
}
#[test]
fn about_owner_requires_exact_credentials_visit_bot_and_account() {
    for change in 0..8 {
        let (mut s, key) = setup();
        assert!(read_current(&s, key));
        let owner = s.app_home.about.surface.as_ref().unwrap().owner.clone();
        match change {
            0 => s.channel_generation += 1,
            1 => s.core.account_epoch += 1,
            2 => s.channel_tab = ChannelTab::Messages,
            3 => {
                s.core
                    .workspaces
                    .get_mut(&owner.ctx.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            4 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.ctx.session.team_id)
                .unwrap()
                .token
                .push('x'),
            5 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.ctx.session.team_id)
                .unwrap()
                .url
                .push_str("/changed"),
            6 => {
                s.core
                    .workspaces
                    .get_mut(&owner.ctx.session.team_id)
                    .unwrap()
                    .channels
                    .get_mut(&owner.channel)
                    .unwrap()
                    .user = Some("U_NEW".into())
            }
            _ => {
                s.core
                    .workspaces
                    .get_mut(&owner.ctx.session.team_id)
                    .unwrap()
                    .users
                    .get_mut(&owner.bot)
                    .unwrap()
                    .deleted = true
            }
        }
        assert!(!read_current(&s, key));
        assert!(!actions::enabled(&s, key));
    }
}
#[test]
fn optional_read_failure_retains_verified_info_without_action_authority() {
    let (mut s, key) = setup();
    let h = s.app_home.about.surface.as_mut().unwrap();
    let home = h.home.clone().unwrap();
    let info = h.info.clone().unwrap();
    h.fixture = false;
    let (_, read_key) = begin_read(&mut s).unwrap();
    assert_ne!(read_key.id, key.id);
    assert!(adopt_info(&mut s, read_key, home.clone(), info.clone()));
    assert_eq!(
        s.app_home.about.surface.as_ref().unwrap().info.as_ref(),
        Some(&info)
    );
    assert!(!actions::enabled(&s, read_key));
    s.channel_generation += 1;
    assert!(!adopt_info(&mut s, read_key, home, info));
}
#[test]
fn unknown_attempt_survives_close_and_reopen_but_never_replays() {
    let (mut s, _) = setup();
    fixture::install(&mut s, true);
    let n = s.app_home.about.attempts.len();
    let old = s.app_home.about.surface.as_ref().unwrap().owner.clone();
    clear(&mut s);
    assert_eq!(s.app_home.about.attempts.len(), n);
    s.channel_generation += 1;
    let (owner, key) = begin_read(&mut s).unwrap();
    assert!(!old.current(&s));
    let home = Home {
        channel: owner.channel.clone(),
        app: "A_HOME".into(),
        installed_team: owner.ctx.session.team_id.clone(),
        enabled: true,
        messages: true,
        view_id: None,
    };
    assert!(adopt_info(
        &mut s,
        key,
        home,
        Info::decode("A_HOME", json!({"ok":true,"app_id":"A_HOME"})).unwrap()
    ));
    assert!(actions::status(&s).unwrap().1);
    assert!(!actions::enabled(&s, key));
}
#[test]
fn named_fixture_uses_actual_prepared_connection_and_existing_modal_reducer() {
    let (mut s, key) = setup();
    s.core.composer.text = "unsent draft".into();
    fixture::connect(&mut s, key);
    assert_eq!(s.app_home.fixture_forms.len(), 1);
    let f = &s.app_home.fixture_forms[0];
    assert_eq!(
        url::Url::parse(f["url"].as_str().unwrap()).unwrap().path(),
        "/api/apps.user.connection.submit"
    );
    let fields: Vec<(String, String)> = serde_json::from_value(f["fields"].clone()).unwrap();
    assert!(fields.contains(&("type".into(), "connect".into())));
    assert!(fields.contains(&("service_id".into(), "0".into())));
    assert_eq!(s.overlay, Some(Overlay::AppModal));
    assert_eq!(s.core.composer.text, "unsent draft");
    assert!(!actions::enabled(&s, key));
}

#[test]
fn actual_about_rsx_permission_content_follows_observed_experiment_not_payload_presence() {
    for show in [false, true] {
        let mut dom = VirtualDom::new_with_props(
            |show: bool| {
                let state = use_signal(move || {
                    let (mut s, _) = setup();
                    s.app_home
                        .about
                        .surface
                        .as_mut()
                        .unwrap()
                        .flags
                        .as_mut()
                        .unwrap()
                        .about = show;
                    s
                });
                panel(state)
            },
            show,
        );
        let mutations = dom.rebuild_to_vec();
        let found=mutations.edits.iter().any(|edit|matches!(edit,dioxus::core::Mutation::CreateTextNode{value,..} if value=="Read the channels you select"));
        assert_eq!(found, show);
    }
}

#[test]
fn actual_about_rsx_retains_known_ai_description_after_optional_profile_failure() {
    for flag in ["is_ai_app", "is_agent_app"] {
        let mut dom = VirtualDom::new_with_props(
            |flag: &'static str| {
                let state = use_signal(move || {
                    let (mut s, _) = setup();
                    let h = s.app_home.about.surface.as_mut().unwrap();
                    let mut raw = h.info.as_ref().unwrap().raw().clone();
                    raw[flag] = json!(true);
                    raw["assistant_description"] =
                        json!("Known assistant description survives optional read failure");
                    h.info = Some(Info::decode("A_HOME", raw).unwrap());
                    h.profile = None;
                    h.flags = None;
                    h.connection = None;
                    h.connection_enabled = None;
                    h.notice = Some("Optional profile read failed".into());
                    s
                });
                panel(state)
            },
            flag,
        );
        let mutations = dom.rebuild_to_vec();
        let contains = |text: &str| {
            mutations.edits.iter().any(|edit| matches!(edit,dioxus::core::Mutation::CreateTextNode{value,..} if value==text))
        };
        assert!(contains(
            "Known assistant description survives optional read failure"
        ));
        assert!(!contains("Read the channels you select"));
    }
}

#[test]
fn about_description_reuses_typed_mrkdwn_and_exact_current_workspace() {
    let (mut s, key) = setup();
    let body = "*Bold* and _italic_ with <https://example.test/guide|Guide>\n> Quoted detail\n```\nlet item = 1;\n```";
    let nodes = description::nodes(&s, key, body);
    let crate::state::RichNode::Paragraph(first) = &nodes[0] else {
        panic!("paragraph")
    };
    assert!(first.iter().any(
        |n| matches!(n, crate::state::RichNode::StyledText {text,bold:true,..} if text=="Bold")
    ));
    assert!(first.iter().any(
        |n| matches!(n, crate::state::RichNode::StyledText {text,italic:true,..} if text=="italic")
    ));
    assert!(first.iter().any(|n| matches!(n, crate::state::RichNode::Link {label,url} if label=="Guide" && url=="https://example.test/guide")));
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, crate::state::RichNode::Quote(_)))
    );
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, crate::state::RichNode::Code(text) if text=="let item = 1;"))
    );
    assert!(description::nodes(&s, key, " \n ").is_empty());
    s.channel_generation += 1;
    assert!(description::nodes(&s, key, body).is_empty());
}
