//! Synthetic canonical text and manually completed requests. No transport exists.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use super_platinum_core::{
    canvas::{
        edit::text::PreparedTextEdit,
        wire::{EditAck, EncodedField as F, LoadDataResponse, RawMessage},
    },
    config::WorkspaceSession,
    slack::api::{CanvasCollabSnapshot, CanvasControllerInit},
};
pub(in crate::canvas) const HTML: &str = "<div class='quip-canvas-content'><h1>Paragraph editing verification</h1><p id='paragraph'><span id='paragraph'>A <b>bold</b> line 🦊 日本語 &amp; details.</span></p><p id='unsupported'><u>This unsupported formatting stays readable.</u></p><div data-section-style='7'><ul><li id='item'><span id='item'>An untouched checklist item</span></li></ul></div></div>";
const INITIAL: &str = "A <b>bold</b> line 🦊 日本語 &amp; details.";
#[derive(Default)]
pub(in crate::canvas) struct State {
    pub attempts: usize,
    pub(in crate::canvas) loader: Option<crate::canvas::loader::fixture::State>,
    canonical: Option<Arc<CanvasCollabSnapshot>>,
    pending: Option<(checklist::Attempt, PreparedTextEdit)>,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-text" | "canvas-text-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.transport.is_none()
        && shell.core.session.is_none()
        && shell.canvas_checklist.text_fixture.is_some()
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
pub(in crate::canvas) fn snapshot(text: &str, sequence: u64) -> Arc<CanvasCollabSnapshot> {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com/".into(),
        token: "synthetic".into(),
    };
    let options = msg(vec![s(1, "quip-user"), s(33, "U1"), s(34, "T1")]);
    let init=CanvasControllerInit::parse(serde_json::json!({"ok":true,"quip_user_id":"quip-user","init_options":STANDARD.encode(options.raw())}),&workspace).unwrap();
    let file=serde_json::from_value(serde_json::json!({"id":"Ffixture","filetype":"quip","quip_thread_id":"thread","editable":true})).unwrap();
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
    let section = |id: &str, content: &str| {
        msg(vec![
            s(1, id),
            u(2, sequence),
            s(6, "thread"),
            s(7, "document"),
            m(12, &msg(vec![m(1, &msg(vec![s(1, content), s(2, "en")]))])),
        ])
    };
    let title = msg(vec![
        s(1, "title"),
        u(2, 1000),
        s(6, "thread"),
        s(7, "document"),
        u(9, 64),
        u(10, 48),
        m(12, &msg(vec![m(58, &msg(vec![s(1, "Fixture title")]))])),
    ]);
    let parent = msg(vec![
        s(1, "parent"),
        u(2, 1000),
        s(6, "thread"),
        s(7, "document"),
        u(10, 7),
    ]);
    let item = section("item", "An untouched checklist item")
        .replace_fields(13, &[m(13, &msg(vec![s(1, "parent"), u(4, 7)]))])
        .unwrap()
        .replace_fields(16, &[m(16, &msg(vec![u(2, 0)]))])
        .unwrap();
    let sections = vec![
        title,
        section("paragraph", text),
        section(
            "unsupported",
            "<u>This unsupported formatting stays readable.</u>",
        ),
        parent,
        item,
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
    if shell.canvas_checklist.text_fixture.is_none() {
        shell.canvas_checklist.text_fixture = Some(State {
            canonical: Some(snapshot(INITIAL, 1000)),
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
    let canonical = shell
        .canvas_checklist
        .text_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
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
        .text_fixture
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
    let fixture = shell.canvas_checklist.text_fixture.as_mut().unwrap();
    fixture.attempts += 1;
    fixture.pending = Some((attempt.clone(), prepared));
}
pub(super) fn complete(shell: &mut ShellState) -> bool {
    let Some((attempt, prepared)) = shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return false;
    };
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
    let previous = shell
        .canvas_checklist
        .text_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let raw = previous
        .section("paragraph")
        .unwrap()
        .raw()
        .replace_fields(2, &[u(2, sequence)])
        .unwrap()
        .replace_fields(12, &[m(12, &sent.message(12).unwrap().unwrap())])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
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
        .find(|s| s.id() == "paragraph")
        .unwrap() = accepted.section().clone();
    shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .unwrap()
        .canonical = Some(Arc::new(fresh));
    checklist::text_ops::finish_text(shell, &attempt, accepted);
    refresh(shell);
    true
}
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell
        .canvas_checklist
        .text_fixture
        .as_ref()
        .map_or(0, |f| f.attempts)
}
pub(super) fn uncertain(shell: &mut ShellState) -> bool {
    let Some((attempt, _)) = shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return false;
    };
    checklist::finish(shell, &attempt, checklist::Outcome::Uncertain)
}
