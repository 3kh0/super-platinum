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
    let mut fields = vec![
        json!({"elementId":"text","__typename":"ProfileTextElement","text":"Old"}),
        json!({"elementId":"date","__typename":"ProfileDateElement","date":"2024-02-29"}),
        json!({"elementId":"link","__typename":"ProfileLinkElement","displayText":"Site","uri":"https://example.com"}),
        json!({"elementId":"tags","__typename":"ProfileTagsElement","tags":["One","Two"]}),
        json!({"elementId":"people","__typename":"ProfilePersonElement","persons":["U2"],"isScimManaged":false}),
        json!({"elementId":"location","__typename":"ProfileCurrentLocationElement","city":"City","state":"NY","country":"US","description":"Home"}),
        json!({"elementId":"long","__typename":"ProfileLongTextElement","blocks":"[]"}),
        json!({"elementId":"file","__typename":"ProfileFileElement","fileId":"F1","permissions":{"ui":false}}),
    ];
    for f in &mut fields {
        if f.get("permissions").is_none() {
            f["permissions"] = json!({"ui":true});
        }
        f["label"] = f["elementId"].clone();
        f["id"] = json!(format!(
            "U1:section:S1:{}",
            f["elementId"].as_str().unwrap()
        ));
    }
    json!({"ok":true,"result":{"data":{"user":{"id":"U1","profileSections":[{
        "sectionId":"S1","id":"U1:section:S1","type":"CUSTOM","label":"About","canEdit":true,"profileElements":fields,
        "opaque":{"preserve":true}}]}}}})
}
fn snapshot(v: &Value) -> OwnProfileSections {
    OwnProfileSections::from_response(&workspace(), v).unwrap()
}
fn section(v: &mut Value) -> &mut Value {
    &mut v["result"]["data"]["user"]["profileSections"][0]
}
fn draft(v: &Value) -> SectionDraft {
    SectionDraft::open(&snapshot(v), "S1").unwrap()
}

