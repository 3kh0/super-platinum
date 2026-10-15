//! Isolated synthetic completion for text edits on structurally inserted rows.
use super::*;
use super_platinum_core::{
    canvas::{
        edit::text::{AcceptedText, PreparedTextEdit},
        wire::{EditAck, EncodedField as F, RawMessage, Section},
    },
    slack::api::CanvasCollabSnapshot,
};
pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-structure-text" | "canvas-structure-text-narrow")
        )
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    (requested() || super::replacement_fixture::requested()) && structure_fixture::enabled(shell)
}
pub(super) fn dispatch(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: Arc<TextIntent>,
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
    let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    fixture.text_attempts += 1;
    fixture.pending_text = Some((attempt.clone(), prepared));
}
fn canonical(
    prepared: &PreparedTextEdit,
    previous: &CanvasCollabSnapshot,
    now: u64,
) -> (AcceptedText, Arc<CanvasCollabSnapshot>) {
    let update = Section::from_raw(prepared.document_data().message(1).unwrap().unwrap()).unwrap();
    let old = previous.section(update.id()).unwrap();
    let sequence = old.sequence().unwrap().unwrap() + 1000;
    let n = |tag, value| F::uint(tag, value).unwrap();
    let m = |tag, value: &RawMessage| F::message(tag, value).unwrap();
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, now + 60_000_000)])
        .unwrap();
    assert_eq!(wire.uint(2).unwrap(), Some(0));
    let raw = old
        .merge_partial(&update)
        .unwrap()
        .raw()
        .clone()
        .replace_fields(2, &[n(2, sequence)])
        .unwrap()
        .replace_fields(38, &[m(38, &wire)])
        .unwrap();
    let data = RawMessage::from_fields(&[m(1, &raw)]).unwrap();
    let ack = EditAck::decode(
        RawMessage::from_fields(&[n(1, 1), m(2, &data)])
            .unwrap()
            .raw(),
    )
    .unwrap();
    let accepted = prepared.validate_ack(&ack).unwrap();
    let mut fresh = previous.clone();
    *fresh
        .sections
        .iter_mut()
        .find(|s| s.id() == update.id())
        .unwrap() = Section::from_raw(raw).unwrap();
    assert!(accepted.confirms_readback(&fresh).unwrap());
    (accepted, Arc::new(fresh))
}
pub(super) fn complete(shell: &mut ShellState) -> bool {
    if !enabled(shell) {
        return false;
    }
    let fixture = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
    let Some((attempt, prepared)) = fixture.pending_text.take() else {
        return false;
    };
    let (accepted, fresh) = canonical(
        &prepared,
        fixture.canonical.as_ref().unwrap(),
        checklist::save::now_usec().unwrap(),
    );
    fixture.canonical = Some(fresh);
    let finished = checklist::text_ops::finish_text(shell, &attempt, accepted);
    structure_fixture::refresh(shell);
    finished
}
pub(super) fn attempts(shell: &ShellState) -> usize {
    shell
        .canvas_checklist
        .structure_fixture
        .as_ref()
        .map_or(0, |f| f.text_attempts)
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::canvas::edit::{
        EditSession, structure::StructureIntent, text::TextDraft,
    };
    #[test]
    fn synthetic_text_completion_preserves_inserted_identity_and_original_inventory() {
        let original = structure_fixture::snapshot();
        let inserted = StructureIntent::insert(
            &original,
            "paragraph",
            super_platinum_core::canvas::edit::structure::Side::After,
            &TextDraft::from_rtml("Inserted").unwrap(),
            100,
        )
        .unwrap()
        .prepare(&original, 100)
        .unwrap();
        let id = inserted.target_section().to_owned();
        let (_, mut current) = structure_fixture::accepted(inserted, &original, 100);
        let mut session = EditSession::new();
        for text in ["First <i>edit</i>", "Second <b>edit</b>"] {
            let prepared = TextIntent::capture(&current, &id, 100)
                .unwrap()
                .prepare(
                    &current,
                    &mut session,
                    &TextDraft::from_rtml(text).unwrap(),
                    100,
                )
                .unwrap();
            let (accepted, fresh) = canonical(&prepared, &current, 100);
            assert!(accepted.confirms_readback(&fresh).unwrap());
            session.remember_text_accepted(&accepted).unwrap();
            assert_eq!(
                fresh
                    .section(&id)
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .uint(2)
                    .unwrap(),
                Some(0)
            );
            for old in &original.sections {
                assert_eq!(fresh.section(old.id()), Some(old));
            }
            current = fresh;
        }
        let delete = StructureIntent::delete(&current, &id, 100)
            .unwrap()
            .prepare(&current, 100)
            .unwrap();
        let (accepted, restored) = structure_fixture::accepted(delete, &current, 100);
        assert!(accepted.confirms_readback(&restored).unwrap());
        assert_eq!(restored.sections, original.sections);
    }
}
