//! Exact offline paragraph/title preflight proof. No HTTP request is executed.
use super::*;
use serde_json::{Value, json};

pub(in crate::canvas) const HTML: &str = "<div class='quip-canvas-content'><h1>Plan &amp; Review</h1><p id='paragraph'><span id='paragraph'>A <b>bold</b> line 🦊 日本語 &amp; details.</span></p><p id='unsupported'><u data-unsupported='true'>This unsupported formatting stays readable.</u></p><div data-section-style='7'><ul><li id='item'><span id='item'>An untouched checklist item</span></li></ul></div></div>";
pub(in crate::canvas::text) const TITLE: &str = "<b>Plan &amp; Review</b>";
pub(in crate::canvas::text) const DESIRED: &str =
    "A <b>bold</b> line 🦊 日本語 &amp; details. <i>Added</i>";
pub(in crate::canvas::text) const STALE_TITLE: &str = "<i>Plan &#38; Review</i>";

#[derive(Default)]
pub(super) struct State {
    preflights: Vec<Value>,
    writes: Vec<Value>,
    acknowledgements: Vec<Value>,
    readbacks: Vec<Value>,
}

pub(in crate::canvas::text) fn requested() -> bool {
    super::requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_FORMATTED_TITLE_EDIT_CHECK").as_deref() == Ok("1")
}
pub(in crate::canvas::text) fn enabled(shell: &ShellState) -> bool {
    requested() && super::enabled(shell)
}

/// Decode a complete authored response, preserving every untouched raw field.
fn rebuild(previous: &CanvasCollabSnapshot, rows: Vec<RawMessage>) -> Arc<CanvasCollabSnapshot> {
    let mut payload = vec![m(3, &previous.thread), m(6, &previous.document)];
    payload.extend(rows.iter().map(|row| m(7, row)));
    let mut index = vec![s(1, "Section/document")];
    index.extend(
        rows.iter()
            .map(|row| m(2, &msg(vec![s(1, row.string(1).unwrap().unwrap())]))),
    );
    let response = msg(vec![u(1, 1), m(2, &msg(payload)), m(3, &msg(index))]);
    let load = LoadDataResponse::decode(msg(vec![m(2, &response)]).raw()).unwrap();
    Arc::new(
        CanvasCollabSnapshot::from_responses(previous.file.clone(), previous.init.clone(), load)
            .unwrap(),
    )
}

fn title_source(
    previous: &CanvasCollabSnapshot,
    text: &str,
    revision: u64,
) -> Arc<CanvasCollabSnapshot> {
    rebuild(
        previous,
        previous
            .sections
            .iter()
            .map(|row| {
                if row.id() != "title" {
                    return row.raw().clone();
                }
                row.raw()
                    .replace_fields(2, &[u(2, revision)])
                    .unwrap()
                    .replace_fields(12, &[m(12, &msg(vec![m(58, &msg(vec![s(1, text)]))]))])
                    .unwrap()
            })
            .collect(),
    )
}

pub(super) fn initial() -> Arc<CanvasCollabSnapshot> {
    title_source(&super::snapshot(INITIAL, 1000), TITLE, 1000)
}

pub(in crate::canvas::text) fn inventory(value: &CanvasCollabSnapshot) -> Value {
    json!({"file":value.file_id(),"thread":value.thread_id(),"document":value.document_id(),
        "threadRaw":STANDARD.encode(value.thread.raw()),"documentRaw":STANDARD.encode(value.document.raw()),
        "sections":value.sections.iter().map(|row|json!({"id":row.id(),"raw":STANDARD.encode(row.raw().raw()),"sequence":row.sequence().unwrap()})).collect::<Vec<_>>()})
}

