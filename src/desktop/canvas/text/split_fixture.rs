//! Offline split pair with independent bodies, original CRDT, and real journal.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use super_platinum_core::{
    canvas::{
        CanvasDocument,
        edit::structure::{EndIntent, PreparedEndEdit, PreparedSplitEdit, SplitIntent},
        wire::{EditAck, EncodedField as F, LoadDataResponse, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};

mod completion;
mod preparation;
mod text_completion;
mod text_preparation;
pub(super) use text_completion::complete_text;
pub(super) use text_preparation::dispatch as dispatch_text_multi;
mod empty;
mod repeat;
pub(super) mod sequence;
pub(super) fn is_sequence() -> bool {
    mode() == "sequence"
}
pub(super) use repeat::{
    checkpoint as repeat_checkpoint, history as repeat_history, layout as repeat_layout,
    matches as matches_repeat, snapshot as repeat_snapshot, valid as repeat_valid,
};
pub(super) fn is_repeat() -> bool {
    mode() == "repeat"
}

mod end_original;
pub(super) use empty::{
    history as empty_history, layout as empty_layout, matches as matches_empty,
    valid as empty_valid,
};
pub(super) fn is_empty() -> bool {
    mode() == "empty"
}
pub(super) fn parent_id() -> &'static str {
    if (pre_enter::enabled() || parent_edit::enabled() || second_middle::enabled())
        && parent_mode() == "retained-zero"
    {
        "temp:C:Dte0000000000000000000000001"
    } else if is_sequence() {
        sequence::parent()
    } else if is_repeat() {
        repeat::parent()
    } else if is_empty() {
        empty::parent()
    } else {
        "paragraph"
    }
}
pub(super) fn empty_snapshot(snapshot: &CanvasCollabSnapshot) -> Arc<CanvasCollabSnapshot> {
    empty::snapshot(snapshot)
}
mod middle;
pub(super) mod parent_edit;
pub(super) mod pre_enter;
pub(super) mod second_middle;
mod start;
mod start_typing;
pub(super) use start::dispatch as dispatch_start;
pub(super) use start_typing::dispatch as dispatch_start_typing;
mod pair;
mod reuse;
mod typing;
use pair::PreparedPair;

pub(super) use middle::{is_typed as is_middle_typing, parent_mode, parent_valid};

pub(super) fn typing() -> &'static str {
    typing::selector()
}
pub(super) fn typing_valid() -> bool {
    typing::valid()
}
pub(super) fn typing_reports() -> &'static [&'static str] {
    typing::reports()
}
pub(super) fn typing_updates() -> Vec<super_platinum_core::canvas::edit::text::TextDraft> {
    typing::updates()
}
pub(super) fn is_start_typing() -> bool {
    is_start() && typing() != "none"
}
pub(super) fn parent_text() -> &'static str {
    if second_middle::enabled() {
        second_middle::final_text(0)
    } else if parent_edit::enabled() {
        parent_edit::final_text()
    } else if pre_enter::enabled() {
        pre_enter::parent_text()
    } else if is_start_typing() {
        typing_reports().last().copied().unwrap()
    } else {
        corpus().left
    }
}
pub(super) fn typing_final_text() -> &'static str {
    if is_start() {
        parent_text()
    } else if is_repeat() {
        second_child_text()
    } else {
        child_text()
    }
}
pub(super) fn second_child_text() -> &'static str {
    typing_reports().last().copied().unwrap_or("")
}
pub(super) fn child_text() -> &'static str {
    if second_middle::enabled() {
        second_middle::final_text(1)
    } else {
        typing::child()
    }
}
pub(super) fn continued_text() -> &'static str {
    typing::continued()
}
pub(super) fn reuse_content() -> &'static str {
    reuse::content()
}
pub(super) fn reuse_source_rtml() -> &'static str {
    reuse::source_rtml()
}
pub(super) fn reuse_base() -> &'static str {
    reuse::base()
}
pub(super) fn reuse_valid() -> bool {
    reuse::valid()
}
pub(super) fn reuse_child() -> &'static str {
    if is_sequence() {
        sequence::following()
    } else if is_repeat() {
        repeat::following()
    } else if is_empty() {
        empty::following()
    } else {
        reuse::child_id()
    }
}

