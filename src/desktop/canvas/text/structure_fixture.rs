//! Disconnected structural fixture. All completions use real core ACK guards.
use super::*;
mod mixed;

use super_platinum_core::{
    canvas::{
        CanvasDocument,
        edit::structure::{AcceptedStructure, PreparedStructureEdit, StructureIntent},
        wire::{EditAck, EncodedField as F, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};
pub(in crate::canvas) const HTML: &str = "<div class='quip-canvas-content'><h1>Paragraph structure verification</h1><p id='paragraph'>First paragraph</p><p id='neighbor'>Another paragraph</p></div>";
#[derive(Default)]
pub(in crate::canvas) struct State {
    pub attempts: usize,
    pub(super) canonical: Option<Arc<CanvasCollabSnapshot>>,
    pub(super) text_attempts: usize,
    pub(super) pending_text: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::text::PreparedTextEdit,
    )>,
    pending: Option<(checklist::Attempt, PreparedStructureEdit)>,
    pub(super) pair_reservations: usize,
    pub(super) reserved_replacement: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::structure::PreparedReplacement,
        checklist::recovery::Ticket,
    )>,
    pub(super) pending_replacement: Option<(
        checklist::Attempt,
        super_platinum_core::canvas::edit::structure::PreparedReplacement,
    )>,
}
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-replacement"
                | "canvas-replacement-narrow"
                | "canvas-structure"
                | "canvas-structure-narrow"
                | "canvas-structure-mixed"
                | "canvas-structure-mixed-narrow"
                | "canvas-local-review"
                | "canvas-local-review-narrow"
                | "canvas-structure-text"
                | "canvas-structure-text-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.canvas_checklist.structure_fixture.is_some()
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
pub(in crate::canvas) fn snapshot() -> Arc<CanvasCollabSnapshot> {
    let mut snapshot = (*super::fixture::snapshot("First <b>paragraph</b>", 1000)).clone();
    snapshot
        .sections
        .retain(|section| matches!(section.id(), "title" | "paragraph"));
    let original = snapshot.section("paragraph").unwrap();
    let neighbor = Section::from_raw(
        original
            .raw()
            .replace_fields(1, &[t(1, "neighbor")])
            .unwrap()
            .replace_fields(
                12,
                &[m(
                    12,
                    &msg(&[m(1, &msg(&[t(1, "Another paragraph"), t(2, "en")]))]),
                )],
            )
            .unwrap(),
    )
    .unwrap();
    snapshot.sections.push(neighbor);
    for section in &mut snapshot.sections {
        let position = match section.id() {
            "title" => "aaa",
            "paragraph" => "aac",
            _ => "aae",
        };
        *section =
            Section::from_raw(section.raw().replace_fields(8, &[t(8, position)]).unwrap()).unwrap();
    }
    // Unknown noncanonical varint encoding must survive the whole UI workflow.
    let neighbor = snapshot
        .sections
        .iter_mut()
        .find(|s| s.id() == "neighbor")
        .unwrap();
    let mut bytes = neighbor.raw().raw().to_vec();
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    *neighbor = Section::decode(&bytes).unwrap();
    snapshot.document = snapshot
        .document
        .replace_fields(42, &[t(42, "xyz")])
        .unwrap();
    Arc::new(snapshot)
}
pub(in crate::canvas) fn document(snapshot: &CanvasCollabSnapshot) -> CanvasDocument {
    if snapshot.section("mixed-heading").is_some() {
        return mixed::document(snapshot);
    }
    let mut sections: Vec<_> = snapshot
        .sections
        .iter()
        .filter(|s| {
            s.section_type().ok().flatten().unwrap_or(0) == 0
                && s.deleted().ok().flatten() != Some(true)
        })
        .collect();
    sections.sort_by_key(|s| s.raw().string(8).ok().flatten().unwrap_or(""));
    let mut html =
        "<div class='quip-canvas-content'><h1>Paragraph structure verification</h1>".to_owned();
    for section in sections {
        let text = section
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        // Only our disconnected synthetic core-produced IDs/RTML reach this parser.
        html.push_str(&format!(
            "<p id='{}'>{}</p>",
            section.id(),
            text.string(1).unwrap().unwrap_or("")
        ));
    }
    html.push_str("</div>");
    CanvasDocument::parse(&html).unwrap()
}
pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() || shell.core.transport.is_some() || shell.core.session.is_some() {
        return;
    }
    let initial = if super::replacement_fixture::requested() {
        let snapshot = super::replacement_fixture::snapshot();
        if !super::replacement_fixture::install_owner(shell, &snapshot) {
            return;
        }
        snapshot
    } else if mixed::requested() {
        mixed::snapshot()
    } else {
        snapshot()
    };
    checklist::open_visit(shell);
    if shell.canvas_checklist.structure_fixture.is_none() {
        shell.canvas_checklist.structure_fixture = Some(State {
            canonical: Some(initial),
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
    let snapshot = shell
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    checklist::finish_read(shell, &read, Ok(snapshot));
    if let Some(attempt) = checklist::pending_attempt(shell) {
        checklist::confirm_readback(shell, &attempt, &read);
    }
}
pub(in crate::canvas) fn dispatch(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: StructureIntent,
) {
    if !enabled(shell) {
        return;
    }
    let fresh = shell
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .unwrap()
        .canonical
        .clone()
        .unwrap();
    let Ok(prepared) = intent.prepare(&fresh, checklist::save::now_usec().unwrap()) else {
        checklist::cancel_preflight(shell, attempt);
        return;
    };
    if !checklist::structure_ops::mark_dispatched(shell, attempt, &prepared) {
        return;
    }
    let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    fixture.attempts += 1;
    fixture.pending = Some((attempt.clone(), prepared));
}
pub(in crate::canvas) fn accepted(
    prepared: PreparedStructureEdit,
    original: &CanvasCollabSnapshot,
    now: u64,
) -> (AcceptedStructure, Arc<CanvasCollabSnapshot>) {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let prior = original.section(update.id());
    let sequence = prior.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
    let mut canonical = prior.map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap());
    let mut raw = canonical.raw().clone();
    for field in [
        n(2, sequence),
        t(6, original.thread_id()),
        t(7, original.document_id()),
    ] {
        raw = raw
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    if let Some(position) = raw
        .string(8)
        .unwrap()
        .map(str::to_owned)
        .and_then(|p| p.strip_suffix(":temp").map(str::to_owned))
    {
        raw = raw
            .replace_fields(8, &[t(8, &position)])
            .unwrap()
            .replace_fields(21, &[t(21, &position)])
            .unwrap();
    }
    let wire = raw
        .message(38)
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, now + 60_000_000)])
        .unwrap();
    canonical = Section::from_raw(raw.replace_fields(38, &[m(38, &wire)]).unwrap()).unwrap();
    let ack = EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, canonical.raw())]))]).raw()).unwrap();
    let receipt = prepared.validate_ack(&ack).unwrap();
    let mut current = original.clone();
    current.sections.retain(|s| s.id() != canonical.id());
    if canonical.deleted().unwrap() != Some(true) {
        current.sections.push(canonical);
    }
    (receipt, Arc::new(current))
}
pub(super) fn complete(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    let Some((attempt, prepared)) = fixture.pending.take() else {
        return false;
    };
    let (receipt, snapshot) = accepted(
        prepared,
        fixture.canonical.as_ref().unwrap(),
        checklist::save::now_usec().unwrap(),
    );
    fixture.canonical = Some(snapshot);
    let finished = checklist::structure_ops::finish_structure(shell, &attempt, receipt);
    refresh(shell);
    finished
}
pub(super) fn uncertain(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let Some((attempt, _)) = shell
        .canvas_checklist
        .structure_fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
    else {
        return false;
    };
    checklist::finish(shell, &attempt, checklist::Outcome::Uncertain)
}
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .map_or(0, |f| f.attempts)
}
