use super::*;
use crate::canvas::edit::structure::ReplacementIntent;
fn setup() -> (CanvasCollabSnapshot, WorkspaceSession) {
    let mut snapshot = crate::canvas::edit::structure::tests::fixture();
    snapshot.sections.retain(|s| s.id() != "other");
    let workspace = WorkspaceSession {
        team_id: "Ttest".into(),
        user_id: "Utest".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    (snapshot, workspace)
}
#[test]
fn pair_form_binds_one_complete_payload_and_actual_account() {
    let (snapshot, workspace) = setup();
    let prepared = ReplacementIntent::main_canvas(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let result = request(
        &SlackClient::default(),
        &workspace,
        &snapshot,
        &prepared,
        100,
    )
    .unwrap();
    let crate::slack::client::RequestBody::Form(fields) = result.body else {
        panic!("form")
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(fields["session"], prepared.session_id());
    assert_eq!(fields["sequence"], "1");
    assert_eq!(fields["retry_count"], "0");
    assert_eq!(
        decode_canvas_base64(&fields["data_binary"]).unwrap(),
        prepared.document_data().raw()
    );
    assert_eq!(prepared.document_data().messages(1).unwrap().len(), 2);
    let mut other = workspace;
    other.user_id = "Uother".into();
    assert!(request(&SlackClient::default(), &other, &snapshot, &prepared, 100).is_err());
}
#[tokio::test]
async fn offline_pair_is_not_sent_and_never_replayed() {
    let (snapshot, workspace) = setup();
    let prepared = ReplacementIntent::main_canvas(&snapshot, "item", 100)
        .unwrap()
        .prepare(&snapshot, 100)
        .unwrap();
    let transport = Transport::new("fixture-only").unwrap();
    transport.set_health(crate::slack::transport::Health::Offline);
    assert!(matches!(
        execute_canvas_replacement_edit(
            &transport,
            &SlackClient::default(),
            &workspace,
            &snapshot,
            prepared
        )
        .await,
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
}
