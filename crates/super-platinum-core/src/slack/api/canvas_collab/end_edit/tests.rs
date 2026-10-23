use super::*;
use crate::canvas::edit::{
    structure::{EndIntent, EndWitness},
    text::TextDraft,
};
fn setup() -> (CanvasCollabSnapshot, WorkspaceSession) {
    let mut s = crate::canvas::edit::structure::tests::fixture();
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    let field = crate::canvas::wire::EncodedField::bytes(1, b"Keep thisSecond paragraph").unwrap();
    let text = RawMessage::from_fields(&[field]).unwrap();
    let content =
        RawMessage::from_fields(&[crate::canvas::wire::EncodedField::message(1, &text).unwrap()])
            .unwrap();
    s.sections[i] = Section::from_raw(
        s.sections[i]
            .raw()
            .replace_fields(
                12,
                &[crate::canvas::wire::EncodedField::message(12, &content).unwrap()],
            )
            .unwrap(),
    )
    .unwrap();
    (
        s,
        WorkspaceSession {
            team_id: "Ttest".into(),
            user_id: "Utest".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        },
    )
}
fn prepare(s: &CanvasCollabSnapshot) -> PreparedEndEdit {
    let intent = EndIntent::capture(s, "item", 100).unwrap();
    let w = EndWitness::new(intent.seed(), 25, intent.seed(), &TextDraft::default()).unwrap();
    intent.prepare(s, &w, 100).unwrap()
}
#[test]
fn end_form_binds_one_complete_pair_and_current_account() {
    let (s, w) = setup();
    let p = prepare(&s);
    let result = request(&SlackClient::default(), &w, &s, &p, 100).unwrap();
    assert_eq!(
        url::Url::parse(&result.url).unwrap().path(),
        "/canvas/-/edit-document"
    );
    let crate::slack::client::RequestBody::Form(fields) = result.body else {
        panic!("form")
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fields["retry_count"], "0");
    assert_eq!(fields["sequence"], "1");
    assert_eq!(fields["session"], p.session_id());
    assert_eq!(fields["title"], p.form_title());
    assert_eq!(
        decode_canvas_base64(&fields["data_binary"]).unwrap(),
        p.document_data().raw()
    );
    assert_eq!(p.document_data().messages(1).unwrap().len(), 2);
    let mut other = w.clone();
    other.user_id = "Uother".into();
    assert!(request(&SlackClient::default(), &other, &s, &p, 100).is_err());
    let mut fresh = s.clone();
    fresh.sections.pop();
    assert!(request(&SlackClient::default(), &w, &fresh, &p, 100).is_err());
}
#[tokio::test]
async fn offline_end_is_not_sent_and_never_replayed() {
    let (s, w) = setup();
    let p = prepare(&s);
    let transport = Transport::new("fixture-only").unwrap();
    transport.set_health(crate::slack::transport::Health::Offline);
    assert!(matches!(
        execute_canvas_end_edit(&transport, &SlackClient::default(), &w, &s, p).await,
        Err(CanvasEditFailure::NotSent(Error::Offline(_)))
    ));
}