/// Two fixed fixture corpora; no caller-supplied text or offset admission.
pub(super) struct Corpus {
    pub name: &'static str,
    pub seed: &'static str,
    pub left: &'static str,
    pub right: &'static str,
    pub edited: &'static str,
    pub offset: usize,
    pub seed_rtml: &'static str,
    pub left_rtml: &'static str,
    edited_rtml: &'static str,
    html: &'static str,
}
const ASCII: Corpus = Corpus {
    name: "ascii",
    seed: "Keep thisSecond paragraph",
    left: "Keep this",
    right: "Second paragraph",
    edited: "Keep this edited",
    offset: 9,
    seed_rtml: "Keep thisSecond paragraph",
    left_rtml: "Keep this",
    edited_rtml: "Keep this edited",
    html: "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='paragraph'>Keep thisSecond paragraph</p><p id='neighbor'>Another paragraph</p></div>",
};
const UNICODE: Corpus = Corpus {
    name: "unicode",
    seed: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}Second paragraph",
    left: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}",
    right: "Second paragraph",
    edited: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600} edited",
    seed_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second paragraph",
    left_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}",
    edited_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600} edited",
    html: "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}Second paragraph</p><p id='neighbor'>Another paragraph</p></div>",
    offset: 11,
};
const END_ASCII: Corpus = Corpus {
    seed: "Keep this",
    seed_rtml: "Keep this",
    right: "",
    edited: "New paragraph",
    edited_rtml: "New paragraph",
    html: "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='paragraph'>Keep this</p><p id='neighbor'>Another paragraph</p></div>",
    ..ASCII
};
const END_UNICODE: Corpus = Corpus {
    seed: "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}",
    seed_rtml: "Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}",
    right: "",
    edited: "New \u{65e5}\u{672c}\u{1f600} paragraph",
    edited_rtml: "New \u{65e5}\u{672c}\u{1f600} paragraph",
    html: "<div class='quip-canvas-content'><h1>Paragraph splitting</h1><p id='paragraph'>Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}</p><p id='neighbor'>Another paragraph</p></div>",
    ..UNICODE
};
pub(super) fn mode() -> &'static str {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MODE").as_deref() {
        Ok("end") => "end",
        Ok("reuse") => "reuse",
        Ok("start") => "start",
        Ok("empty") => "empty",
        Ok("repeat") => "repeat",
        Ok("sequence") => "sequence",
        _ => "middle",
    }
}
pub(super) fn is_end() -> bool {
    matches!(mode(), "end" | "reuse" | "empty" | "repeat" | "sequence")
}
pub(super) fn is_start() -> bool {
    mode() == "start"
}
pub(super) fn is_reuse() -> bool {
    mode() == "reuse"
        || (is_sequence() && sequence::layout() == "empty-final-following")
        || (is_repeat() && repeat_layout() == "empty-final-following")
        || (is_empty() && empty_layout() == "empty-final-following")
}
pub(super) fn mode_valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_MODE") {
        Ok(value) => matches!(
            value.as_str(),
            "middle" | "end" | "reuse" | "start" | "empty" | "repeat" | "sequence"
        ),
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
pub(super) fn corpus() -> &'static Corpus {
    if is_sequence() {
        return sequence::corpus();
    }
    if is_repeat() {
        return repeat::corpus();
    }
    if is_empty() {
        return empty::corpus();
    }
    if is_start() {
        return start::corpus();
    }
    match (
        is_end(),
        std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS").as_deref() == Ok("unicode"),
    ) {
        (false, false) => &ASCII,
        (false, true) if is_middle_typing() => &middle::UNICODE_TYPED,
        (false, true) => &UNICODE,
        (true, false) => &END_ASCII,
        (true, true) => &END_UNICODE,
    }
}
pub(super) fn corpus_valid() -> bool {
    match std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS") {
        Ok(value) => matches!(value.as_str(), "ascii" | "unicode"),
        Err(std::env::VarError::NotPresent) => true,
        Err(_) => false,
    }
}
pub(super) fn edited_html() -> String {
    format!("<div>{}</div>", typing::escaped(continued_text()))
}
pub(in crate::canvas) fn html() -> &'static str {
    if is_empty() {
        corpus().html
    } else if is_reuse() {
        reuse::html()
    } else {
        corpus().html
    }
}
const ACCOUNT: &str = "00000000-0000-4000-8000-000000009996";

