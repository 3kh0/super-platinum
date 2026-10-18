use super::*;
use crate::canvas::wire::EditAck;
mod journal;
fn n(tag: u32, v: u64) -> F {
    F::uint(tag, v).unwrap()
}
fn t(tag: u32, v: &str) -> F {
    F::bytes(tag, v.as_bytes()).unwrap()
}
fn m(tag: u32, v: &RawMessage) -> F {
    F::message(tag, v).unwrap()
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn changed(s: &Section, field: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
pub(super) fn fixture() -> CanvasCollabSnapshot {
    let mut s = super::super::structure::tests::fixture();
    s.thread = s.thread.replace_fields(23, &[n(23, 5)]).unwrap();
    s.file
        .extra
        .insert("access".into(), serde_json::json!("comment"));
    s
}
fn prepare(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
    AnnotationIntent::capture(s, "item")
        .unwrap()
        .prepare(s)
        .unwrap()
}
fn receipt(p: &PreparedAnnotation) -> Section {
    let mut s = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    for field in [
        n(2, 10001),
        t(6, p.thread_id()),
        t(7, p.document_id()),
        t(21, position::base(&p.intent.path).unwrap()),
    ] {
        s = changed(&s, field);
    }
    s
}
fn ack(s: &Section) -> EditAck {
    EditAck::decode(raw(&[n(1, 1), m(2, &raw(&[m(1, s.raw())]))]).raw()).unwrap()
}

#[test]
fn source_control_payload_is_separate_from_unchanged_paragraph_bytes() {
    let s = fixture();
    let before = s.sections.clone();
    let p = prepare(&s);
    assert_eq!(s.sections, before);
    let v = p.document_data().message(1).unwrap().unwrap();
    assert_eq!(
        v.fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
        vec![1, 2, 3, 9, 11, 12, 14, 19, 20, 21, 33, 35]
    );
    assert_eq!(v.uint(9).unwrap(), Some(9));
    assert_eq!(v.uint(11).unwrap(), Some(1));
    let h = v.message(12).unwrap().unwrap().message(7).unwrap().unwrap();
    assert_eq!(h.uint(9).unwrap(), Some(2));
    assert_eq!(strings(&h, 3).unwrap(), vec!["item"]);
    assert!(!v.has(38));
    assert!(!v.has(13));
}
#[test]
fn annotation_permission_is_distinct_from_body_edit_and_stale_owner_refuses() {
    let s = fixture();
    assert!(AnnotationIntent::capture(&s, "item").is_ok());
    for permission in [1, 13] {
        let mut other = s.clone();
        other.thread = other
            .thread
            .replace_fields(23, &[n(23, permission)])
            .unwrap();
        assert!(AnnotationIntent::capture(&other, "item").is_err());
    }
    let mut other = s.clone();
    other.thread = other.thread.replace_fields(105, &[n(105, 1)]).unwrap();
    assert!(AnnotationIntent::capture(&other, "item").is_err());
    let intent = AnnotationIntent::capture(&s, "item").unwrap();
    let mut other = s.clone();
    other.sections[0] = changed(&other.sections[0], n(2, 10002));
    assert!(intent.prepare(&other).is_err());
}
#[test]
fn ack_needs_exact_anchor_then_fresh_canonical_control_before_bootstrap() {
    let s = fixture();
    let p = prepare(&s);
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    assert!(accepted.confirm(&s).is_err());
    let p = prepare(&s);
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    let mut fresh = s.clone();
    fresh.sections.push(control);
    let verified = accepted.confirm(&fresh).unwrap();
    assert_eq!(verified.section().section_type().unwrap(), Some(9));
    let p = prepare(&s);
    let mut control = receipt(&p);
    let content = control.raw().message(12).unwrap().unwrap();
    let highlight = content
        .message(7)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[t(3, "other")])
        .unwrap();
    control = changed(&control, m(12, &raw(&[m(7, &highlight)])));
    assert!(p.validate_ack(&ack(&control)).is_err());
}
#[test]
fn existing_archived_control_is_not_silently_deleted_or_duplicated() {
    let mut s = fixture();
    let p = prepare(&s);
    let control = receipt(&p);
    s.sections.push(control.clone());
    assert!(AnnotationIntent::capture(&s, "item").is_err());
    let content = control.raw().message(12).unwrap().unwrap();
    let h = content
        .message(7)
        .unwrap()
        .unwrap()
        .replace_fields(4, &[n(4, 1)])
        .unwrap();
    *s.sections.last_mut().unwrap() = changed(&control, m(12, &raw(&[m(7, &h)])));
    assert!(AnnotationIntent::capture(&s, "item").is_err());
    let repeated = raw(&[t(3, "item"), t(3, "other")]);
    assert_eq!(strings(&repeated, 3).unwrap(), vec!["item", "other"]);
}

#[tokio::test]
async fn sealed_bootstrap_builder_passes_actual_one_attempt_guard_without_rewriting_its_wire_fields()
 {
    use crate::{
        canvas::comments::bootstrap::BootstrapIntent,
        config::WorkspaceSession,
        slack::{api, client::SlackClient, transport::Transport},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let s = fixture();
    let p = prepare(&s);
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    let mut fresh = s.clone();
    fresh.sections.push(control);
    let intent =
        BootstrapIntent::capture(accepted.confirm(&fresh).unwrap(), 1_720_000_000_001).unwrap();
    let session = WorkspaceSession {
        team_id: "Ttest".into(),
        enterprise_id: None,
        user_id: "Utest".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    assert_eq!(
        intent.record().annotation().scope().workspace,
        session.team_id
    );
    assert_eq!(intent.record().annotation().scope().user, session.user_id);
    let mut request =
        api::canvas_comment_bootstrap(&SlackClient::default(), &session, &intent).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut url = url::Url::parse(&request.url).unwrap();
    url.set_scheme("http").unwrap();
    url.set_host(Some("127.0.0.1")).unwrap();
    url.set_port(Some(address.port())).unwrap();
    request.url = url.into();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 16384];
        let size = stream.read(&mut bytes).await.unwrap();
        assert!(
            String::from_utf8_lossy(&bytes[..size]).starts_with("POST /api/conversations.replies?")
        );
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .await
            .unwrap();
    });
    let result = Transport::new("synthetic-cookie")
        .unwrap()
        .execute_canvas_comment_bootstrap(request)
        .await;
    assert_eq!(result.unwrap(), b"{}");
    server.await.unwrap();
}
