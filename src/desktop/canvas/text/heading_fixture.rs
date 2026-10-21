//! Closed H1–H3 CONTENT-only writes. Synthetic ACKs are not server evidence.
//! Contract: tmp/canvas-heading-edit-20261019/heading-contract.json.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use super_platinum_core::{
    canvas::{
        edit::text::PreparedTextEdit,
        wire::{EditAck, EncodedField as F, LoadDataResponse, RawMessage},
    },
    config::WorkspaceSession,
    slack::api::{CanvasCollabSnapshot, CanvasControllerInit},
};
mod content_id;
pub(super) use content_id::validate_proof as validate_attributes;

pub(in crate::canvas) fn html() -> &'static str {
    "<div class='quip-canvas-content'><h1>Heading editing verification</h1><h1 id='heading-one'>Shared <b>heading</b> 中🙂</h1><h2 id='heading-two'>Shared <b>heading</b> 中🙂</h2><h3 id='heading-three'>Third <i>heading</i> 日🦊</h3><h2 id='heading-twin'>Shared <b>heading</b> 中🙂</h2><p id='neighbor'>Untouched <i>neighbor</i> &amp; details.</p></div>"
}
#[derive(Default)]
pub(in crate::canvas) struct State {
    pub attempts: usize,
    canonical: Option<Arc<CanvasCollabSnapshot>>,
    pending: Option<(checklist::Attempt, PreparedTextEdit)>,
    writes: Vec<Value>,
    acknowledgements: Vec<String>,
    uncertain: bool,
    reads: usize,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-heading"
                | "canvas-heading-narrow"
                | "canvas-heading-content-id"
                | "canvas-heading-content-id-narrow")
        )
}
pub(super) fn has_content_id() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("canvas-heading-content-id" | "canvas-heading-content-id-narrow")
    )
}
pub(super) fn prefix() -> &'static str {
    if has_content_id() {
        "canvas-heading-content-id"
    } else {
        "canvas-heading"
    }
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.transport.is_none()
        && shell.core.session.is_none()
        && shell.canvas_checklist.heading_fixture.is_some()
}
fn msg(fields: Vec<F>) -> RawMessage {
    RawMessage::from_fields(&fields).unwrap()
}
fn s(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn u(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(super) fn initial() -> Arc<CanvasCollabSnapshot> {
    initial_with(false)
}
fn initial_with(content_ids: bool) -> Arc<CanvasCollabSnapshot> {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com/".into(),
        token: "synthetic".into(),
    };
    let options = msg(vec![s(1, "quip-user"), s(33, "U1"), s(34, "T1")]);
    let init = CanvasControllerInit::parse(
        json!({"ok":true,"quip_user_id":"quip-user","init_options":STANDARD.encode(options.raw())}),
        &workspace,
    )
    .unwrap();
    let file = serde_json::from_value(
        json!({"id":"Ffixture","filetype":"quip","quip_thread_id":"thread","editable":true}),
    )
    .unwrap();
    let thread = msg(vec![
        s(1, "thread"),
        u(2, 1000),
        s(10, "document"),
        u(22, 13),
        u(23, 13),
        u(119, 1),
    ]);
    let document = msg(vec![
        s(1, "document"),
        u(2, 1000),
        s(6, "thread"),
        u(12, 1),
        m(69, &msg(vec![s(1, "Fixture title")])),
    ]);
    let section = |id: &str, level: u64, rtml: &str, position: &str| {
        msg(vec![
            s(1, id),
            u(2, 1000),
            s(6, "thread"),
            s(7, "document"),
            s(8, position),
            u(9, 0),
            u(10, level),
            m(12, &msg(vec![m(1, &msg(vec![s(1, rtml), s(2, "ja")]))])),
            m(13, &msg(vec![])),
            m(
                16,
                &if content_ids && (1..=3).contains(&level) {
                    content_id::attributes(id)
                } else {
                    msg(vec![])
                },
            ),
            s(177, "heading-fixture-opaque"),
        ])
    };
    let sections = vec![
        msg(vec![
            s(1, "title"),
            u(2, 1000),
            s(6, "thread"),
            s(7, "document"),
            u(9, 64),
            u(10, 48),
            m(12, &msg(vec![m(58, &msg(vec![s(1, "Fixture title")]))])),
        ]),
        section("heading-one", 1, "Shared <b>heading</b> 中🙂", "A"),
        section("heading-two", 2, "Shared <b>heading</b> 中🙂", "B"),
        section("heading-three", 3, "Third <i>heading</i> 日🦊", "C"),
        section("heading-twin", 2, "Shared <b>heading</b> 中🙂", "D"),
        section(
            "neighbor",
            0,
            "Untouched <i>neighbor</i> &amp; details.",
            "E",
        ),
    ];
    let mut payload = vec![m(3, &thread), m(6, &document)];
    payload.extend(sections.iter().map(|v| m(7, v)));
    let mut index = vec![s(1, "Section/document")];
    index.extend(
        sections
            .iter()
            .map(|v| m(2, &msg(vec![s(1, v.string(1).unwrap().unwrap())]))),
    );
    let response = msg(vec![u(1, 1), m(2, &msg(payload)), m(3, &msg(index))]);
    let load = LoadDataResponse::decode(msg(vec![m(2, &response)]).raw()).unwrap();
    Arc::new(CanvasCollabSnapshot::from_responses(file, init, load).unwrap())
}
pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() || shell.core.transport.is_some() {
        return;
    }
    shell.core.session = None;
    checklist::open_visit(shell);
    if shell.canvas_checklist.heading_fixture.is_none() {
        shell.canvas_checklist.heading_fixture = Some(State {
            canonical: Some(if has_content_id() {
                initial_with(true)
            } else {
                initial()
            }),
            ..Default::default()
        });
    }
    refresh(shell);
}
pub(in crate::canvas) fn refresh(shell: &mut ShellState) {
    if !enabled(shell) {
        return;
    }
    let Some(read) = checklist::begin_read(shell) else {
        return;
    };
    let f = shell.canvas_checklist.heading_fixture.as_mut().unwrap();
    f.reads += 1;
    let canonical = f.canonical.clone().unwrap();
    checklist::finish_read(shell, &read, Ok(canonical));
    if let Some(attempt) = checklist::pending_attempt(shell) {
        checklist::confirm_readback(shell, &attempt, &read);
    }
}
pub(in crate::canvas) fn dispatch(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
) {
    if !enabled(shell) {
        return;
    }
    let fresh = shell
        .canvas_checklist
        .heading_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let prepared = checklist::with_edit_session(shell, attempt, |session| {
        intent.prepare(
            &fresh,
            session,
            checklist::text_ops::draft(attempt).unwrap(),
            checklist::save::now_usec().unwrap(),
        )
    });
    let Some(Ok(prepared)) = prepared else {
        checklist::cancel_preflight(shell, attempt);
        return;
    };
    if !checklist::text_ops::mark_dispatched(shell, attempt, &prepared) {
        return;
    }
    let sent = prepared.document_data().message(1).unwrap().unwrap();
    let f = shell.canvas_checklist.heading_fixture.as_mut().unwrap();
    f.attempts += 1;
    f.writes.push(json!({"section":prepared.section_id(),"documentData":STANDARD.encode(prepared.document_data().raw()),"sectionFields":sent.fields().iter().map(|v|v.tag()).collect::<Vec<_>>(),"desired":checklist::text_ops::draft(attempt).unwrap().rtml().unwrap(),"sequence":prepared.sequence()}));
    f.pending = Some((attempt.clone(), prepared));
}
pub(super) fn complete(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let f = shell.canvas_checklist.heading_fixture.as_mut().unwrap();
    let Some((attempt, prepared)) = f.pending.take() else {
        return false;
    };
    let previous = f.canonical.clone().unwrap();
    let sequence = prepared.base_sequence() + 1000;
    let sent = prepared.document_data().message(1).unwrap().unwrap();
    let wire = sent
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[u(3, sequence)])
        .unwrap()
        .replace_fields(
            4,
            &[u(4, checklist::save::now_usec().unwrap() + 60_000_000)],
        )
        .unwrap();
    // Synthetic partial ACK omits style/attrs/position; core owns the merge.
    let raw = msg(vec![
        s(1, prepared.section_id()),
        u(2, sequence),
        s(6, prepared.thread_id()),
        s(7, prepared.document_id()),
        m(12, &sent.message(12).unwrap().unwrap()),
        m(38, &wire),
    ]);
    let ack = EditAck::decode(
        msg(vec![
            u(1, 1),
            m(2, &msg(vec![s(15, "document"), m(1, &raw)])),
        ])
        .raw(),
    )
    .unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let mut fresh = (*previous).clone();
    *fresh
        .sections
        .iter_mut()
        .find(|s| s.id() == prepared.section_id())
        .unwrap() = previous
        .section(prepared.section_id())
        .unwrap()
        .merge_partial(accepted.section())
        .unwrap();
    f.canonical = Some(Arc::new(fresh));
    f.acknowledgements.push(STANDARD.encode(raw.raw()));
    let finished = checklist::text_ops::finish_text(shell, &attempt, accepted);
    refresh(shell);
    finished
}
pub(super) fn uncertain(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let f = shell.canvas_checklist.heading_fixture.as_mut().unwrap();
    let Some((attempt, _)) = f.pending.take() else {
        return false;
    };
    f.uncertain = true;
    checklist::finish(shell, &attempt, checklist::Outcome::Uncertain)
}
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell
        .canvas_checklist
        .heading_fixture
        .as_ref()
        .map_or(0, |f| f.attempts)
}
pub(super) fn proof(shell: &ShellState) -> Value {
    let f = shell.canvas_checklist.heading_fixture.as_ref().unwrap();
    json!({"attempts":f.attempts,"writes":f.writes,"acknowledgements":f.acknowledgements,"uncertain":f.uncertain,"reads":f.reads,"sections":inventory(f.canonical.as_ref().unwrap())})
}
pub(super) fn inventory(snapshot: &CanvasCollabSnapshot) -> Value {
    json!(snapshot.sections.iter().map(|s|json!({"id":s.id(),"raw":STANDARD.encode(s.raw().raw()),"style":s.style().unwrap(),"sequence":s.sequence().unwrap()})).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{EditSession, text::TextDraft};
    #[test]
    fn heading_fixture_has_exact_identity_and_content_only_projection() {
        let source = initial();
        assert_eq!(source.sections.len(), 6);
        let mut session = EditSession::new();
        for (id, level) in [("heading-one", 1), ("heading-two", 2), ("heading-three", 3)] {
            let intent = TextIntent::capture(&source, id, 100).unwrap();
            assert_eq!(intent.heading_level(), Some(level));
            let prepared = intent
                .prepare(
                    &source,
                    &mut session,
                    &TextDraft::from_rtml("<b>Changed 中🙂</b><br><br>日🦊").unwrap(),
                    100,
                )
                .unwrap();
            let row = prepared.document_data().message(1).unwrap().unwrap();
            assert_eq!(row.string(1).unwrap(), Some(id));
            for tag in [8, 10, 16, 29, 177] {
                assert!(!row.has(tag));
            }
            assert!(row.has(12));
            assert!(row.has(38));
        }
        let first = source.section("heading-two").unwrap();
        let twin = source.section("heading-twin").unwrap();
        assert_eq!(
            first.raw().message(12).unwrap(),
            twin.raw().message(12).unwrap()
        );
        assert_ne!(first.id(), twin.id());
        assert_ne!(
            first.raw().string(8).unwrap(),
            twin.raw().string(8).unwrap()
        );
    }
}
