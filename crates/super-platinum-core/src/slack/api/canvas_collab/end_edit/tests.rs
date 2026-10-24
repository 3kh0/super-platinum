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

fn long_prepare(s: &CanvasCollabSnapshot) -> PreparedEndEdit {
    let intent = EndIntent::capture_sequence(s, "item", 16, 100).unwrap();
    let reports = (0..16)
        .map(|i| vec![TextDraft::from_rtml(&format!("Paragraph {i} 日本😀")).unwrap()])
        .collect::<Vec<_>>();
    let witness = EndWitness::sequence(intent.seed(), 25, &reports).unwrap();
    intent.prepare(s, &witness, 100).unwrap()
}

// Model the same server-authored sequence/position fields as the core end tests,
// retaining the exact CRDT request histories instead of inventing ACK contents.
fn authored_rows(prepared: &PreparedEndEdit, source: &CanvasCollabSnapshot) -> Vec<Section> {
    use crate::canvas::wire::EncodedField as F;
    prepared
        .document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|raw| {
            let update = Section::from_raw(raw).unwrap();
            let previous = source.section(update.id());
            let sequence = previous
                .and_then(|row| row.sequence().unwrap())
                .unwrap_or(0)
                + 1000;
            let row =
                previous.map_or_else(|| update.clone(), |row| row.merge_partial(&update).unwrap());
            let mut raw = row.raw().clone();
            for field in [
                F::uint(2, sequence).unwrap(),
                F::bytes(6, source.thread_id().as_bytes()).unwrap(),
                F::bytes(7, source.document_id().as_bytes()).unwrap(),
            ] {
                raw = raw
                    .merge_partial(&RawMessage::decode(field.raw()).unwrap())
                    .unwrap();
            }
            if previous.is_none() {
                let position = raw.string(8).unwrap().unwrap();
                let position = position
                    .strip_suffix(":temp")
                    .unwrap_or(position)
                    .to_owned();
                for tag in [8, 21] {
                    raw = raw
                        .replace_fields(tag, &[F::bytes(tag, position.as_bytes()).unwrap()])
                        .unwrap();
                }
            }
            let wire = raw
                .message(38)
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[F::uint(3, sequence).unwrap()])
                .unwrap()
                .replace_fields(4, &[F::uint(4, 1_000_000).unwrap()])
                .unwrap();
            Section::from_raw(
                raw.replace_fields(38, &[F::message(38, &wire).unwrap()])
                    .unwrap(),
            )
            .unwrap()
        })
        .collect()
}

fn response(rows: &[Section]) -> Vec<u8> {
    use crate::canvas::wire::EncodedField as F;
    let rows = RawMessage::from_fields(
        &rows
            .iter()
            .map(|row| F::message(1, row.raw()).unwrap())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    RawMessage::from_fields(&[F::uint(1, 1).unwrap(), F::message(2, &rows).unwrap()])
        .unwrap()
        .raw()
        .to_vec()
}

#[tokio::test]
async fn background_end_ack_accepts_all_sixteen_members_and_preserves_readback_proof() {
    let (mut source, _) = setup();
    let prepared = long_prepare(&source);
    let rows = authored_rows(&prepared, &source);
    assert_eq!(rows.len(), 17);
    let ids = prepared
        .appended_members()
        .map(|member| member.id().to_owned())
        .collect::<Vec<_>>();
    let accepted = validate_response(prepared, response(&rows)).await.unwrap();
    assert!(
        accepted
            .appended_sections()
            .map(|row| row.id())
            .eq(ids.iter().map(String::as_str))
    );
    for row in rows {
        if let Some(index) = source.sections.iter().position(|old| old.id() == row.id()) {
            source.sections[index] = row;
        } else {
            source.sections.push(row);
        }
    }
    assert!(accepted.confirms_readback(&source).unwrap());
    source.sections.pop();
    assert!(!accepted.confirms_readback(&source).unwrap_or(false));
}

#[tokio::test]
async fn background_end_ack_keeps_malformed_and_missing_member_responses_uncertain() {
    let (source, _) = setup();
    assert!(matches!(
        validate_response(long_prepare(&source), vec![0x80]).await,
        Err(CanvasEditFailure::Uncertain(Error::Api(reason)))
            if reason == "invalid_canvas_edit_acknowledgement"
    ));
    let prepared = long_prepare(&source);
    let mut rows = authored_rows(&prepared, &source);
    rows.pop();
    assert!(matches!(
        validate_response(prepared, response(&rows)).await,
        Err(CanvasEditFailure::Uncertain(Error::Api(reason)))
            if reason == "unconfirmed_canvas_edit_acknowledgement"
    ));
}
