use super::*;
use crate::block_actions::posting_policy::Flags;

fn peers() -> Vec<Value> {
    vec![
        json!({"id":"UHUMAN002","team_id":"TWORKFLOW"}),
        json!({"id":"UHUMAN001","team_id":"TWORKFLOW"}),
    ]
}
fn channel() -> Value {
    json!({"id":"GSHARE001","is_mpim":true,"is_private":true,"fromAnotherTeam":false,
        "members":["UHUMAN001","USELF0001","UHUMAN002"],"opaque":{"original":true}})
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-group-token".into(),
    }
}
fn open() -> Open {
    Open::new(&peers(), "TWORKFLOW", "USELF0001", None).unwrap()
}

#[test]
fn group_factory_deduplicates_in_order_and_keeps_self_first() {
    let mut p = peers();
    p.push(p[0].clone());
    let o = Open::new(&p, "TWORKFLOW", "USELF0001", None).unwrap();
    assert_eq!(o.peers().len(), 2);
    assert_eq!(o.member_ids(), ["USELF0001", "UHUMAN002", "UHUMAN001"]);
    let r = o.request(&SlackClient::default(), &session()).unwrap();
    assert_eq!(
        r.inspect()["fields"],
        json!([
            ["quiet", "1"],
            ["return_im", "1"],
            ["users", "USELF0001,UHUMAN002,UHUMAN001"],
            ["_x_reason", "startMpimByMemberIds"]
        ])
    );
    assert!(r.fields_current());
    assert!(!format!("{r:?}").contains("synthetic-group-token"));
    assert!(!r.inspect().to_string().contains("synthetic-group-token"));
}
#[test]
fn group_limits_do_not_turn_single_person_or_conflicting_facts_into_mpim() {
    assert!(Open::new(&peers()[..1], "TWORKFLOW", "USELF0001", None).is_err());
    let mut p = peers();
    p[0]["id"] = json!("USELF0001");
    assert!(Open::new(&p, "TWORKFLOW", "USELF0001", None).is_err());
    let mut p = peers();
    let mut duplicate = p[0].clone();
    duplicate["name"] = json!("changed");
    p.push(duplicate);
    assert!(Open::new(&p, "TWORKFLOW", "USELF0001", None).is_err());
    let mut p = (1..=8)
        .map(|i| json!({"id":format!("UHUMAN{i:03}"),"team_id":"TWORKFLOW"}))
        .collect::<Vec<_>>();
    assert_eq!(
        Open::new(&p, "TWORKFLOW", "USELF0001", None)
            .unwrap()
            .member_ids()
            .len(),
        9
    );
    p.push(json!({"id":"UHUMAN009","team_id":"TWORKFLOW"}));
    assert!(Open::new(&p, "TWORKFLOW", "USELF0001", None).is_err());
}
#[test]
fn group_peer_and_session_replacement_refused() {
    for (field, value) in [
        ("deleted", json!(true)),
        ("is_bot", json!(true)),
        ("is_stranger", json!(true)),
        ("team_id", json!("TOTHER")),
    ] {
        let mut p = peers();
        p[1][field] = value;
        assert!(Open::new(&p, "TWORKFLOW", "USELF0001", None).is_err());
    }
    for n in 0..3 {
        let mut s = session();
        match n {
            0 => s.team_id = "TOTHER".into(),
            1 => s.user_id = "UOTHER001".into(),
            _ => s.token.clear(),
        };
        assert!(open().request(&SlackClient::default(), &s).is_err());
    }
}
#[test]
fn group_open_reuses_only_actual_open_exact_set_and_closed_requests_users() {
    let mut c = channel();
    c["is_open"] = json!(true);
    let o = Open::new(&peers(), "TWORKFLOW", "USELF0001", Some(&c)).unwrap();
    assert_eq!(o.reusable_channel(), Some(&c));
    assert!(o.request(&SlackClient::default(), &session()).is_err());
    c["is_open"] = json!(false);
    let o = Open::new(&peers(), "TWORKFLOW", "USELF0001", Some(&c)).unwrap();
    assert!(o.reusable_channel().is_none());
    let r = o.request(&SlackClient::default(), &session()).unwrap();
    assert_eq!(
        r.inspect()["fields"][2],
        json!(["users", "USELF0001,UHUMAN002,UHUMAN001"])
    );
    let mut actual = c.clone();
    actual["id"] = json!("GSHARE002");
    assert_eq!(
        o.decode(json!({"ok":true,"channel":actual})).unwrap().id(),
        "GSHARE002"
    );
    c.as_object_mut().unwrap().remove("fromAnotherTeam");
    assert!(Open::new(&peers(), "TWORKFLOW", "USELF0001", Some(&c)).is_err());
}
#[test]
fn group_decode_retains_original_order_opaque_fields_and_no_invented_flags() {
    let mut c = channel();
    c.as_object_mut().unwrap().remove("fromAnotherTeam");
    let raw = json!({"ok":true,"channel":c,"opaque_receipt":"original"});
    let decoded = open().decode(raw.clone()).unwrap();
    assert_eq!(decoded.raw(), &raw);
    assert_eq!(decoded.channel(), &raw["channel"]);
    assert_eq!(
        decoded.member_ids(),
        ["UHUMAN001", "USELF0001", "UHUMAN002"]
    );
    assert!(decoded.channel().get("is_member").is_none());
}
#[test]
fn group_sparse_duplicate_extra_wrong_and_malformed_members_never_ack() {
    for members in [
        Value::Null,
        json!([]),
        json!(["USELF0001", "UHUMAN001"]),
        json!(["USELF0001", "UHUMAN001", "UHUMAN001"]),
        json!(["USELF0001", "UHUMAN001", "UHUMAN002", "UOTHER001"]),
        json!(["USELF0001", "UHUMAN001", "UOTHER001"]),
        json!(["USELF0001", 5, "UHUMAN002"]),
    ] {
        let mut c = channel();
        c["members"] = members;
        assert!(open().decode(json!({"ok":true,"channel":c})).is_err());
        assert!(Open::new(&peers(), "TWORKFLOW", "USELF0001", Some(&c)).is_err());
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":false,"channel":channel()}),
        json!({"ok":1,"channel":channel()}),
        json!({"ok":true,"error":"denied","channel":channel()}),
    ] {
        assert!(open().decode(raw).is_err());
    }
}
#[test]
fn group_wrong_type_external_huddle_and_malformed_flags_refused() {
    for (key, value) in [
        ("id", json!("DDMPEER01")),
        ("is_mpim", json!(false)),
        ("is_im", json!(true)),
        ("user", json!("UHUMAN001")),
        ("is_archived", json!(true)),
        ("is_ext_shared", json!(true)),
        ("fromAnotherTeam", json!(true)),
        ("is_open", json!("true")),
        ("is_read_only", json!(0)),
        ("properties", json!({"huddles":{"is_huddle_channel":true}})),
    ] {
        let mut c = channel();
        c[key] = value;
        assert!(open().decode(json!({"ok":true,"channel":c})).is_err());
    }
}
#[test]
fn group_membership_and_open_ack_do_not_grant_posting() {
    let allow = Flags {
        read_only: false,
        thread_only: false,
    };
    assert!(eligible(
        &channel(),
        allow,
        "TWORKFLOW",
        "USELF0001",
        &peers()
    ));
    for flags in [
        Flags {
            read_only: true,
            thread_only: false,
        },
        Flags {
            read_only: false,
            thread_only: true,
        },
    ] {
        assert!(!eligible(
            &channel(),
            flags,
            "TWORKFLOW",
            "USELF0001",
            &peers()
        ));
    }
    let mut c = channel();
    c.as_object_mut().unwrap().remove("fromAnotherTeam");
    assert!(!eligible(&c, allow, "TWORKFLOW", "USELF0001", &peers()));
}

