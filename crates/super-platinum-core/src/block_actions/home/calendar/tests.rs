use super::*;
fn view(action: &str, value: &str) -> View {
    let home = Home {
        channel: "D1".into(),
        app: "A_HOME".into(),
        installed_team: "T1".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    View::decode(&home,json!({"id":"V1","type":"home","app_id":"A_HOME","app_installed_team_id":"T1",
        "opaque":{"kept":73},"state":{"values":{"unsent":{"value":"retained input"}}},
        "blocks":[{"type":"actions","block_id":"b","elements":[{"type":"button","action_id":action,"value":value,
        "text":{"type":"plain_text","text":"Calendar"}}]}]})).unwrap()
}
#[test]
fn create_entry_preserves_full_home_and_does_not_gain_ordinary_dispatch_authority() {
    for (prefix, provider) in [
        ("ADZ494LHY_CREATE_EVENT", Provider::Google),
        ("AFV5ECLBZ_CREATE_EVENT", Provider::Outlook),
        ("A01NK8UJRGC_CREATE_EVENT", Provider::OutlookGcc),
    ] {
        let v = view("create", &format!("{prefix}:tomorrow"));
        let entry = Entry::capture(&v, "b", "create", "U1", "2026-10-17").unwrap();
        assert_eq!(
            entry.operation(),
            &Operation::Create {
                provider,
                preset: Some("2026-10-18".into())
            }
        );
        assert!(entry.same_view(&v, "U1"));
        assert!(v.button("b", "create", "U1").is_err());
        let mut changed = v.raw().clone();
        changed["state"]["values"]["unsent"]["value"] = json!("new input");
        assert!(!entry.same_view(&View::decode(&v.home, changed).unwrap(), "U1"));
    }
}
#[test]
fn exact_date_suffix_uses_captured_integer_components_and_rejects_overflow() {
    for (input, want) in [
        ("today", Some("2026-10-17")),
        ("tomorrow", Some("2026-10-18")),
        ("2026-01-01", Some("2026-10-17")),
        ("2028-2-29suffix", Some("2028-02-29")),
        ("2027-2-29", None),
        ("2028-13-01", None),
        ("invalid", None),
        ("", None),
    ] {
        assert_eq!(preset(Some(input), "2026-10-17").as_deref(), want);
    }
}
#[test]
fn confirmation_recipient_and_unique_raw_control_checks_are_not_bypassed() {
    let v = view("create", "ADZ494LHY_CREATE_EVENT");
    for change in 0..4 {
        let mut raw = v.raw().clone();
        match change {
            0 => raw["blocks"][0]["elements"][0]["visible_to_user_ids"] = json!(["U_OTHER"]),
            1 => raw["blocks"][0]["elements"][0]["confirm"] = json!({"title":"bad"}),
            2 => {
                let button = raw["blocks"][0]["elements"][0].clone();
                raw["blocks"][0]["elements"]
                    .as_array_mut()
                    .unwrap()
                    .push(button);
            }
            _ => raw["blocks"][0]["type"] = json!("input"),
        }
        assert!(
            Entry::capture(
                &View::decode(&v.home, raw).unwrap(),
                "b",
                "create",
                "U1",
                "2026-10-17"
            )
            .is_err()
        );
    }
}
#[test]
fn connect_uses_exact_four_or_five_fields_and_accepts_only_safe_success_url() {
    use crate::slack::client::RequestBody;
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let client = SlackClient::default();
    for scope in [None, Some("calendar.events")] {
        let mut value = json!({"app_id":"A_HOME","provider_key":"gcal"});
        if let Some(scope) = scope {
            value["scope_group"] = json!(scope);
        }
        let v = view("calendar_hermes_connect", &value.to_string());
        let e = Entry::capture(&v, "b", "calendar_hermes_connect", "U1", "2026-10-17").unwrap();
        assert!(v.button("b", "calendar_hermes_connect", "U1").is_err());
        let req = e.connect_request(&client, &w).unwrap();
        assert_eq!(req.method, "POST");
        let url = url::Url::parse(&req.url).unwrap();
        assert_eq!(url.origin().ascii_serialization(), "https://example.test");
        assert_eq!(url.path(), "/api/apps.auth.external.start");
        assert!(url.fragment().is_none());
        // SlackClient adds captured client metadata after the exact endpoint path.
        let keys: Vec<_> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
        assert_eq!(
            keys,
            [
                "_x_id",
                "_x_csid",
                "_x_desktop_ia",
                "_x_foreground",
                "_x_frontend_build_type",
                "_x_gantry",
                "_x_num_retries",
                "_x_version_ts",
                "fp",
                "slack_route",
            ]
        );
        let RequestBody::Form(fields) = req.body else {
            panic!("form")
        };
        assert_eq!(fields.len(), if scope.is_some() { 5 } else { 4 });
        let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields.len(), if scope.is_some() { 5 } else { 4 });
        assert_eq!(fields["token"], "synthetic");
        assert_eq!(fields["app_id"], "A_HOME");
        assert_eq!(fields["provider_key"], "gcal");
        assert_eq!(fields["_x_reason"], "calendar_app_home_connect");
        assert_eq!(fields.get("scope_group").map(String::as_str), scope);
    }
    assert_eq!(
        authorization_url(&json!({"ok":true,"authorization_url":"https://auth.example/connect"}))
            .unwrap(),
        "https://auth.example/connect"
    );
    for raw in [
        json!({"ok":false,"authorization_url":"https://auth.example/connect"}),
        json!({"ok":true,"authorization_url":"javascript:bad"}),
        json!({"ok":true}),
    ] {
        assert!(authorization_url(&raw).is_err());
    }
}
