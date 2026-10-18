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
    json!({"ok":true,"result":{"data":{"user":{"id":"U1","isSelf":true,"profileSections":[{"sectionId":"H1","type":"HEADER","canEdit":false,"profileElements":[
        {"elementId":"name","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":"Zo-ee","permissions":{"ui":false}},
        {"elementId":"title","elementKey":"title","__typename":"ProfileTextElement","text":"","permissions":{"ui":false}},
        {"elementId":"recording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F00000001","permissions":{"ui":false}}
    ]}]}}}})
}
fn snapshot(raw: &Value) -> OwnProfileSections {
    OwnProfileSections::from_response(&workspace(), raw).unwrap()
}
fn fields(r: PreparedRequest) -> Vec<(String, String)> {
    let crate::slack::client::RequestBody::Form(v) = r.body else {
        panic!("form")
    };
    v
}
#[test]
fn recording_replace_and_clear_preserve_ordered_header_siblings() {
    let snapshot = snapshot(&raw());
    let mut draft = RecordingDraft::open(&snapshot).unwrap();
    assert!(!draft.dirty());
    assert!(
        draft
            .request(&SlackClient::default(), &workspace(), &snapshot)
            .is_err()
    );
    draft.replace("F00000002").unwrap();
    for expected in [json!({"fileId":"F00000002"}), Value::Null] {
        let payload = fields(
            draft
                .request(&SlackClient::default(), &workspace(), &snapshot)
                .unwrap(),
        );
        let elements: Value =
            serde_json::from_str(&payload.iter().find(|(k, _)| k == "elements").unwrap().1)
                .unwrap();
        assert_eq!(
            elements,
            json!([
                {"element_id":"name","text":{"text":"Zo-ee"}},
                {"element_id":"title","text":{"text":""}},
                {"element_id":"recording","file":expected}
            ])
        );
        draft.remove();
    }
}
#[test]
fn recording_fresh_self_identity_and_sibling_conflict_are_required() {
    let baseline = snapshot(&raw());
    let mut draft = RecordingDraft::open(&baseline).unwrap();
    draft.remove();
    for mode in 0..7 {
        let mut value = raw();
        let header = &mut value["result"]["data"]["user"]["profileSections"][0];
        match mode {
            0 => header["profileElements"][0]["text"] = json!("Changed elsewhere"),
            1 => header["profileElements"][1]["__typename"] = json!("FutureField"),
            2 => header["profileElements"][2]["elementId"] = json!("title"),
            3 => header["profileElements"][2]["__typename"] = json!("ProfileTextElement"),
            4 => header["profileElements"][2]["elementKey"] = json!("Other file"),
            5 => value["result"]["data"]["user"]["isSelf"] = json!(false),
            _ => value["result"]["data"]["user"]["isSelf"] = Value::Null,
        }
        assert!(draft.revalidate(&snapshot(&value)).is_err());
    }
    let mut other = workspace();
    other.user_id = "U2".into();
    assert!(
        draft
            .request(&SlackClient::default(), &other, &baseline)
            .is_err()
    );
}
#[test]
fn recording_positive_readback_is_identity_bound_and_never_authorizes_replay() {
    let mut draft = RecordingDraft::open(&snapshot(&raw())).unwrap();
    draft.remove();
    assert!(!draft.confirms(&snapshot(&raw())));
    let mut changed = raw();
    changed["result"]["data"]["user"]["profileSections"][0]["profileElements"][2]["fileId"] =
        Value::Null;
    assert!(draft.confirms(&snapshot(&changed)));
    // A readback match does not relax the original write preflight.
    assert!(draft.revalidate(&snapshot(&changed)).is_err());
    for id in ["", "C00000001", "Fshort", "F0000000/", "F0000000a"] {
        assert!(draft.replace(id).is_err());
    }
}
#[test]
fn recording_private_upload_wire_carries_subtype_without_channel_or_message() {
    let c = SlackClient::default();
    let w = workspace();
    let ticket=UploadTicket::decode(&json!({"ok":true,"file":"F00000002","upload_url":"https://files.slack.com/upload/v1/opaque"})).unwrap();
    let upload = fields(ticket_request(&c, &w, 20).unwrap());
    assert_eq!(
        upload,
        vec![
            ("token".into(), "synthetic".into()),
            ("filename".into(), FILENAME.into()),
            ("length".into(), "20".into()),
            ("subtype".into(), SUBTYPE.into()),
            ("_x_reason".into(), "upload-queue".into())
        ]
    );
    let complete = fields(complete_request(&c, &w, &ticket));
    assert_eq!(complete.len(), 3);
    let v: Value = serde_json::from_str(&complete[1].1).unwrap();
    assert_eq!(
        v,
        json!([{"id":"F00000002","title":FILENAME,"subtype":SUBTYPE}])
    );
    for size in [0, MAX_BYTES + 1] {
        assert!(ticket_request(&c, &w, size).is_err());
    }
}
#[test]
fn recording_upload_capabilities_reject_redirect_credentials_and_ambiguous_success() {
    for url in [
        "http://files.slack.com/upload",
        "https://evil.test/upload",
        "https://files.slack.com.evil.test/upload",
        "https://user@files.slack.com/upload",
        "https://files.slack.com:444/upload",
        "https://files.slack.com/upload#fragment",
    ] {
        assert!(
            UploadTicket::decode(&json!({"ok":true,"file":"F00000001","upload_url":url})).is_err()
        );
    }
    for value in [
        json!({}),
        json!({"ok":false}),
        json!({"ok":true,"error":"failed"}),
        json!({"ok":"true"}),
    ] {
        assert!(success(&value).is_err());
    }
}