#[test]
fn group_independent_posting_response_can_omit_but_cannot_replace_membership() {
    let original = channel();
    let mut posting = original.clone();
    posting.as_object_mut().unwrap().remove("members");
    posting.as_object_mut().unwrap().remove("fromAnotherTeam");
    let allow = Flags {
        read_only: false,
        thread_only: false,
    };
    assert!(eligible_with_membership(
        &posting,
        &original,
        allow,
        "TWORKFLOW",
        "USELF0001",
        &peers()
    ));
    assert!(!eligible(
        &posting,
        allow,
        "TWORKFLOW",
        "USELF0001",
        &peers()
    ));
    for (key, value) in [
        ("id", json!("GOTHER001")),
        ("is_mpim", json!(false)),
        ("is_im", json!(true)),
        ("is_archived", json!(true)),
        ("is_ext_shared", json!(true)),
        ("members", Value::Null),
        ("members", json!(["USELF0001", "UHUMAN001", "UOTHER001"])),
    ] {
        let mut bad = posting.clone();
        bad[key] = value;
        assert!(!eligible_with_membership(
            &bad,
            &original,
            allow,
            "TWORKFLOW",
            "USELF0001",
            &peers()
        ));
    }
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("members");
    assert!(!eligible_with_membership(
        &posting,
        &missing,
        allow,
        "TWORKFLOW",
        "USELF0001",
        &peers()
    ));
    assert!(!eligible_with_membership(
        &posting,
        &original,
        Flags {
            read_only: true,
            thread_only: false
        },
        "TWORKFLOW",
        "USELF0001",
        &peers()
    ));
}
