use super::*;
use crate::canvas::edit::{CheckboxIntent, EditSession};
use crate::slack::client::RequestBody;
fn msg(fields: Vec<EncodedField>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn text(tag: u32, value: &str) -> EncodedField {
    EncodedField::bytes(tag, value.as_bytes()).unwrap()
}
fn num(tag: u32, value: u64) -> EncodedField {
    EncodedField::uint(tag, value).unwrap()
}
fn nested(tag: u32, value: &RawMessage) -> EncodedField {
    EncodedField::message(tag, value).unwrap()
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "Ttest".into(),
        user_id: "Utest".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn snapshot() -> CanvasCollabSnapshot {
    let section = |id: &str, kind: u64, style: u64, extra: Vec<EncodedField>| {
        let mut fields = vec![
            text(1, id),
            num(2, 10_000),
            text(6, "ThTest"),
            text(7, "Dtest"),
            num(9, kind),
            num(10, style),
        ];
        fields.extend(extra);
        Section::from_raw(msg(fields)).unwrap()
    };
    let item = section(
        "item",
        0,
        0,
        vec![
            nested(
                12,
                &msg(vec![nested(1, &msg(vec![text(1, "Synthetic checkbox")]))]),
            ),
            nested(13, &msg(vec![text(1, "parent"), num(4, 7)])),
            nested(16, &msg(vec![num(2, 0)])),
        ],
    );
    let parent = section("parent", 0, 7, vec![]);
    let title = section(
        "title",
        64,
        48,
        vec![nested(
            12,
            &msg(vec![nested(58, &msg(vec![text(1, "Synthetic canvas")]))]),
        )],
    );
    crate::slack::api::canvas_collab_test_snapshot(
        vec![item, parent, title],
        msg(vec![text(1, "Dtest"), num(2, 10_000), text(6, "ThTest")]),
    )
}
fn prepared(snapshot: &CanvasCollabSnapshot) -> PreparedCheckboxEdit {
    CheckboxIntent::capture(snapshot, "item", true, 100)
        .unwrap()
        .prepare(snapshot, &mut EditSession::new(), 100)
        .unwrap()
}
fn fields(request: &PreparedRequest) -> std::collections::BTreeMap<&str, &str> {
    let RequestBody::Form(fields) = &request.body else {
        panic!("form required")
    };
    fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}
#[test]
fn exact_canvas_save_form_binds_account_title_sequence_and_binary() {
    let mut snapshot = snapshot();
    snapshot.thread = snapshot
        .thread
        .replace_fields(106, &[text(106, "synthetic-secret-path")])
        .unwrap();
    let prepared = prepared(&snapshot);
    let request = request(
        &SlackClient::default(),
        &workspace(),
        &snapshot,
        &prepared,
        100,
    )
    .unwrap();
    assert_eq!(request.method, "POST");
    let url = url::Url::parse(&request.url).unwrap();
    assert_eq!(url.host_str(), Some("fixture.slack.com"));
    assert_eq!(url.path(), "/canvas/-/edit-document");
    assert_eq!(url.query_pairs().count(), 1);
    let fields = fields(&request);
    assert_eq!(
        fields.keys().copied().collect::<Vec<_>>(),
        vec![
            "data_binary",
            "document",
            "nav_action_id",
            "retry_count",
            "search_session_id",
            "secret_path",
            "sequence",
            "session",
            "thread",
            "title",
            "token"
        ]
    );
    assert_eq!(fields["document"], "Dtest");
    assert_eq!(fields["thread"], "ThTest");
    assert_eq!(fields["retry_count"], "0");
    assert_eq!(fields["secret_path"], "synthetic-secret-path");
    assert_eq!(fields["sequence"], "1");
    assert_eq!(fields["session"], prepared.session_id());
    assert_eq!(fields["title"], "Synthetic canvas");
    assert_eq!(fields["nav_action_id"], "null");
    assert_eq!(fields["search_session_id"], "null");
    assert_eq!(
        decode_canvas_base64(fields["data_binary"]).unwrap(),
        prepared.document_data().raw()
    );
}
#[test]
fn stale_canonical_inputs_or_different_account_cannot_construct_a_request() {
    let mut snapshot = snapshot();
    let prepared = prepared(&snapshot);
    let client = SlackClient::default();
    let mut owner = workspace();
    owner.user_id = "OtherUser".into();
    assert!(request(&client, &owner, &snapshot, &prepared, 100).is_err());
    owner = workspace();
    owner.team_id = "OtherTeam".into();
    assert!(request(&client, &owner, &snapshot, &prepared, 100).is_err());
    owner = workspace();
    owner.url = "https://example.com".into();
    assert!(request(&client, &owner, &snapshot, &prepared, 100).is_err());
    snapshot.sections[0] = Section::from_raw(
        snapshot.sections[0]
            .raw()
            .replace_fields(2, &[num(2, 11_000)])
            .unwrap(),
    )
    .unwrap();
    assert!(request(&client, &workspace(), &snapshot, &prepared, 100).is_err());
}
fn ack(snapshot: &CanvasCollabSnapshot, checked: bool, sequence: u64) -> Vec<u8> {
    let section = snapshot.section("item").unwrap();
    let attrs = section
        .attributes()
        .unwrap()
        .unwrap()
        .replace_fields(2, &[num(2, u64::from(checked))])
        .unwrap();
    let raw = section
        .raw()
        .replace_fields(16, &[nested(16, &attrs)])
        .unwrap()
        .replace_fields(2, &[num(2, sequence)])
        .unwrap();
    msg(vec![num(1, 1), nested(2, &msg(vec![nested(1, &raw)]))])
        .raw()
        .to_vec()
}
#[test]
fn only_direct_matching_binary_ack_returns_sealed_acceptance() {
    let snapshot = snapshot();
    let prepared = prepared(&snapshot);
    let good = ack(&snapshot, true, 12_000);
    let receipt = accepted(&prepared, &good).unwrap();
    assert_eq!(receipt.session_id(), prepared.session_id());
    assert_eq!(receipt.sequence(), prepared.sequence());
    assert_eq!(receipt.section().sequence().unwrap(), Some(12_000));
    for bytes in [
        vec![0xff],
        ack(&snapshot, false, 12_000),
        ack(&snapshot, true, 10_000),
    ] {
        assert!(matches!(
            accepted(&prepared, &bytes),
            Err(CanvasEditFailure::Uncertain(_))
        ));
    }
    let raw = RawMessage::decode(&good)
        .unwrap()
        .replace_fields(6, &[num(6, 1)])
        .unwrap();
    assert!(matches!(
        accepted(&prepared, raw.raw()),
        Err(CanvasEditFailure::Uncertain(_))
    ));
}
#[tokio::test]
async fn known_offline_is_not_sent_and_prepared_value_is_consumed() {
    let snapshot = snapshot();
    let prepared = prepared(&snapshot);
    let transport = Transport::new("synthetic-cookie").unwrap();
    transport.set_health(crate::slack::transport::Health::Offline);
    let result = execute_canvas_checkbox_edit(
        &transport,
        &SlackClient::default(),
        &workspace(),
        &snapshot,
        prepared,
    )
    .await;
    assert!(matches!(
        result,
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
}
