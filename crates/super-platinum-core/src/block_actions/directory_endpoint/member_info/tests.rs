use super::*;

fn owner(team: &str) -> WorkspaceSession {
    WorkspaceSession {
        team_id: team.into(),
        enterprise_id: Some("E00000001".into()),
        user_id: "USELF1234".into(),
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn read() -> Read {
    Read::capture(vec!["U00000001".into(), "U00000002".into()]).unwrap()
}

#[test]
fn exact_unconditional_request_keeps_logical_credentials_and_discovered_route() {
    let now = Instant::now();
    for (logical, route) in [
        ("T00000001", "T00000001"),
        ("T00000001", "E00000001"),
        ("E00000001", "E00000001"),
    ] {
        let owner = owner(logical);
        let endpoint=DirectoryEndpoint::decode(&owner,&json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{route}")}),now).unwrap();
        let request = endpoint
            .member_info(&SlackClient::default(), &owner, now, &read())
            .unwrap();
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/cache/{route}/users/info")
        );
        let crate::slack::client::RequestBody::Json(mut body) = request.body else {
            panic!("json");
        };
        assert_eq!(
            body.as_object_mut().unwrap().remove("token"),
            Some(json!("synthetic"))
        );
        assert_eq!(
            body.as_object_mut().unwrap().remove("enterprise_token"),
            if logical.starts_with('E') {
                Some(json!("synthetic"))
            } else {
                None
            }
        );
        assert_eq!(
            body,
            json!({"ids":["U00000001","U00000002"],"check_interaction":true,"include_profile_only_users":true})
        );
        let mut changed = owner.clone();
        changed.token = "new-synthetic".into();
        assert!(
            endpoint
                .member_info(&SlackClient::default(), &changed, now, &read())
                .is_err()
        );
        assert!(
            endpoint
                .member_info(
                    &SlackClient::default(),
                    &owner,
                    now + Duration::from_secs(60),
                    &read()
                )
                .is_err()
        );
    }
}

#[test]
fn raw_order_and_incomplete_outcomes_are_preserved_without_coverage_claim() {
    let raw = json!({"ok":true,"results":{"z":[{"id":"U00000002","profile":{"title":"retained"}}],"a":{"id":"U00000001"}},"pending_ids":["U00000001"],"invalid_ids":["U00000002"],"failed_ids":["U00000001"],"disconnected_ids":["U00000002"],"can_interact":{"U00000001":false},"next_marker":"more","extra":{"kept":true}});
    assert!(Page::decode(&read(), raw.clone(), None).is_err());
    let page = Page::decode(&read(), raw.clone(), Some(vec!["z".into(), "a".into()])).unwrap();
    assert_eq!(page.raw(), &raw);
    assert_eq!(
        page.rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["U00000002", "U00000001"]
    );
    assert_eq!(page.pending_ids(), ["U00000001"]);
    assert_eq!(page.invalid_ids(), ["U00000002"]);
    assert_eq!(page.failed_ids(), ["U00000001"]);
    assert_eq!(page.disconnected_ids(), ["U00000002"]);
    assert_eq!(page.interactions().get("U00000001"), Some(&false));
    assert_eq!(page.next_marker(), Some("more"));
    assert_eq!(read().next_page("more").unwrap().body()["marker"], "more");
}

#[test]
fn malformed_scope_envelope_keys_and_interaction_are_unavailable() {
    assert!(Read::capture(vec![]).is_err());
    assert!(Read::capture(vec!["U00000001".into(); 2]).is_err());
    assert!(Read::capture((0..101).map(|n| format!("U{n:08}")).collect()).is_err());
    assert!(Read::capture(vec!["C00000001".into()]).is_err());
    for error in [json!(false), json!(""), json!({})] {
        assert!(Page::decode(&read(), json!({"ok":true,"error":error}), None).is_err());
    }
    for raw in [
        json!({"ok":true,"results":true}),
        json!({"ok":true,"results":[{"id":"U99999999"}]}),
        json!({"ok":true,"can_interact":{"U00000001":null}}),
        json!({"ok":true,"pending_ids":["U99999999"]}),
    ] {
        assert!(Page::decode(&read(), raw, None).is_err());
    }
    assert!(
        Page::decode(
            &read(),
            json!({"ok":true,"results":{"z":[]}}),
            Some(vec!["z".into(), "z".into()])
        )
        .is_err()
    );
}

#[test]
fn final_interaction_projection_uses_explicit_prior_absence_not_missing_evidence() {
    let rows = vec![json!({"id":"U00000001","is_stranger":true})];
    assert!(project_interaction(&rows, &BTreeMap::new(), &BTreeMap::new()).is_err());
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["interactions"].as_array().unwrap() {
        let prior = case["prior"].as_bool();
        let incoming = case["incoming"].as_bool();
        let prior_map = BTreeMap::from([("U00000001".into(), prior)]);
        let incoming_map = incoming
            .map(|v| BTreeMap::from([("U00000001".into(), v)]))
            .unwrap_or_default();
        let projected = project_interaction(&rows, &incoming_map, &prior_map).unwrap();
        assert_eq!(projected, case["result"].as_array().unwrap().clone());
        assert_eq!(rows[0]["is_stranger"], true);
    }
}

#[test]
fn absent_producer_stays_zero_while_transport_admits_exclusive_conditional_mode() {
    let mut body = Read::absent(vec!["U00000001".into()]).unwrap().body();
    assert_eq!(body["updated_ids"], json!({"U00000001":0}));
    assert!(body.get("ids").is_none());
    body["token"] = json!("synthetic");
    assert!(valid_wire_body(&body));
    let row = crate::block_actions::workflow_management::member_hydration::Current {
        native: json!({}),
        source: json!({"id":"U00000001","updated":101}),
        interaction: None,
    };
    let mut conditional = Read::conditional(vec!["U00000001".into()], &[row])
        .unwrap()
        .body();
    conditional["token"] = json!("synthetic");
    assert_eq!(conditional["updated_ids"], json!({"U00000001":101}));
    assert!(valid_wire_body(&conditional));
    for value in [json!(-1), json!("0"), Value::Null, json!(false), json!(1.5)] {
        let mut invalid = body.clone();
        invalid["updated_ids"]["U00000001"] = value;
        assert!(!valid_wire_body(&invalid));
    }
    let mut invalid = body.clone();
    invalid["ids"] = json!(["U00000001"]);
    assert!(!valid_wire_body(&invalid));
    let mut invalid = body.clone();
    invalid["updated_ids"] = json!({});
    assert!(!valid_wire_body(&invalid));
    let mut invalid = body.clone();
    invalid["cannot_interact"] = json!(["U00000001"]);
    assert!(!valid_wire_body(&invalid));
    let unconditional = read().body();
    assert!(unconditional.get("updated_ids").is_none());
}
