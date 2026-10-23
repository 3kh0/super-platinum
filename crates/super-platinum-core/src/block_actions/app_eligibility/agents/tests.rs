use super::*;

#[test]
fn enabled_default_profile_is_reached_only_after_actual_settings_scope() {
    assert_eq!(
        Settings::decode(&json!({}), None)
            .unwrap()
            .enabled_default_app(),
        None
    );
    let team = json!({"default_channel_knowledge_agent":{"app_id":"ADEFAULT"},
        "ai_apps_settings":{"apps":[{"app_id":"ADEFAULT","is_ai_enabled":false}]}});
    assert_eq!(
        Settings::decode(&team, None).unwrap().enabled_default_app(),
        None
    );
    // Enterprise locked settings supersede team app enablement, but default
    // identity still comes from the current team's original preference.
    assert_eq!(
        Settings::decode(&team, Some(&json!({})))
            .unwrap()
            .enabled_default_app(),
        Some("ADEFAULT")
    );
    let disabled =
        json!({"ai_apps_settings":{"apps":[{"app_id":"ADEFAULT","is_ai_enabled":false}]}});
    assert_eq!(
        Settings::decode(&team, Some(&disabled))
            .unwrap()
            .enabled_default_app(),
        None
    );
}

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}

#[test]
fn read_builders_keep_actual_methods_fields_and_cursor_out_of_permissions() {
    let w = workspace();
    let client = SlackClient::default();
    for (request, method, fields) in [
        (
            list_request(&client, &w, None).unwrap(),
            "aiApps.list",
            vec![("limit", "20")],
        ),
        (
            list_request(&client, &w, Some("opaque+1")).unwrap(),
            "aiApps.list",
            vec![("limit", "20"), ("cursor", "opaque+1")],
        ),
        (
            boot_request(&client, &w),
            "client.init",
            vec![("include_relevant_onboarding", "false")],
        ),
        (
            preferences_request(&client, &w),
            "client.extras",
            vec![("extras", "[\"team_prefs\"]")],
        ),
        (
            enterprise_request(&client, &w),
            "enterprise.prefs.get",
            vec![],
        ),
    ] {
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/api/{method}")
        );
        let crate::slack::client::RequestBody::Form(form) = request.body else {
            panic!("form");
        };
        let mut expected = vec![("token", "synthetic")];
        expected.extend(fields.iter().copied().filter(|(k, _)| *k != "cursor"));
        expected.push(("_x_reason", "native-calendar-attendee-agents"));
        expected.extend(fields.iter().copied().filter(|(k, _)| *k == "cursor"));
        assert_eq!(
            form.iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            form.iter().find(|(k, _)| k == "_x_reason").unwrap().1,
            "native-calendar-attendee-agents"
        );
    }
    assert!(list_request(&client, &w, Some("bad\nnext")).is_err());
    let raw = json!({"ok":true,"self":{"id":"U1"},"team":{"id":"T1","enterprise_id":"E1"}});
    assert!(boot(&w, &raw).unwrap().1);
    let mut wrong = raw;
    wrong["self"]["id"] = json!("U2");
    assert!(boot(&w, &wrong).is_err());
    assert!(
        settings(
            &json!({"ok":true,"team_prefs":{"workspace_prefs":{}}}),
            None
        )
        .is_err()
    );
}