#[test]
fn full_section_wire_preserves_order_readonly_raw_and_exact_null_clears() {
    let source = response();
    let mut d = draft(&source);
    for (id, value) in [
        ("text", SectionValue::Text(String::new())),
        ("date", SectionValue::Date(String::new())),
        (
            "link",
            SectionValue::Link {
                display: "ignored on clear".into(),
                uri: String::new(),
            },
        ),
        ("tags", SectionValue::Tags(vec![])),
        ("people", SectionValue::Persons(vec![])),
    ] {
        d.change(id, value).unwrap();
    }
    let intent = d.capture().unwrap();
    assert_eq!(
        intent.elements,
        json!([
            {"element_id":"text","text":null},{"element_id":"date","date":null},{"element_id":"link","link":null},
            {"element_id":"tags","tags":{"tags":[]}},{"element_id":"people","person":null},
            {"element_id":"location","currentLocation":{"city":"City","state":"NY","country":"US","description":"Home"}},
            {"element_id":"long","long_text":{"blocks":"[]"}},{"element_id":"file","file":{"fileId":"F1"}}
        ])
    );
    assert_eq!(d.snapshot.raw(), &source);
    let request = section_request(
        &SlackClient::default(),
        &workspace(),
        &intent,
        &snapshot(&source),
    )
    .unwrap();
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["token", "user", "section", "elements", "_x_reason"]
    );
    assert_eq!(fields[3].1, intent.elements.to_string());
}
#[test]
fn options_utf16_dates_tags_and_locations_keep_invalid_edits_for_correction() {
    let mut v = response();
    section(&mut v)["profileElements"][0]["options"] = json!(["A", "B"]);
    let mut d = draft(&v);
    d.change("text", SectionValue::Text("C".into())).unwrap();
    assert!(d.capture().is_err());
    d.change("text", SectionValue::Text("B".into())).unwrap();
    assert!(d.capture().is_ok());
    d.change("date", SectionValue::Date("2025-02-29".into()))
        .unwrap();
    assert!(d.capture().is_err());
    d.change("date", SectionValue::Date("2028-02-29".into()))
        .unwrap();
    assert!(d.capture().is_ok());
    d.change("tags", SectionValue::Tags(vec!["x".into(); 76]))
        .unwrap();
    assert!(d.capture().is_err());
    d.change("tags", SectionValue::Tags(vec!["Two".into(), "One".into()]))
        .unwrap();
    assert!(d.capture().is_ok());
    let mut d = draft(&response());
    d.change("text", SectionValue::Text("\u{1f600}".repeat(128)))
        .unwrap();
    assert!(d.capture().is_ok());
    d.change("text", SectionValue::Text("\u{1f600}".repeat(129)))
        .unwrap();
    assert!(d.capture().is_err());
}
#[test]
fn contact_email_and_scim_person_are_omitted_not_mutated() {
    let mut v = response();
    let s = section(&mut v);
    s["type"] = json!("CONTACT");
    s["profileElements"][0]["elementKey"] = json!("email");
    s["profileElements"][4]["isScimManaged"] = json!(true);
    let mut d = draft(&v);
    assert!(d.change("text", SectionValue::Text("new".into())).is_err());
    assert!(d.change("people", SectionValue::Persons(vec![])).is_err());
    d.change("tags", SectionValue::Tags(vec![])).unwrap();
    let i = d.capture().unwrap();
    assert!(
        !i.elements
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["element_id"] == "text" || e["element_id"] == "people")
    );
}
#[test]
fn schema_permission_owner_and_unknown_siblings_refuse_before_dispatch() {
    let mut d = draft(&response());
    d.change("text", SectionValue::Text("New".into())).unwrap();
    let i = d.capture().unwrap();
    for kind in 0..4 {
        let mut v = response();
        let s = section(&mut v);
        match kind {
            0 => s["canEdit"] = json!(false),
            1 => s["profileElements"][1]["date"] = json!("2025-01-01"),
            2 => s["profileElements"][0]["permissions"]["ui"] = json!(false),
            _ => s["opaque"] = json!(null),
        };
        assert!(i.revalidate(&snapshot(&v)).is_err());
    }
    let mut not_self = response();
    not_self["result"]["data"]["user"]["isSelf"] = json!(false);
    assert!(i.revalidate(&snapshot(&not_self)).is_err());
    let mut w = workspace();
    w.user_id = "U2".into();
    assert!(section_request(&SlackClient::default(), &w, &i, &snapshot(&response())).is_err());
    let mut v = response();
    section(&mut v)["profileElements"][7]["__typename"] = json!("Future");
    let mut d = draft(&v);
    d.change("text", SectionValue::Text("New".into())).unwrap();
    assert!(d.capture().is_err());
}
#[test]
fn readback_requires_changed_fields_but_permits_later_unrelated_sibling_change() {
    let mut d = draft(&response());
    d.change("text", SectionValue::Text("New".into())).unwrap();
    let i = d.capture().unwrap();
    assert!(!i.confirms_readback(&snapshot(&response())));
    let mut v = response();
    section(&mut v)["profileElements"][0]["text"] = json!("New");
    section(&mut v)["profileElements"][1]["date"] = json!("2025-01-01");
    assert!(i.confirms_readback(&snapshot(&v)));
    section(&mut v)["profileElements"][0]["elementId"] = json!("Other");
    assert!(!i.confirms_readback(&snapshot(&v)));
}
#[test]
fn long_text_is_structured_and_limited_without_flattening_originals() {
    let mut d = draft(&response());
    let blocks = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Bold","style":{"bold":true}},{"type":"link","url":"https://example.com","text":"Example"}]}]}]);
    d.change(
        "long",
        SectionValue::LongText(blocks.as_array().unwrap().clone()),
    )
    .unwrap();
    let i = d.capture().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(i.elements[6]["long_text"]["blocks"].as_str().unwrap())
            .unwrap(),
        blocks
    );
    let huge = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"x".repeat(5001)}]}]}]);
    d.change(
        "long",
        SectionValue::LongText(huge.as_array().unwrap().clone()),
    )
    .unwrap();
    assert!(d.capture().is_err());
}

