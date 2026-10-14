use super::*;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn response() -> Value {
    json!({"ok":true,"result":{"data":{"user":{"id":"U1","profileSections":[{
    "sectionId":"PsHeader","id":"U1:section:PsHeader","type":"HEADER","opaque":{"keep":true},
    "profileElements":[
        {"elementId":"PeTitle","elementKey":"title","__typename":"ProfileTextElement","text":"Original","permissions":{"ui":true,"api":[],"scim":false}},
        {"elementId":"PePronunciation","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":"Sound","permissions":{"ui":false}},
        {"elementId":"PeRecording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F1","permissions":{"ui":false}}
    ]}]}}}})
}
fn header(v: &mut Value) -> &mut Value {
    &mut v["result"]["data"]["user"]["profileSections"][0]
}
fn snapshot(v: &Value) -> OwnProfileSections {
    OwnProfileSections::from_response(&workspace(), v).unwrap()
}
#[test]
fn header_write_preserves_known_readonly_siblings_and_only_changes_title() {
    let v = response();
    let s = snapshot(&v);
    assert!(s.title_capability().allowed());
    let intent = OwnProfileTitleIntent::capture(&s, "  Jose\u{301}  ").unwrap();
    assert_eq!(intent.desired_title(), "  José  ");
    let req =
        own_profile_title_request(&SlackClient::default(), &workspace(), &intent, &s).unwrap();
    let super::super::super::client::RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec!["token", "user", "section", "elements", "_x_reason"]
    );
    assert_eq!(fields[1].1, "U1");
    assert_eq!(fields[2].1, "PsHeader");
    assert_eq!(fields[4].1, "edit-profile-form-pane");
    assert_eq!(
        serde_json::from_str::<Value>(&fields[3].1).unwrap(),
        json!([
            {"element_id":"PeTitle","text":{"text":"  José  "}},
            {"element_id":"PePronunciation","text":{"text":"Sound"}},
            {"element_id":"PeRecording","file":{"fileId":"F1"}}
        ])
    );
    assert_eq!(s.raw(), &v);
    let get = own_profile_sections_request(&SlackClient::default(), &workspace());
    assert_eq!(
        url::Url::parse(&get.url).unwrap().path(),
        "/api/users.profile.getSections"
    );
}
#[test]
fn exact_preflight_pins_siblings_permissions_and_schema_not_unrelated_sections() {
    let s = snapshot(&response());
    let intent = OwnProfileTitleIntent::capture(&s, "New").unwrap();
    for kind in 0..4 {
        let mut v = response();
        match kind {
            0 => header(&mut v)["profileElements"][1]["text"] = json!("New pronunciation"),
            1 => header(&mut v)["profileElements"][0]["permissions"]["ui"] = json!(false),
            2 => header(&mut v)["opaque"] = json!({"keep":false}),
            _ => header(&mut v)["profileElements"][0]["elementId"] = json!("PeReplacement"),
        }
        assert!(intent.revalidate(&snapshot(&v)).is_err());
    }
    let mut v = response();
    v["result"]["data"]["user"]["profileSections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sectionId":"Other","type":"CUSTOM","anything":"preserved"}));
    assert!(intent.revalidate(&snapshot(&v)).is_ok());
    let mut w = workspace();
    w.team_id = "T2".into();
    assert!(own_profile_title_request(&SlackClient::default(), &w, &intent, &s).is_err());
}
#[test]
fn unknown_target_permissions_or_unrepresentable_sibling_never_grant_writes() {
    for kind in 0..6 {
        let mut v = response();
        match kind {
            0 => header(&mut v)["profileElements"][0]["permissions"] = json!({}),
            1 => header(&mut v)["profileElements"][0]["permissions"]["ui"] = json!(false),
            2 => header(&mut v)["profileElements"][1]["__typename"] = json!("FutureElement"),
            3 => header(&mut v)["profileElements"][2]["fileId"] = json!({"unknown":"shape"}),
            4 => header(&mut v)["profileElements"][1]["elementId"] = json!("PeTitle"),
            _ => header(&mut v)["profileElements"][1]["elementKey"] = json!("title"),
        }
        let s = snapshot(&v);
        assert!(!s.title_capability().allowed());
        assert!(OwnProfileTitleIntent::capture(&s, "New").is_err());
    }
}
#[test]
fn source_converter_preserves_nullish_defaults_and_scim_person_omission() {
    let v = json!({"profileElements":[
        {"elementId":"1","__typename":"ProfileTextElement","text":null},
        {"elementId":"2","__typename":"ProfileFileElement"},
        {"elementId":"3","__typename":"ProfileCurrentLocationElement","city":"City","country":null},
        {"elementId":"4","__typename":"ProfileTagsElement","tags":["One"]},
        {"elementId":"5","__typename":"ProfileDateElement"},
        {"elementId":"6","__typename":"ProfileLinkElement","uri":"https://example.invalid"},
        {"elementId":"7","__typename":"ProfileLongTextElement","blocks":"[]"},
        {"elementId":"8","__typename":"ProfilePersonElement","isScimManaged":true,"persons":["U2"]},
        {"elementId":"9","__typename":"ProfilePersonElement","persons":null}
    ]});
    assert_eq!(
        schema::convert(&v).unwrap(),
        vec![
            json!({"element_id":"1","text":{"text":""}}),
            json!({"element_id":"2","file":null}),
            json!({"element_id":"3","currentLocation":{"city":"City","country":"","state":"","description":""}}),
            json!({"element_id":"4","tags":{"tags":["One"]}}),
            json!({"element_id":"5","date":{"date":""}}),
            json!({"element_id":"6","link":{"displayText":"","uri":"https://example.invalid"}}),
            json!({"element_id":"7","long_text":{"blocks":"[]"}}),
            json!({"element_id":"9","person":{"persons":[]}})
        ]
    );
}
#[test]
fn readback_confirms_exact_owner_field_and_title_allowing_later_sibling_change() {
    let s = snapshot(&response());
    let intent = OwnProfileTitleIntent::capture(&s, "").unwrap();
    assert!(!intent.confirms_readback(&s));
    let mut v = response();
    header(&mut v)["profileElements"][0]["text"] = json!("");
    header(&mut v)["profileElements"][1]["text"] = json!("Concurrent sibling edit");
    assert!(intent.confirms_readback(&snapshot(&v)));
    let mut w = workspace();
    w.team_id = "T2".into();
    assert!(!intent.confirms_readback(&OwnProfileSections::from_response(&w, &v).unwrap()));
    header(&mut v)["profileElements"][0]["elementId"] = json!("Other");
    assert!(!intent.confirms_readback(&snapshot(&v)));
}
#[test]
fn wrong_owner_malformed_response_and_title_validation_fail_closed() {
    for v in [
        json!({"ok":true}),
        json!({"ok":false,"error":"not_allowed"}),
        json!({"ok":true,"error":"bad"}),
    ] {
        assert!(OwnProfileSections::from_response(&workspace(), &v).is_err());
    }
    let mut v = response();
    v["result"]["data"]["user"]["id"] = json!("U2");
    assert!(OwnProfileSections::from_response(&workspace(), &v).is_err());
    assert!(normalize_profile_title(&format!(" {} ", "a".repeat(249))).is_ok());
    assert!(normalize_profile_title(&"🦊".repeat(125)).is_err());
    assert_eq!(normalize_profile_title(" \u{feff} ").unwrap(), " \u{feff} ");
    assert!(normalize_profile_title(&format!("{}\u{0085}", "a".repeat(249))).is_err());
    let empty = OwnProfileTitleIntent::capture(&snapshot(&response()), "").unwrap();
    assert_eq!(empty.elements[0]["text"], json!({"text":""})); // Header clear is not text:null.
}