#[derive(Default)]
pub(in crate::canvas) struct State {
    canonical: Option<Arc<CanvasCollabSnapshot>>,
    pending: Option<(checklist::Attempt, PreparedPair)>,
    pending_start_typing: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::structure::PreparedStartTypingEdit,
    )>,
    pending_start: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::structure::PreparedStructureEdit,
    )>,
    pending_text: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::text::PreparedTextEdit,
        Option<checklist::session_worker::Lease>,
    )>,
    text_completing: Option<Arc<()>>,
    preflights: Vec<Value>,
    writes: Vec<Value>,
    acknowledgements: Vec<Value>,
    readbacks: Vec<Value>,
    reservations: usize,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-structure" | "canvas-structure-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.canvas_checklist.split_fixture.is_some()
}
pub(super) fn scenario() -> String {
    std::env::var("SUPER_PLATINUM_CANVAS_SPLIT_SCENARIO").unwrap_or_else(|_| "success".into())
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn body(value: &str, language: bool) -> RawMessage {
    let mut text = vec![t(1, value)];
    if language {
        text.push(t(2, "en"));
    }
    msg(&[m(1, &msg(&text))])
}
fn rebuild(previous: &CanvasCollabSnapshot, rows: Vec<RawMessage>) -> Arc<CanvasCollabSnapshot> {
    let mut payload = vec![m(3, &previous.thread), m(6, &previous.document)];
    payload.extend(rows.iter().map(|row| m(7, row)));
    let mut index = vec![t(1, "Section/document")];
    index.extend(
        rows.iter()
            .map(|row| m(2, &msg(&[t(1, row.string(1).unwrap().unwrap())]))),
    );
    let response = msg(&[n(1, 1), m(2, &msg(&payload)), m(3, &msg(&index))]);
    let load = LoadDataResponse::decode(msg(&[m(2, &response)]).raw()).unwrap();
    Arc::new(
        CanvasCollabSnapshot::from_responses(previous.file.clone(), previous.init.clone(), load)
            .unwrap(),
    )
}
pub(super) fn parent_source(raw: &RawMessage) -> RawMessage {
    if pre_enter::enabled() || parent_edit::enabled() || second_middle::enabled() {
        return pre_enter::parent_source(raw);
    }
    if is_start_typing() {
        return start_typing::parent_source(raw);
    }
    if is_end() {
        raw.replace_fields(38, &[m(38, &end_original::wire(corpus().name))])
            .unwrap()
    } else {
        middle::parent_source(raw)
    }
}
fn initial() -> Arc<CanvasCollabSnapshot> {
    if pre_enter::enabled() || parent_edit::enabled() || second_middle::enabled() {
        return pre_enter::snapshot(&super::structure_fixture::snapshot());
    }
    if is_sequence() {
        return sequence::snapshot(&super::structure_fixture::snapshot());
    }
    if is_repeat() {
        return repeat::snapshot(&super::structure_fixture::snapshot());
    }
    if is_empty() {
        return empty::snapshot(&super::structure_fixture::snapshot());
    }
    let previous = super::structure_fixture::snapshot();
    let rows = previous
        .sections
        .iter()
        .map(|row| {
            if row.id() == parent_id() {
                parent_source(
                    &row.raw()
                        .replace_fields(12, &[m(12, &body(corpus().seed_rtml, true))])
                        .unwrap(),
                )
            } else {
                row.raw().clone()
            }
        })
        .collect();
    reuse_source(rebuild(&previous, rows))
}
pub(super) fn reuse_source(snapshot: Arc<CanvasCollabSnapshot>) -> Arc<CanvasCollabSnapshot> {
    if is_reuse() {
        reuse::snapshot(&snapshot)
    } else {
        snapshot
    }
}
pub(in crate::canvas) async fn install(mut state: Signal<ShellState>, mut reader: Signal<Reader>) {
    if !requested() || state.peek().core.session.is_some() || state.peek().core.transport.is_some()
    {
        return;
    }
    let initial = initial();
    {
        let mut shell = state.write();
        if shell.core.active_team.as_deref() != Some(initial.init.workspace_id()) {
            return;
        }
        let Some(workspace) = shell.core.workspaces.get_mut(initial.init.workspace_id()) else {
            return;
        };
        workspace.self_user_id = "U1".into();
        shell.self_account.user_id = "U1".into();
        shell.core.active_account = Some(ACCOUNT.into());
        shell.canvas_checklist.split_fixture = Some(State {
            canonical: Some(initial.clone()),
            ..Default::default()
        });
        checklist::open_visit(&mut shell);
    }
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return;
    };
    checklist::recovery::load(state, &read).await;
    let mut shell = state.write();
    checklist::finish_read(&mut shell, &read, Ok(initial));
    checklist::reader::update_mapping(&shell, &mut reader.write());
}
pub(in crate::canvas) fn document(snapshot: &CanvasCollabSnapshot) -> CanvasDocument {
    super::structure_fixture::document(snapshot)
}
pub(super) fn inventory(snapshot: &CanvasCollabSnapshot) -> Value {
    super::fixture::formatted_title::inventory(snapshot)
}
pub(super) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: SplitIntent,
) -> Result<(), &'static str> {
    if !enabled(&state.peek()) {
        return Err("The disconnected split fixture is unavailable.");
    }
    let fresh = state
        .peek()
        .canvas_checklist
        .split_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    if !preparation::current(&state.peek(), attempt, &fresh) {
        return Err("This canvas view changed before the edit was sent.");
    }
    let now = checklist::save::now_usec().ok_or("Could not check the canvas clock.")?;
    let (prepared, preflight, write) = preparation::prepare(
        intent,
        fresh.clone(),
        attempt.snapshot.clone(),
        checklist::split_ops::witness(attempt).unwrap().clone(),
        now,
        second_middle::enabled(),
    )
    .await?;
    if !enabled(&state.peek()) || !preparation::current(&state.peek(), attempt, &fresh) {
        return Err("This canvas view changed before the edit was sent.");
    }
    state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .preflights
        .push(preflight);
    let (prepared, checkpoint) =
        prepared.map_err(|_| "This paragraph or your access changed. Cancel and refresh.")?;
    let write = write.ok_or("The disconnected split evidence is unavailable.")?;
    let ticket = checklist::recovery::reserve_split(state, attempt, checkpoint).await?;
    if !enabled(&state.peek())
        || !preparation::current(&state.peek(), attempt, &fresh)
        || !checklist::split_ops::mark_dispatched(&mut state.write(), attempt, &prepared)
    {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas view changed before the edit was sent.");
    }
    let mut shell = state.write();
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    f.reservations += 1;
    f.writes.push(write);
    f.pending = Some((attempt.clone(), PreparedPair::Middle(prepared)));
    Ok(())
}
pub(super) async fn dispatch_end(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    intent: EndIntent,
) -> Result<(), &'static str> {
    if !enabled(&state.peek()) {
        return Err("The disconnected split fixture is unavailable.");
    }
    let fresh = state
        .peek()
        .canvas_checklist
        .split_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    if !checklist::dispatchable(&state.peek(), attempt) {
        return Err("This canvas view changed before the edit was sent.");
    }
    let prepared = super::end_save::prepare(
        intent,
        fresh.clone(),
        checklist::end_ops::witness(attempt).unwrap().clone(),
        checklist::save::now_usec().unwrap(),
    )
    .await;
    if !enabled(&state.peek()) || !checklist::dispatchable(&state.peek(), attempt) {
        return Err("This canvas view changed before the edit was sent.");
    }
    state.write().canvas_checklist.split_fixture.as_mut().unwrap().preflights.push(json!({
        "source":inventory(&attempt.snapshot),"fresh":inventory(&fresh),
        "outcome":match &prepared {Ok(_)=>"prepared".to_owned(),Err(error)=>format!("{error:?}")}}));
    let (prepared, checkpoint) =
        prepared.map_err(|_| "This paragraph or your access changed. Cancel and refresh.")?;
    if prepared.clears_child() != (is_reuse() && reuse_content() != "empty")
        || prepared.reuses_child() != is_reuse()
        || (is_reuse() && prepared.child_section() != reuse_child())
    {
        return Err("The disconnected reuse fixture selected an unexpected child.");
    }
    let ticket = checklist::recovery::reserve_end(state, attempt, checkpoint).await?;
    if !checklist::end_ops::mark_dispatched(&mut state.write(), attempt, &prepared) {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas view changed before the edit was sent.");
    }
    let mut shell = state.write();
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    f.reservations += 1;
    f.writes.push(json!({"kind":if is_sequence(){"sequence"}else if is_repeat(){"repeat"}else if is_empty(){"empty"}else if is_reuse(){"reuse"}else{"end"},"documentData":STANDARD.encode(prepared.document_data().raw()),
        "formTitle":prepared.form_title(),"session":prepared.session_id(),"sequence":prepared.sequence(),
        "parent":prepared.target_section(),"parentUpdates":prepared.parent_updates().iter().map(|d|d.rtml().unwrap()).collect::<Vec<_>>(),"parentFinalRtml":prepared.parent_draft().rtml().unwrap(),"child":prepared.child_section(),"secondChild":prepared.second_child_section(),"members":if prepared.is_sequence(){prepared.appended_members().map(|m|json!({"id":m.id(),"position":m.position(),"reused":m.reused(),"updates":m.updates().iter().map(|d|d.rtml().unwrap()).collect::<Vec<_>>(),"finalRtml":m.draft().rtml().unwrap()})).collect::<Vec<_>>()}else{Vec::new()},"clearsChild":prepared.clears_child(),"httpFactoryExecuted":false}));
    f.pending = Some((attempt.clone(), PreparedPair::End(prepared)));
    Ok(())
}
fn authored_rows(prepared: &PreparedPair, previous: &CanvasCollabSnapshot) -> Vec<RawMessage> {
    prepared
        .document_data()
        .fields()
        .iter()
        .filter(|f| f.tag() == 1)
        .map(|field| {
            let update = RawMessage::decode(prepared.document_data().encoded_field(field).unwrap())
                .unwrap()
                .message(1)
                .unwrap()
                .unwrap();
            let parent = update.string(1).unwrap() == Some(parent_id());
            let reused = is_reuse() && update.string(1).unwrap() == Some(reuse_child());
            let sequence = if parent || reused { 2000 } else { 1000 };
            let raw = if parent {
                previous
                    .section(parent_id())
                    .unwrap()
                    .raw()
                    .merge_partial(&update)
                    .unwrap()
            } else if reused {
                previous
                    .section(reuse_child())
                    .unwrap()
                    .raw()
                    .merge_partial(&update)
                    .unwrap()
            } else {
                update
            };
            let position = raw
                .string(8)
                .unwrap()
                .unwrap()
                .strip_suffix(":temp")
                .unwrap_or(raw.string(8).unwrap().unwrap())
                .to_owned();
            let wire = raw
                .message(38)
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, sequence)])
                .unwrap()
                .replace_fields(
                    4,
                    &[n(4, checklist::save::now_usec().unwrap() + 60_000_000)],
                )
                .unwrap();
            let content = if let PreparedPair::Middle(middle) = prepared {
                let ids = middle.member_ids();
                let index = ids
                    .iter()
                    .position(|id| Some(*id) == raw.string(1).unwrap())
                    .unwrap();
                middle.member_drafts()[index].rtml().unwrap()
            } else if parent {
                if let PreparedPair::End(end) = prepared {
                    end.parent_draft().rtml().unwrap()
                } else {
                    typing::escaped(parent_text())
                }
            } else if let PreparedPair::End(end) = prepared {
                if end.is_sequence() {
                    end.appended_members()
                        .find(|member| Some(member.id()) == raw.string(1).unwrap())
                        .unwrap()
                        .draft()
                        .rtml()
                        .unwrap()
                } else if end.second_child_section() == raw.string(1).unwrap() {
                    typing::escaped(second_child_text())
                } else {
                    typing::escaped(child_text())
                }
            } else {
                typing::escaped(child_text())
            };
            let mut result = raw
                .replace_fields(2, &[n(2, sequence)])
                .unwrap()
                .replace_fields(6, &[t(6, previous.thread_id())])
                .unwrap()
                .replace_fields(7, &[t(7, previous.document_id())])
                .unwrap()
                .replace_fields(12, &[m(12, &body(&content, parent))])
                .unwrap()
                .replace_fields(38, &[m(38, &wire)])
                .unwrap();
            if !parent && !reused {
                result = result
                    .replace_fields(8, &[t(8, &position)])
                    .unwrap()
                    .replace_fields(21, &[t(21, &position)])
                    .unwrap();
            }
            result
        })
        .collect()
}
pub(super) async fn complete(mut state: Signal<ShellState>) -> bool {
    if is_start_typing() {
        return start_typing::complete(state).await;
    }
    if is_start() {
        return start::complete(state).await;
    }
    if !enabled(&state.peek()) {
        return false;
    }
    let Some((attempt, prepared)) = state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
    else {
        return false;
    };
    let previous = state
        .peek()
        .canvas_checklist
        .split_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    if matches!(&prepared, PreparedPair::End(_)) || second_middle::enabled() {
        return completion::end(state, attempt, prepared, previous).await;
    }
    let rows = authored_rows(&prepared, &previous);
    let partial = scenario() == "partial";
    let ack_rows = if partial { &rows[..1] } else { &rows[..] };
    let mut data = vec![t(15, previous.document_id())];
    data.extend(ack_rows.iter().map(|row| m(1, row)));
    let ack_bytes = msg(&[n(1, 1), m(2, &msg(&data))]);
    let ack = EditAck::decode(ack_bytes.raw()).unwrap();
    let accepted = prepared.validate_ack(&ack);
    state
        .write()
        .canvas_checklist
        .split_fixture
        .as_mut()
        .unwrap()
        .acknowledgements
        .push(json!({
        "raw":STANDARD.encode(ack_bytes.raw()),"partial":partial,"accepted":accepted.is_ok()}));
    if partial {
        if accepted.is_ok() {
            return false;
        }
        return checklist::finish(&mut state.write(), &attempt, checklist::Outcome::Uncertain);
    }
    let Ok(accepted) = accepted else { return false };
    let mut canonical = Vec::new();
    for row in &previous.sections {
        if is_reuse() {
            canonical.push(
                rows.iter()
                    .find(|updated| updated.string(1).ok().flatten() == Some(row.id()))
                    .cloned()
                    .unwrap_or_else(|| row.raw().clone()),
            );
        } else if row.id() == parent_id() {
            canonical.extend(rows.clone());
        } else {
            canonical.push(row.raw().clone());
        }
    }
    if (is_repeat() || is_sequence()) && is_reuse() {
        canonical.extend(
            rows.iter()
                .filter(|r| previous.section(r.string(1).unwrap().unwrap()).is_none())
                .cloned(),
        );
    }
    let fresh = rebuild(&previous, canonical);
    if !accepted.confirms_readback(&fresh).unwrap_or(false) {
        return false;
    }
    {
        let mut shell = state.write();
        let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
        f.readbacks.push(inventory(&fresh));
        f.canonical = Some(fresh.clone());
        if !accepted.finish(&mut shell, &attempt) {
            return false;
        }
    }
    let Some(read) = checklist::begin_read(&mut state.write()) else {
        return false;
    };
    {
        let mut shell = state.write();
        checklist::finish_read(&mut shell, &read, Ok(fresh));
        checklist::confirm_readback(&mut shell, &attempt, &read);
    }
    checklist::recovery::settle(state, &read).await;
    checklist::text_ops::settled(&state.peek(), &attempt)
}
pub(super) fn change_preflight(mut state: Signal<ShellState>) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let mut shell = state.write();
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    let previous = f.canonical.as_ref().unwrap();
    let rows = previous
        .sections
        .iter()
        .map(|row| {
            if row.id()
                == if is_reuse() {
                    reuse_child()
                } else {
                    parent_id()
                }
            {
                row.raw().replace_fields(2, &[n(2, 3000)]).unwrap()
            } else {
                row.raw().clone()
            }
        })
        .collect();
    f.canonical = Some(rebuild(previous, rows));
    true
}
pub(super) fn proof(shell: &ShellState) -> Value {
    let f = shell.canvas_checklist.split_fixture.as_ref().unwrap();
    let view = checklist::presentation(shell).unwrap();
    json!({"scenario":scenario(),"corpus":corpus().name,"mode":mode(),"typing":typing(),"parentMode":parent_mode(),"preEnter":pre_enter::selector(),"middleParent":parent_edit::selector(),"middleCadence":parent_edit::cadence(),"secondMiddle":second_middle::selector(),"secondMiddleCadence":second_middle::cadence(),"parentId":parent_id(),"sequenceCount":sequence::count(),"sequenceHistory":sequence::history(),"sequenceLayout":sequence::layout(),"sequenceCadence":sequence::cadence(),"repeatHistory":repeat_history(),"repeatLayout":repeat_layout(),"emptyHistory":empty_history(),"emptyLayout":empty_layout(),"reuseBase":reuse_base(),"reuseContent":reuse_content(),"reuseChild":is_reuse().then(reuse_child),"pending":f.pending.is_some()||f.pending_start.is_some()||f.pending_start_typing.is_some()||f.pending_text.is_some()||f.text_completing.is_some(),"preflights":f.preflights,
        "writes":f.writes,"acknowledgements":f.acknowledgements,"readbacks":f.readbacks,"reservations":f.reservations,
        "fresh":inventory(f.canonical.as_ref().unwrap()),"visible":view.snapshot.as_ref().map(|s|inventory(s)),
        "visit":view.visit,"read":view.read,"stale":view.stale,"loading":view.loading,"capacityReason":view.capacity_reason,
        "operation":view.operation.map(|(serial,_,stage)|json!({"serial":serial,"stage":format!("{stage:?}")}))})
}