pub(in crate::canvas::text) fn dispatch(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
) -> bool {
    if !enabled(shell) {
        return false;
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
    let outcome = match &prepared {
        Some(Ok(_)) => "prepared".to_owned(),
        Some(Err(error)) => format!("{error:?}"),
        None => "owner-unavailable".to_owned(),
    };
    shell.canvas_checklist.text_fixture.as_mut().unwrap().formatted.as_mut().unwrap().preflights.push(
        json!({"visit":attempt.visit(),"source":inventory(&attempt.snapshot),"fresh":inventory(&fresh),"outcome":outcome}));
    let Some(Ok(prepared)) = prepared else {
        return false;
    };
    if !checklist::text_ops::mark_dispatched(shell, attempt, &prepared) {
        return false;
    }
    let f = shell.canvas_checklist.text_fixture.as_mut().unwrap();
    f.attempts += 1;
    f.formatted.as_mut().unwrap().writes.push(json!({
        "section":prepared.section_id(),"session":prepared.session_id(),"sequence":prepared.sequence(),
        "baseSequence":prepared.base_sequence(),"formTitle":prepared.form_title(),
        "documentData":STANDARD.encode(prepared.document_data().raw()),
        "desired":prepared.desired().rtml().unwrap(),"httpFactoryExecuted":false}));
    f.pending = Some((attempt.clone(), prepared));
    true
}

pub(in crate::canvas::text) fn complete(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some((attempt, prepared)) = shell
        .canvas_checklist
        .text_fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
    else {
        return false;
    };
    let previous = shell
        .canvas_checklist
        .text_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    // Only the actual CRDT head/session/expiry is correlated to the held write.
    // ContentText is independently authored; it is never copied from sent field12.
    let sequence = 2000;
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
    let authored_content = msg(vec![m(1, &msg(vec![s(1, DESIRED), s(2, "en")]))]);
    let raw = previous
        .section("paragraph")
        .unwrap()
        .raw()
        .replace_fields(2, &[u(2, sequence)])
        .unwrap()
        .replace_fields(12, &[m(12, &authored_content)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let ack_bytes = msg(vec![
        u(1, 1),
        m(2, &msg(vec![s(15, "document"), m(1, &raw)])),
    ]);
    let ack = EditAck::decode(ack_bytes.raw()).unwrap();
    let Ok(accepted) = prepared.validate_ack(&ack) else {
        return false;
    };
    // A separately decoded complete readback has the authored body, not a copy
    // of AcceptedText. Other sections and Document.69 remain byte-exact.
    let fresh = rebuild(
        &previous,
        previous
            .sections
            .iter()
            .map(|row| {
                if row.id() == "paragraph" {
                    raw.clone()
                } else {
                    row.raw().clone()
                }
            })
            .collect(),
    );
    if !accepted.confirms_readback(&fresh).unwrap_or(false) {
        return false;
    }
    let f = shell.canvas_checklist.text_fixture.as_mut().unwrap();
    let evidence = f.formatted.as_mut().unwrap();
    evidence
        .acknowledgements
        .push(json!({"raw":STANDARD.encode(ack_bytes.raw()),"authoredContent":DESIRED}));
    evidence.readbacks.push(inventory(&fresh));
    f.canonical = Some(fresh);
    let finished = checklist::text_ops::finish_text(shell, &attempt, accepted);
    super::refresh(shell);
    finished
}

/// Simulated fresh preflight response, not a published read or a user rename.
pub(in crate::canvas::text) fn change_preflight_title(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let f = shell.canvas_checklist.text_fixture.as_mut().unwrap();
    if f.attempts != 1 || f.pending.is_some() {
        return false;
    }
    f.canonical = Some(title_source(
        f.canonical.as_ref().unwrap(),
        STALE_TITLE,
        3000,
    ));
    true
}

pub(in crate::canvas::text) fn proof(shell: &ShellState) -> Value {
    let f = shell.canvas_checklist.text_fixture.as_ref().unwrap();
    let evidence = f.formatted.as_ref().unwrap();
    let view = checklist::presentation(shell).unwrap();
    json!({"attempts":f.attempts,"pending":f.pending.is_some(),"preflights":evidence.preflights,
        "writes":evidence.writes,"acknowledgements":evidence.acknowledgements,"readbacks":evidence.readbacks,
        "fresh":inventory(f.canonical.as_ref().unwrap()),"visible":view.snapshot.as_ref().map(|s|inventory(s)),
        "visit":view.visit,"read":view.read,"stale":view.stale,"loading":view.loading,
        "operation":view.operation.map(|(serial,_,stage)|json!({"serial":serial,"stage":format!("{stage:?}")}))})
}
