use super::*;
use crate::slack::client::RequestBody;

fn message(fields: Vec<EncodedField>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn text(tag: u32, value: &str) -> EncodedField {
    EncodedField::bytes(tag, value.as_bytes()).unwrap()
}
fn uint(tag: u32, value: u64) -> EncodedField {
    EncodedField::uint(tag, value).unwrap()
}
fn nested(tag: u32, value: &RawMessage) -> EncodedField {
    EncodedField::message(tag, value).unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "Ttest".into(),
        enterprise_id: None,
        user_id: "Utest".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn init() -> CanvasControllerInit {
    let options = message(vec![
        text(1, "QuipTest"),
        text(33, "Utest"),
        text(34, "Ttest"),
        text(32, "fixture-session"),
    ]);
    CanvasControllerInit::parse(serde_json::json!({"ok":true,"quip_user_id":"QuipTest","init_options":encode_canvas_base64(options.raw()),"js_initializer":{"fixture":true}}), &workspace()).unwrap()
}
pub(crate) fn canvas_collab_test_snapshot(
    sections: Vec<Section>,
    document: RawMessage,
) -> CanvasCollabSnapshot {
    let doc = document.string(1).unwrap().unwrap();
    let thread_id = document.string(6).unwrap().unwrap();
    let thread = message(vec![
        text(1, thread_id),
        uint(2, 100),
        text(10, doc),
        uint(22, 13),
        uint(23, 13),
        uint(119, 1),
    ]);
    let mut objects = vec![nested(3, &thread), nested(6, &document)];
    let mut index = vec![text(1, &format!("Section/{doc}")), uint(6, 1)];
    for section in &sections {
        objects.push(nested(7, section.raw()));
        if section.deleted().unwrap() != Some(true) {
            index.push(nested(2, &message(vec![text(1, section.id())])));
        }
    }
    let response = message(vec![
        uint(1, 1),
        nested(2, &message(objects)),
        nested(3, &message(index)),
    ]);
    let wrapper = message(vec![nested(2, &response), nested(3, &message(vec![]))]);
    let file = serde_json::from_value(serde_json::json!({"id":"Ftest","filetype":"quip","editable":true,"quip_thread_id":thread_id})).unwrap();
    CanvasCollabSnapshot::from_responses(
        file,
        init(),
        LoadDataResponse::decode(wrapper.raw()).unwrap(),
    )
    .unwrap()
}
fn snapshot() -> CanvasCollabSnapshot {
    let section = Section::from_raw(message(vec![
        text(1, "temp:C:fixture"),
        uint(2, 100),
        text(6, "ThTest"),
        text(7, "Dtest"),
        nested(16, &message(vec![uint(2, 0)])),
        text(80, "retained-unknown"),
    ]))
    .unwrap();
    canvas_collab_test_snapshot(
        vec![section],
        message(vec![text(1, "Dtest"), uint(2, 100), text(6, "ThTest")]),
    )
}
#[test]
fn load_form_uses_thread_bulk_request_without_ui_tracing() {
    let request =
        canvas_collab_load_request(&SlackClient::default(), &workspace(), "ThTest").unwrap();
    let url = url::Url::parse(&request.url).unwrap();
    assert_eq!(url.path(), "/canvas/-/load-data");
    assert_eq!(url.query_pairs().count(), 1);
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    assert_eq!(fields.len(), 2);
    let encoded = &fields
        .iter()
        .find(|(key, _)| key == "request_binary")
        .unwrap()
        .1;
    let raw = RawMessage::decode(&decode_canvas_base64(encoded).unwrap()).unwrap();
    assert_eq!(raw.string(1).unwrap(), Some("ThTest"));
    let bulk = raw.message(3).unwrap().unwrap();
    assert_eq!(bulk.uint(1).unwrap(), Some(1));
    assert_eq!(bulk.string(2).unwrap(), Some("ThTest"));
    assert!(!raw.has(5) && !raw.has(6));
    let mut other = workspace();
    other.url = "https://example.com".into();
    assert!(canvas_collab_load_request(&SlackClient::default(), &other, "ThTest").is_err());
}
#[test]
fn initialization_is_account_bound_and_preserves_session_metadata() {
    let init = init();
    assert_eq!(init.options().string(32).unwrap(), Some("fixture-session"));
    assert_eq!(init.initializer()["fixture"], true);
    for (user, team, quip) in [
        ("other", "Ttest", "QuipTest"),
        ("Utest", "other", "QuipTest"),
        ("Utest", "Ttest", "other"),
    ] {
        let options = message(vec![text(1, quip), text(33, user), text(34, team)]);
        assert!(CanvasControllerInit::parse(serde_json::json!({"ok":true,"quip_user_id":"QuipTest","init_options":encode_canvas_base64(options.raw())}), &workspace()).is_err());
    }
}
#[test]
fn canonical_snapshot_preserves_colon_ids_permissions_unknown_fields_and_false() {
    let snapshot = snapshot();
    assert_eq!(
        (
            snapshot.file_id(),
            snapshot.thread_id(),
            snapshot.document_id()
        ),
        ("Ftest", "ThTest", "Dtest")
    );
    assert_eq!(snapshot.thread.uint(23).unwrap(), Some(13));
    let section = snapshot.section("temp:C:fixture").unwrap();
    assert_eq!(section.checked().unwrap(), Some(false));
    assert_eq!(section.raw().string(80).unwrap(), Some("retained-unknown"));
}
#[test]
fn canonical_snapshot_rejects_cross_document_missing_index_objects_and_followups() {
    let original = snapshot();
    let response = original.load.response().unwrap();
    let payload = response.message(2).unwrap().unwrap();
    let section = original.sections[0].raw();
    let wrong = section
        .replace_fields(7, &[text(7, "OtherDocument")])
        .unwrap();
    let wrong_payload = payload.replace_fields(7, &[nested(7, &wrong)]).unwrap();
    let missing_payload = payload.replace_fields(7, &[]).unwrap();
    let responses = [
        response
            .replace_fields(2, &[nested(2, &wrong_payload)])
            .unwrap(),
        response
            .replace_fields(2, &[nested(2, &missing_payload)])
            .unwrap(),
        response
            .replace_fields(4, &[nested(4, &message(vec![]))])
            .unwrap(),
    ];
    for response in responses {
        let wrapper = message(vec![nested(2, &response)]);
        assert!(
            CanvasCollabSnapshot::from_responses(
                original.file.clone(),
                original.init.clone(),
                LoadDataResponse::decode(wrapper.raw()).unwrap()
            )
            .is_err()
        );
    }
}

fn preserialized_snapshot_load(
    original: &CanvasCollabSnapshot,
    row: &RawMessage,
    history: Option<&RawMessage>,
) -> LoadDataResponse {
    let payload = original.load.payload().unwrap().unwrap().remove(7).unwrap();
    let response = original
        .load
        .response()
        .unwrap()
        .replace_fields(2, &[nested(2, &payload)])
        .unwrap();
    let record = message(vec![uint(1, 2), nested(2, row)]);
    let mut items = vec![nested(1, &record)];
    if let Some(history) = history {
        items.push(nested(2, history));
    }
    LoadDataResponse::decode(message(vec![nested(2, &response), nested(3, &message(items))]).raw())
        .unwrap()
}
#[test]
fn preserialized_snapshot_keeps_canonical_ownership_and_opaque_history() {
    let original = snapshot();
    let history = message(vec![
        text(1, original.sections[0].id()),
        uint(2, 0),
        uint(3, 100),
        text(5, "synthetic opaque history"),
        text(90, "unknown"),
    ]);
    let load = preserialized_snapshot_load(&original, original.sections[0].raw(), Some(&history));
    let wire = load.raw().raw().to_vec();
    let result =
        CanvasCollabSnapshot::from_responses(original.file.clone(), original.init.clone(), load)
            .unwrap();
    assert_eq!(result.file_id(), original.file_id());
    assert_eq!(result.thread, original.thread);
    assert_eq!(result.document, original.document);
    assert_eq!(result.sections[0].crdt().unwrap().unwrap(), history);
    assert_eq!(
        result.sections[0].raw().string(80).unwrap(),
        Some("retained-unknown")
    );
    assert_eq!(result.load.raw().raw(), wire);
    // Loading is not history acceptance: no sidecar does not invent a session.
    let load = preserialized_snapshot_load(&original, original.sections[0].raw(), None);
    let result = CanvasCollabSnapshot::from_responses(original.file, original.init, load).unwrap();
    assert!(result.sections[0].crdt().unwrap().is_none());
}
#[test]
fn preserialized_snapshot_still_refuses_cross_owner_or_incomplete_inventory() {
    let original = snapshot();
    for tag in [6, 7] {
        let wrong = original.sections[0]
            .raw()
            .replace_fields(tag, &[text(tag, "Other")])
            .unwrap();
        let load = preserialized_snapshot_load(&original, &wrong, None);
        assert!(
            CanvasCollabSnapshot::from_responses(
                original.file.clone(),
                original.init.clone(),
                load
            )
            .is_err()
        );
    }
    let load = preserialized_snapshot_load(&original, original.sections[0].raw(), None);
    let response = load.response().unwrap();
    let empty_index = message(vec![text(1, "Section/Dtest"), uint(6, 1)]);
    for response in [
        response.remove(3).unwrap(),
        response
            .replace_fields(3, &[nested(3, &empty_index)])
            .unwrap(),
        response
            .replace_fields(4, &[nested(4, &message(vec![]))])
            .unwrap(),
    ] {
        let raw = load
            .raw()
            .replace_fields(2, &[nested(2, &response)])
            .unwrap();
        assert!(
            CanvasCollabSnapshot::from_responses(
                original.file.clone(),
                original.init.clone(),
                LoadDataResponse::decode(raw.raw()).unwrap()
            )
            .is_err()
        );
    }
    let mut wrong_file = original.file.clone();
    wrong_file
        .extra
        .insert("quip_thread_id".into(), serde_json::json!("OtherThread"));
    assert!(CanvasCollabSnapshot::from_responses(wrong_file, original.init, load).is_err());
}