pub(super) fn dispatch_text(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
) -> bool {
    if !enabled(shell) || scenario() != "success" {
        return false;
    }
    let f = shell.canvas_checklist.split_fixture.as_ref().unwrap();
    if !(f.writes.len() == 1
        || ((is_middle_typing() || is_start() || is_empty()) && f.writes.len() == 2)
        || (second_middle::enabled() && (1..=3).contains(&f.writes.len()))
        || (is_repeat() && (1..=3).contains(&f.writes.len()))
        || (is_sequence() && (1..=sequence::count() + 1).contains(&f.writes.len())))
        || f.pending.is_some()
        || f.pending_start_typing.is_some()
        || f.pending_start.is_some()
        || f.pending_text.is_some()
    {
        return false;
    }
    let fresh = f.canonical.clone().unwrap();
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
        Some(Err(e)) => format!("{e:?}"),
        None => "owner-unavailable".into(),
    };
    shell.canvas_checklist.split_fixture.as_mut().unwrap().preflights.push(json!({
        "source":inventory(&attempt.snapshot),"fresh":inventory(&fresh),"outcome":outcome,"kind":"text"}));
    let Some(Ok(prepared)) = prepared else {
        return false;
    };
    if !checklist::text_ops::mark_dispatched(shell, attempt, &prepared) {
        return false;
    }
    let f = shell.canvas_checklist.split_fixture.as_mut().unwrap();
    f.writes.push(json!({"kind":"text","parent":prepared.section_id(),"session":prepared.session_id(),
        "sequence":prepared.sequence(),"formTitle":prepared.form_title(),
        "documentData":STANDARD.encode(prepared.document_data().raw()),"httpFactoryExecuted":false}));
    f.pending_text = Some((attempt.clone(), prepared, None));
    true
}

#[cfg(test)]
mod corpus_tests {
    use super::*;
    use super_platinum_core::canvas::edit::text::TextDraft;
    #[test]
    fn fixed_corpora_keep_visible_utf16_and_escaped_rtml_distinct() {
        for corpus in [&ASCII, &UNICODE, &END_ASCII, &END_UNICODE] {
            assert_eq!(corpus.seed, format!("{}{}", corpus.left, corpus.right));
            assert_eq!(corpus.offset, corpus.left.encode_utf16().count());
            for (rtml, visible) in [
                (corpus.seed_rtml, corpus.seed),
                (corpus.left_rtml, corpus.left),
                (corpus.edited_rtml, corpus.edited),
            ] {
                let draft = TextDraft::from_rtml(rtml).unwrap();
                assert_eq!(
                    draft
                        .runs
                        .iter()
                        .map(|run| run.text.as_str())
                        .collect::<String>(),
                    visible
                );
                assert_eq!(draft.rtml().unwrap(), rtml);
            }
        }
        assert_eq!(UNICODE.offset, 11);
        assert_eq!(UNICODE.left_rtml.encode_utf16().count(), 15);
        assert_ne!(UNICODE.offset, UNICODE.left.len());
    }
}