fn user(agentforce: bool) -> User {
    serde_json::from_value(
        json!({"id":"UAGENT","is_bot":true,"is_agentforce_bot":agentforce,
        "profile":{"api_app_id":"AAGENT"}}),
    )
    .unwrap()
}
fn profile(agent: bool, agentforce: bool) -> Profile {
    Profile::decode(
        "AAGENT",
        &json!({"ok":true,"app_profile":{"id":"AAGENT",
        "is_agent_app":agent,"agentforce":if agentforce {json!({})} else {Value::Null},
        "bot_user":{"id":"UAGENT","username":"agent"}}}),
    )
    .unwrap()
}
#[test]
fn ordinary_agent_requires_actual_matching_agent_profile_not_bot_or_ai_capabilities() {
    let mut u = user(false);
    assert!(!profile(false, true).agent_for(&u));
    assert!(profile(true, false).agent_for(&u));
    u.profile
        .as_mut()
        .unwrap()
        .extra
        .insert("api_app_id".into(), json!("AOTHER"));
    assert!(!profile(true, false).agent_for(&u));
    assert!(
        Profile::decode(
            "AAGENT",
            &json!({"ok":true,"app_profile":{"id":"AOTHER","is_agent_app":true}})
        )
        .is_err()
    );
    assert!(
        Profile::decode(
            "AAGENT",
            &json!({"ok":true,"app_profile":{"id":"AAGENT","is_agent_app":"true"}})
        )
        .is_err()
    );
}
#[test]
fn agentforce_access_requires_exact_bot_and_enabled_app_or_default_exception() {
    let u = user(true);
    let p = profile(true, true);
    let settings = Settings::decode(&json!({}), None).unwrap();
    let access = Access::new(settings, BTreeSet::from(["AAGENT".into()]), BTreeMap::new());
    assert!(access.allows(&u, Some(&p)));
    assert!(!access.allows(&u, Some(&profile(true, false))));
    assert!(!access.allows(
        &serde_json::from_value(json!({"id":"UOTHER"})).unwrap(),
        Some(&p)
    ));
    let default = Settings::decode(
        &json!({"default_channel_knowledge_agent":{"app_id":"AAGENT","status":"inactive"}}),
        None,
    )
    .unwrap();
    // getMemberFilterFunction uses Cq (ID only), not the separate available/active selector.
    assert!(Access::new(default, BTreeSet::new(), BTreeMap::new()).allows(&u, Some(&p)));
    let disabled = Settings::decode(
        &json!({"default_channel_knowledge_agent":{"app_id":"AAGENT"},
        "ai_apps_settings":{"apps":[{"app_id":"AAGENT","is_ai_enabled":false}]}}),
        None,
    )
    .unwrap();
    assert!(
        !Access::new(disabled, BTreeSet::from(["AAGENT".into()]), BTreeMap::new())
            .allows(&u, Some(&p))
    );
}
#[test]
fn enterprise_uses_locked_settings_but_team_default_and_last_duplicate_setting() {
    let team = json!({"default_channel_knowledge_agent":{"app_id":"AAGENT"},
        "ai_apps_settings":{"apps":[{"app_id":"AAGENT","is_ai_enabled":false}]}});
    let allowed = Settings::decode(&team, Some(&json!({}))).unwrap();
    assert!(
        Access::new(allowed, BTreeSet::new(), BTreeMap::new())
            .allows(&user(true), Some(&profile(true, true)))
    );
    let locked = json!({"default_channel_knowledge_agent":{"app_id":"AOTHER"},"ai_apps_settings":{"apps":[
        {"app_id":"AAGENT","is_ai_enabled":false},{"app_id":"AAGENT","is_ai_enabled":true}]}});
    assert!(
        Access::new(
            Settings::decode(&team, Some(&locked)).unwrap(),
            BTreeSet::new(),
            BTreeMap::new()
        )
        .allows(&user(true), Some(&profile(true, true)))
    );
    assert!(Settings::decode(&json!({"ai_apps_settings":{"apps":"unknown"}}), None).is_err());
}
#[test]
fn list_modes_preserve_all_three_source_skips_without_synthesizing_access() {
    let mut u = user(false);
    for key in ["ai_apps_list_fe_killswitch", "support_lots_of_agents"] {
        assert_eq!(
            list_mode(&json!({"ok":true,"assignments":{key:{"group":"on"}}}), &u).unwrap(),
            ListMode::Skip
        );
    }
    assert_eq!(
        list_mode(&json!({"ok":true,"assignments":{}}), &u).unwrap(),
        ListMode::Fetch
    );
    u.is_restricted = true;
    assert_eq!(
        list_mode(&json!({"ok":true,"assignments":{}}), &u).unwrap(),
        ListMode::Skip
    );
    assert!(list_mode(&json!({"ok":false,"assignments":{}}), &u).is_err());
    u.is_restricted = false;
    assert!(
        list_mode(
            &json!({"ok":true,"assignments":{"support_lots_of_agents":{}}}),
            &u
        )
        .is_err()
    );
}
#[test]
fn real_page_shape_records_only_agentforce_access_and_retains_cursor() {
    let raw = json!({"ok":true,"apps":[{"id":"AAGENT","agentforce":{},"is_agent_app":true,
        "bot_user":{"id":"UAGENT"}},{"id":"AORDINARY","is_agent_app":true}],
        "response_metadata":{"next_cursor":"opaque/cursor+1"}});
    let page = Page::decode(&raw).unwrap();
    assert_eq!(page.accessible, BTreeSet::from(["AAGENT".into()]));
    assert_eq!(page.cursor.as_deref(), Some("opaque/cursor+1"));
    let access = Access::new(
        Settings::decode(&json!({}), None).unwrap(),
        page.accessible,
        page.profiles
            .into_iter()
            .map(|p| (p.app.clone(), p))
            .collect(),
    );
    assert!(access.allows(&user(true), None));
    assert!(Page::decode(&json!({"ok":true,"apps":[{"id":"AAGENT","bot_user":{}}]})).is_err());
    assert!(
        Page::decode(&json!({"ok":true,"apps":[],"response_metadata":{"next_cursor":false}}))
            .is_err()
    );
}
