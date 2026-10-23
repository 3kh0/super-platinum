use super::*;
fn user() -> Value {
    json!({"id":"U1","is_restricted":false})
}
fn facts(user: &Value) -> Facts<'_> {
    Facts {
        team_id: "T1",
        top_level_team_id: "E1",
        self_member: user,
        posting: Flags {
            read_only: false,
            thread_only: false,
        },
        can_post: true,
        manage: Permission::Allowed,
    }
}
fn trigger() -> Value {
    json!({"id":"FtNew01","type":"shortcut","opaque":[1,2]})
}
fn channel() -> Value {
    json!({"id":"CADDDEST01","is_member":true,"properties":{"unrelated":true,"channel_workflows":[]}})
}
fn empty() -> Previews {
    Previews::decode(&[], json!({"ok":true,"triggers":[]})).unwrap()
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-secret".into(),
    }
}
#[test]
fn exact_factory_is_token_first_single_target_csv_and_inspection_redacts_token() {
    let add = Add::new(&trigger(), &channel(), &empty(), facts(&user())).unwrap();
    let req = add.request(&SlackClient::default(), &session()).unwrap();
    let RequestBody::Form(fields) = &req.request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "synthetic-secret".into()),
            ("trigger_ids".into(), "FtNew01".into()),
            ("channel_id".into(), "CADDDEST01".into()),
            ("_x_reason".into(), REASON.into())
        ]
    );
    assert!(
        !req.request
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
    );
    assert!(!req.inspect().to_string().contains("synthetic-secret"));
    assert!(!format!("{req:?}").contains("synthetic-secret"));
    let mut other = session();
    other.user_id = "U2".into();
    assert!(add.request(&SlackClient::default(), &other).is_err());
}
#[test]
fn limit_uses_actual_errors_not_preview_displayability_or_raw_array_length() {
    let ids = (0..16).map(|i| format!("Ft{i}")).collect::<Vec<_>>();
    let mut c = channel();
    c["properties"]["channel_workflows"] = ids
        .iter()
        .map(|id| json!({"workflow_trigger_id":id}))
        .collect();
    let mut rows = ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>();
    rows[0]["error"] = "not_found".into();
    rows[1]["error"] = "gone".into();
    rows[2]["error"] = "access_denied".into();
    rows[3]["isUnknown"] = true.into();
    rows[4]["isNonExistent"] = true.into();
    rows.pop(); // requested but missing response still counts
    let p = Previews::decode(&ids, json!({"ok":true,"triggers":rows})).unwrap();
    assert_eq!(validated_count(&c, &p).unwrap(), 14);
    assert_eq!(
        Add::new(&trigger(), &c, &p, facts(&user()))
            .unwrap()
            .validated_count(),
        14
    );
    let mut raw = p.raw().clone();
    raw["triggers"][0]["error"] = Value::Null;
    let p = Previews::decode(&ids, raw).unwrap();
    assert_eq!(validated_count(&c, &p).unwrap(), 15);
    assert!(Add::new(&trigger(), &c, &p, facts(&user())).is_err());
    assert!(validated_count(&c, &empty()).is_err()); // no fabricated acquired previews
}
#[test]
fn duplicate_filtered_error_never_authorizes_another_add() {
    let mut c = channel();
    c["properties"]["channel_workflows"] = json!([{"workflow_trigger_id":"FtNew01"}]);
    let p = Previews::decode(
        &["FtNew01".into()],
        json!({"ok":true,"triggers":[{"id":"FtNew01","error":"gone"}]}),
    )
    .unwrap();
    assert_eq!(validated_count(&c, &p).unwrap(), 0);
    assert!(
        matches!(Add::new(&trigger(),&c,&p,facts(&user())),Err(Error::Api(code)) if code=="workflow_feature_duplicate")
    );
}
#[test]
fn independent_permission_posting_membership_guest_and_host_guards() {
    let u = user();
    for kind in 0..4 {
        let mut f = facts(&u);
        match kind {
            0 => f.manage = Permission::Denied,
            1 => f.can_post = false,
            2 => f.posting.read_only = true,
            _ => f.posting.thread_only = true,
        };
        assert!(Add::new(&trigger(), &channel(), &empty(), f).is_err());
    }
    for (key, value) in [
        ("is_member", json!(false)),
        ("conversation_host_id", json!("TFOREIGN")),
        ("is_im", json!(true)),
        ("is_archived", json!(true)),
    ] {
        let mut c = channel();
        c[key] = value;
        assert!(Add::new(&trigger(), &c, &empty(), facts(&u)).is_err());
    }
    let mut guest = u.clone();
    guest["is_restricted"] = true.into();
    assert!(Add::new(&trigger(), &channel(), &empty(), facts(&guest)).is_err());
    for host in ["T1", "E1", ""] {
        let mut c = channel();
        c["conversation_host_id"] = host.into();
        assert!(Add::new(&trigger(), &c, &empty(), facts(&u)).is_ok());
    }
}
#[test]
fn malformed_unknown_and_nonshortcut_trigger_cannot_be_selected() {
    for (key, value) in [
        ("id", json!("bad,id")),
        ("type", json!("event")),
        ("isUnknown", json!(true)),
        ("isNonExistent", json!(true)),
        ("error", json!("access_denied")),
    ] {
        let mut t = trigger();
        t[key] = value;
        assert!(Add::new(&t, &channel(), &empty(), facts(&user())).is_err());
    }
    for v in [json!({}), json!([{}]), json!("wrong")] {
        let mut c = channel();
        c["properties"]["channel_workflows"] = v;
        assert!(Add::new(&trigger(), &c, &empty(), facts(&user())).is_err());
    }
}
#[test]
fn acknowledgment_needs_no_invented_collection_and_readback_is_independent() {
    let add = Add::new(&trigger(), &channel(), &empty(), facts(&user())).unwrap();
    assert!(acknowledge(&json!({"ok":true,"opaque":{"keep":1}})).is_ok());
    assert!(!add.observe_channel(&channel()).unwrap().present());
    let mut actual = channel();
    actual["properties"]["channel_workflows"] = json!([
        {"workflow_trigger_id":"FtOld","rank":"before"},{"workflow_trigger_id":"FtNew01","opaque":23}]);
    let read = add.observe_channel(&actual).unwrap();
    assert!(read.present());
    assert_eq!(read.raw(), &actual);
    assert_eq!(read.raw()["properties"]["unrelated"], true);
    // Observed presence after an Unknown is still not acknowledgment.
    for raw in [
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":"yes"}),
        json!({"ok":true,"error":"bad"}),
        json!({}),
    ] {
        assert!(acknowledge(&raw).is_err());
    }
    actual["id"] = "COTHER001".into();
    assert!(add.observe_channel(&actual).is_err());
}
