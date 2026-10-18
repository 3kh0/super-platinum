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
fn raw() -> Value {
    json!({"ok":true,"result":{"data":{"user":{"id":"U1","isSelf":true,"profileSections":[{
        "sectionId":"H1","type":"HEADER","canEdit":false,"profileElements":[
            {"elementId":"name","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":"Old","permissions":{"ui":true}},
            {"elementId":"title","elementKey":"title","__typename":"ProfileTextElement","text":"","permissions":{"ui":false}},
            {"elementId":"recording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F1","permissions":{"ui":false}}
        ]}
    ]}}}})
}
fn header(v: &mut Value) -> &mut Value {
    &mut v["result"]["data"]["user"]["profileSections"][0]
}
fn snapshot(v: &Value) -> OwnProfileSections {
    OwnProfileSections::from_response(&workspace(), v).unwrap()
}
fn draft(v: &Value) -> SectionDraft {
    SectionDraft::open_pronunciation(&snapshot(v)).unwrap()
}

#[test]
fn pronunciation_is_closed_header_scope_and_clear_preserves_empty_text_and_file() {
    let source = raw();
    let s = snapshot(&source);
    assert!(SectionDraft::open(&s, "H1").is_err());
    let mut d = draft(&source);
    assert_eq!(d.fields().len(), 1);
    assert!(d.fields()[0].editable);
    assert!(d.capture().is_err());
    assert!(
        d.change("title", SectionValue::Text("Unrelated".into()))
            .is_err()
    );
    d.change("name", SectionValue::Text(String::new())).unwrap();
    assert_eq!(
        d.capture().unwrap().elements,
        json!([
            {"element_id":"name","text":{"text":""}},
            {"element_id":"title","text":{"text":""}},
            {"element_id":"recording","file":{"fileId":"F1"}}
        ])
    );
    let intent = d.capture().unwrap();
    let request = section_request(&SlackClient::default(), &workspace(), &intent, &s).unwrap();
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("user".into(), "U1".into()),
            ("section".into(), "H1".into()),
            ("elements".into(), intent.elements.to_string()),
            ("_x_reason".into(), "edit-profile-form-pane".into())
        ]
    );
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(section_request(&SlackClient::default(), &other, &intent, &s).is_err());
}
#[test]
fn pronunciation_nfc_js_trimmed_utf16_bound_preserves_transmitted_spaces() {
    let mut d = draft(&raw());
    d.change("name", SectionValue::Text(" e\u{301} ".into()))
        .unwrap();
    assert_eq!(d.capture().unwrap().elements[0]["text"]["text"], " \u{e9} ");
    for (text, valid) in [
        (format!("\u{feff}{}  ", "\u{1f680}".repeat(128)), true),
        ("\u{1f680}".repeat(129), false),
        (format!("{}\u{0085}", "x".repeat(256)), false),
        (" ".repeat(300), true),
    ] {
        d.change("name", SectionValue::Text(text)).unwrap();
        assert_eq!(d.validation("name").is_ok(), valid);
        assert_eq!(d.capture().is_ok(), valid);
    }
}
#[test]
fn pronunciation_permission_identity_and_unknown_siblings_refuse_without_conversion_loss() {
    for permission in [Value::Null, json!(false), json!("true")] {
        let mut source = raw();
        header(&mut source)["profileElements"][0]["permissions"]["ui"] = permission;
        let mut d = draft(&source);
        assert!(!d.fields()[0].editable);
        assert!(d.change("name", SectionValue::Text("New".into())).is_err());
    }
    for kind in 0..5 {
        let mut source = raw();
        match kind {
            0 => {
                header(&mut source)["profileElements"][0]["__typename"] =
                    json!("ProfileFileElement")
            }
            1 => header(&mut source)["profileElements"][1]["elementId"] = json!("name"),
            2 => {
                header(&mut source)["profileElements"][1]["elementKey"] =
                    json!("Name Pronunciation")
            }
            3 => source["result"]["data"]["user"]["isSelf"] = json!(false),
            _ => header(&mut source)["profileElements"][0]["text"] = json!({}),
        }
        assert!(SectionDraft::open_pronunciation(&snapshot(&source)).is_err());
    }
    let mut source = raw();
    header(&mut source)["profileElements"][2]["__typename"] = json!("UnknownNewField");
    let mut d = draft(&source);
    d.change("name", SectionValue::Text("New".into())).unwrap();
    assert!(d.capture().is_err());
}
#[test]
fn pronunciation_exact_preflight_and_target_only_positive_readback() {
    let source = raw();
    let mut d = draft(&source);
    d.change("name", SectionValue::Text("New".into())).unwrap();
    let intent = d.capture().unwrap();
    assert!(intent.revalidate(&snapshot(&source)).is_ok());
    let mut ambiguous = source.clone();
    let mut second = header(&mut ambiguous).clone();
    second["sectionId"] = json!("H2");
    ambiguous["result"]["data"]["user"]["profileSections"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let ambiguous = snapshot(&ambiguous);
    assert!(intent.revalidate(&ambiguous).is_err());
    assert!(section_request(&SlackClient::default(), &workspace(), &intent, &ambiguous).is_err());
    let mut denied = source.clone();
    header(&mut denied)["profileElements"][0]["permissions"]["ui"] = json!(false);
    assert!(intent.revalidate(&snapshot(&denied)).is_err());
    let mut fresh = source.clone();
    header(&mut fresh)["profileElements"][1]["text"] = json!("New title");
    assert!(intent.revalidate(&snapshot(&fresh)).is_err());
    assert!(!intent.confirms_readback(&snapshot(&fresh)));
    header(&mut fresh)["profileElements"][0]["text"] = json!("New");
    assert!(intent.confirms_readback(&snapshot(&fresh)));
    let reopened = intent.draft().reopen(&snapshot(&fresh)).unwrap();
    assert_eq!(reopened.fields().len(), 1);
    assert!(!reopened.dirty());
    header(&mut fresh)["profileElements"][0]["elementId"] = json!("replaced");
    assert!(!intent.confirms_readback(&snapshot(&fresh)));
    let mut foreign = workspace();
    foreign.team_id = "T2".into();
    let other = OwnProfileSections::from_response(&foreign, &fresh).unwrap();
    assert!(intent.revalidate(&other).is_err());
    assert!(!intent.confirms_readback(&other));
}