#[test]
fn section_and_metadata_identity_cannot_grant_editing_to_another_field() {
    let mut raw = response();
    raw["result"]["data"]["user"]["isSelf"] = json!(false);
    assert!(SectionDraft::open(&snapshot(&raw), "S1").is_err());
    let mut raw = response();
    section(&mut raw)["id"] = json!("U1:section:Other");
    assert!(SectionDraft::open(&snapshot(&raw), "S1").is_err());
    let mut raw = response();
    section(&mut raw)["profileElements"][0]["id"] = json!("U1:section:S1:other");
    let mut d = draft(&raw);
    assert!(!d.fields()[0].editable);
    assert!(d.change("text", SectionValue::Text("New".into())).is_err());
}
#[test]
fn current_location_uses_captured_codes_and_exact_stock_minimum() {
    let mut d = draft(&response());
    let location = |city: &str, country: &str| SectionValue::Location {
        city: city.into(),
        country: country.into(),
        description: "Home".into(),
        state: "NY".into(),
    };
    d.change("location", location("LA", "US")).unwrap();
    assert!(d.capture().is_ok());
    for value in [
        location("L", "US"),
        location("", "US"),
        location("LA", "USA"),
    ] {
        d.change("location", value).unwrap();
        assert!(d.capture().is_err());
    }
    d.change(
        "location",
        SectionValue::Location {
            city: String::new(),
            country: String::new(),
            description: String::new(),
            state: String::new(),
        },
    )
    .unwrap();
    assert!(
        d.capture().is_err(),
        "captured Yup min(2) rejects empty despite optional labels"
    );
    assert_eq!(countries().len(), 239);
    assert_eq!(countries().first().unwrap().code, "AF");
    assert_eq!(countries().last().unwrap().code, "ZW");
    assert_eq!(
        countries()
            .iter()
            .map(|c| &c.code)
            .collect::<HashSet<_>>()
            .len(),
        239
    );
}
#[test]
fn tags_use_captured_request_keys_ecmascript_trim_and_strict_response() {
    let request = tags_request(
        &SlackClient::default(),
        &workspace(),
        "tags",
        "\u{feff} a \u{a0}",
    )
    .unwrap();
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("element_id".into(), "tags".into()),
            ("query".into(), "a".into()),
            ("_x_reason".into(), "search.autocomplete.tags.tags".into())
        ]
    );
    assert_eq!(trim_tag_query("\u{85}x\u{85}"), "\u{85}x\u{85}");
    assert!(tags_request(&SlackClient::default(), &workspace(), "tags", "\u{feff}").is_err());
    assert_eq!(
        decode_tags(&json!({"ok":true,"tags":["A","a","A"]})).unwrap(),
        ["A", "a", "A"]
    );
    for value in [
        json!({"ok":true,"error":"invalid_auth","tags":[]}),
        json!({"ok":true,"tags":[null]}),
        json!({"ok":true}),
        json!({"ok":false,"tags":[]}),
    ] {
        assert!(decode_tags(&value).is_err());
    }
}
#[test]
fn link_clear_readback_uses_wire_null_and_text_keeps_original_unicode() {
    let mut d = draft(&response());
    d.change(
        "link",
        SectionValue::Link {
            display: "Retained label".into(),
            uri: String::new(),
        },
    )
    .unwrap();
    d.change("text", SectionValue::Text("e\u{301}".into()))
        .unwrap();
    let intent = d.capture().unwrap();
    assert_eq!(intent.elements[0]["text"]["text"], "e\u{301}");
    let mut raw = response();
    section(&mut raw)["profileElements"][0]["text"] = json!("e\u{301}");
    section(&mut raw)["profileElements"][2]["uri"] = json!(null);
    section(&mut raw)["profileElements"][2]["displayText"] = json!(null);
    assert!(intent.confirms_readback(&snapshot(&raw)));
}
#[test]
fn rich_length_honors_existing_newlines_merged_links_and_code_context() {
    let length = |elements: Value| {
        rich::measured_length(&[json!({"type":"rich_text","elements":elements})]).unwrap()
    };
    assert_eq!(
        length(json!([{"type":"rich_text_section","elements":[{"type":"text","text":"abc\n"}]}])),
        3
    );
    assert_eq!(
        length(
            json!([{"type":"rich_text_section","elements":[{"type":"text","text":"abc\n"}]},{"type":"rich_text_section","elements":[{"type":"text","text":"d"}]}])
        ),
        5
    );
    assert_eq!(
        length(
            json!([{"type":"rich_text_section","elements":[{"type":"link","url":"https://example.com","text":"https://"},{"type":"link","url":"https://example.com","text":"example.com"}]}])
        ),
        19
    );
    assert_eq!(
        length(
            json!([{"type":"rich_text_preformatted","elements":[{"type":"link","url":"https://example.com","text":"Label"}]}])
        ),
        5
    );
    assert_eq!(
        length(
            json!([{"type":"rich_text_list","style":"bullet","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"a\nb"}]},{"type":"rich_text_section","elements":[{"type":"text","text":"c"}]}]}])
        ),
        4
    );
}
