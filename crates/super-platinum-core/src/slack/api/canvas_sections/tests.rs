use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

#[test]
fn original_wire_forms_are_comma_joined_without_reason() {
    let data: Value = serde_json::from_str(include_str!("original-wire.json")).unwrap();
    let workspace = WorkspaceSession {
        team_id: "Tfixture".into(),
        enterprise_id: None,
        user_id: "Ufixture".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    for vector in data["vectors"].as_array().unwrap() {
        let ids: Vec<String> =
            serde_json::from_value(vector["args"]["section_ids"].clone()).unwrap();
        let request =
            CanvasSectionsRequest::new(vector["args"]["file_id"].as_str().unwrap(), &ids).unwrap();
        let prepared = request.prepare(&SlackClient::default(), &workspace);
        assert_eq!(
            url::Url::parse(&prepared.url).unwrap().path(),
            vector["path"].as_str().unwrap()
        );
        let RequestBody::Form(fields) = prepared.body else {
            panic!("form")
        };
        // The client also owns authentication envelope fields; source fields
        // remain exact and no facade-only reason leaks onto the wire.
        for pair in vector["formEntries"].as_array().unwrap() {
            assert!(fields.contains(&(
                pair[0].as_str().unwrap().into(),
                pair[1].as_str().unwrap().into()
            )));
        }
        assert!(!fields.iter().any(|(name, _)| name == "reason"));
    }
}
#[test]
fn response_is_bound_to_request_and_malformed_later_entry_clears() {
    let request = CanvasSectionsRequest::new("F1", &["s1".into()]).unwrap();
    let response=request.decode(json!({"ok":true,"sections":[{"id":"s1","rich_text_blocks":[{"type":"rich_text","elements":[]}]},{"id":"bad"}]}));
    assert!(response.clears_file());
    assert_eq!(response.file_id(), "F1");
    assert!(!request.decode(json!({"ok":true})).clears_file());
    assert!(request.decode(json!({"ok":false})).clears_file());
    for ids in [vec![], vec!["s1,s2".into()], vec!["\n".into()]] {
        assert!(CanvasSectionsRequest::new("F1", &ids).is_err());
    }
    assert!(CanvasSectionsRequest::new("C1", &["s1".into()]).is_err());
}

#[test]
fn canonical_colon_ids_and_bounded_opaque_ids_roundtrip_without_form_splitting() {
    let ids = vec![
        "temp:C:0123456789abcdef01234567".to_owned(),
        "section / \u{65e5}\u{672c}".to_owned(),
    ];
    let request = CanvasSectionsRequest::new("F1", &ids).unwrap();
    assert_eq!(request.section_ids(), ids);
    let response=request.decode(json!({"ok":true,"sections":ids.iter().map(|id|json!({"id":id,"rich_text_blocks":[]})).collect::<Vec<_>>()}));
    assert!(!response.clears_file());
    assert_eq!(
        response
            .rows
            .unwrap()
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    for id in ["s1,s2".to_owned(), "s\n1".to_owned(), "x".repeat(257)] {
        assert!(CanvasSectionsRequest::new("F1", &[id.clone()]).is_err());
        assert!(
            request
                .decode(json!({"ok":true,"sections":[{"id":id,"rich_text_blocks":[]}]}))
                .clears_file()
        );
    }
}
