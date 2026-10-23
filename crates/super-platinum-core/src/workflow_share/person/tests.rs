use super::*;

fn peer() -> Value {
    json!({"id":"UHUMAN001","team_id":"TWORKFLOW","profile":{"display_name":"Person"}})
}
fn channel() -> Value {
    json!({"id":"DDMPEER01","is_im":true,"user":"UHUMAN001","fromAnotherTeam":false})
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn open() -> Open {
    Open::new(&peer(), "TWORKFLOW", None).unwrap()
}

#[test]
fn ordinary_person_locality_requires_actual_facts_without_self_or_guest_ban() {
    let mut p = peer();
    assert!(eligible(&p, "TWORKFLOW"));
    p["is_restricted"] = json!(true);
    p["id"] = json!("USELF0001");
    assert!(eligible(&p, "TWORKFLOW"));
    p.as_object_mut().unwrap().remove("team_id");
    assert!(!eligible(&p, "TWORKFLOW"));
    p["enterprise_user"] = json!({"teams":["TWORKFLOW"]});
    assert!(eligible(&p, "TWORKFLOW"));
    p["enterprise_user"]["teams"] = json!(["TWORKFLOW", 5]);
    assert!(!eligible(&p, "TWORKFLOW"));
    for p in [Value::Null, json!([]), json!("UHUMAN001")] {
        assert!(!eligible(&p, "TWORKFLOW"));
    }
}
#[test]
fn ordinary_person_unsupported_and_malformed_peer_denials_are_explicit() {
    for key in [
        "deleted",
        "isUnknown",
        "isNonExistent",
        "is_bot",
        "is_app_user",
        "is_slackbot",
        "is_slacksecuritybot",
        "is_workflow_bot",
        "is_profile_only_user",
        "is_agentforce_bot",
        "isExternal",
        "is_stranger",
    ] {
        for value in [json!(true), json!("false"), json!(0)] {
            let mut p = peer();
            p[key] = value;
            assert!(!eligible(&p, "TWORKFLOW"), "{key}");
        }
    }
    for id in [
        "USLACKBOT",
        "UCHANNELEXPERT",
        "CCHANNEL1",
        "uHUMAN001",
        "U1",
        "",
    ] {
        let mut p = peer();
        p["id"] = json!(id);
        assert!(!eligible(&p, "TWORKFLOW"));
    }
    for (key, value) in [
        ("profile", json!([])),
        ("profile", json!({"api_app_id":"AAPP"})),
        ("bot_id", json!("BBOT")),
        ("team_id", json!(5)),
    ] {
        let mut p = peer();
        p[key] = value;
        assert!(!eligible(&p, "TWORKFLOW"));
    }
}
#[test]
fn person_open_wire_is_selection_time_numeric_quiet_and_users_xor_channel() {
    let request = open().request(&SlackClient::default(), &session()).unwrap();
    assert!(request.fields_current());
    assert_eq!(
        request.inspect()["fields"],
        json!([
            ["quiet", "1"],
            ["return_im", "1"],
            ["users", "UHUMAN001"],
            ["_x_reason", "startMpimByMemberIds"]
        ])
    );
    assert!(!format!("{request:?}").contains("synthetic"));
    let mut c = channel();
    c["is_open"] = json!(false);
    let existing = Open::new(&peer(), "TWORKFLOW", Some(&c)).unwrap();
    assert!(existing.reusable_channel().is_none());
    let req = existing
        .request(&SlackClient::default(), &session())
        .unwrap();
    assert_eq!(req.inspect()["fields"][2], json!(["channel", "DDMPEER01"]));
}
#[test]
fn actual_open_mapping_reuses_but_wrong_mapping_and_wrong_session_refuse() {
    let mut c = channel();
    c["is_open"] = json!(true);
    let existing = Open::new(&peer(), "TWORKFLOW", Some(&c)).unwrap();
    assert_eq!(existing.reusable_channel(), Some(&c));
    assert!(
        existing
            .request(&SlackClient::default(), &session())
            .is_err()
    );
    c["user"] = json!("UOTHER001");
    assert!(Open::new(&peer(), "TWORKFLOW", Some(&c)).is_err());
    let mut s = session();
    s.team_id = "TOTHER".into();
    assert!(open().request(&SlackClient::default(), &s).is_err());
}
#[test]
fn person_open_response_retains_actual_raw_and_does_not_invent_posting_permission() {
    let mut c = channel();
    c.as_object_mut().unwrap().remove("fromAnotherTeam");
    c["is_read_only"] = json!(true);
    c["is_open"] = json!(false);
    let raw = json!({"ok":true,"channel":c,"opaque":{"actual":[1,2]}});
    let result = open().decode(raw.clone()).unwrap();
    assert_eq!(result.raw(), &raw);
    assert_eq!(result.channel(), &raw["channel"]);
    assert_eq!(result.id(), "DDMPEER01");
    assert_eq!(result.peer_id(), "UHUMAN001");
    assert!(result.channel().get("is_member").is_none());
}
#[test]
fn person_open_response_requires_exact_peer_type_and_requested_existing_id() {
    for (key, value) in [
        ("id", json!("CCHANNEL1")),
        ("user", json!("UOTHER001")),
        ("is_im", json!(false)),
        ("is_group", json!(true)),
        ("is_private", json!(true)),
        ("is_open", json!("true")),
        ("properties", json!({"huddles":{"is_huddle_channel":true}})),
    ] {
        let mut c = channel();
        c[key] = value;
        assert!(
            open().decode(json!({"ok":true,"channel":c})).is_err(),
            "{key}"
        );
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":1,"channel":channel()}),
        json!({"ok":true,"error":"bad","channel":channel()}),
        json!({"ok":false,"error":"not_allowed"}),
    ] {
        assert!(open().decode(raw).is_err());
    }
    let existing = Open::new(&peer(), "TWORKFLOW", Some(&channel())).unwrap();
    let mut c = channel();
    c["id"] = json!("DOTHER001");
    assert!(existing.decode(json!({"ok":true,"channel":c})).is_err());
}
