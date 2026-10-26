use super::*;
use super_platinum_core::canvas::{
    edit::{
        structure::ReplacementIntent,
        text::{TextDraft, TextIntent},
    },
    wire::EncodedField as F,
};
fn load(
    shell: &mut ShellState,
    snapshot: Arc<super_platinum_core::slack::api::CanvasCollabSnapshot>,
) -> checklist::Read {
    let read = checklist::begin_read(shell).unwrap();
    assert!(checklist::finish_read(shell, &read, Ok(snapshot)));
    read
}
fn setup() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Pair".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    load(&mut shell, replacement_fixture::snapshot());
    shell
}
fn begin(shell: &mut ShellState) -> (checklist::Attempt, ReplacementIntent) {
    let view = checklist::presentation(shell).unwrap();
    checklist::replacement_ops::begin_replacement(shell, view.visit, view.read, "paragraph", 100)
        .unwrap()
}
#[test]
fn sole_main_canvas_action_maps_exact_anchor_without_widening_single_delete() {
    let shell = setup();
    let snapshot = replacement_fixture::snapshot();
    let map = Mapping::project(
        &shell,
        &structure_fixture::document(&snapshot),
        Some(&snapshot),
    );
    assert!(map.items["paragraph"].replacement);
    assert!(!map.items["paragraph"].structure.as_ref().unwrap().delete);
    let mut noncanvas = setup();
    noncanvas.overlay = None;
    assert!(
        !Mapping::project(
            &noncanvas,
            &structure_fixture::document(&snapshot),
            Some(&snapshot)
        )
        .items["paragraph"]
            .replacement
    );
    let ambiguous = super_platinum_core::canvas::CanvasDocument::parse(
        "<div class='quip-canvas-content'><p id='paragraph'>one</p><p id='paragraph'>two</p></div>",
    )
    .unwrap();
    assert!(
        Mapping::project(&shell, &ambiguous, Some(&snapshot))
            .items
            .is_empty()
    );
}
#[test]
fn replacement_locks_entire_file_and_cancel_only_releases_unsent_attempt() {
    let mut shell = setup();
    let (attempt, intent) = begin(&mut shell);
    let view = checklist::presentation(&shell).unwrap();
    assert!(checklist::begin(&mut shell, view.visit, view.read, "paragraph", true).is_err());
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    assert!(!checklist::replacement_ops::mark_dispatched(
        &mut shell,
        &attempt,
        &intent.prepare(&attempt.snapshot, 100).unwrap()
    ));
    let (attempt, intent) = begin(&mut shell);
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    let sent = prepared.document_data().clone();
    assert!(checklist::replacement_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    assert!(!checklist::cancel_preflight(&mut shell, &attempt));
    assert_eq!(
        checklist::structure_ops::retained_request(&shell, &attempt),
        Some(sent)
    );
}
#[test]
fn paired_seal_requires_both_rows_and_keeps_replacement_editable_after_settlement() {
    let mut shell = setup();
    let (attempt, intent) = begin(&mut shell);
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    let id = prepared.replacement_section().to_owned();
    assert!(checklist::replacement_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, current) = replacement_fixture::accepted(prepared, &attempt.snapshot, 100);
    assert!(checklist::replacement_ops::finish_replacement(
        &mut shell, &attempt, accepted
    ));
    let mut missing = (*current).clone();
    missing.sections.retain(|s| s.id() != id);
    let read = load(&mut shell, Arc::new(missing));
    assert!(!checklist::replacement_ops::readback::verify_now(
        &mut shell, &read
    ));
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    let mut changed = (*current).clone();
    let row = changed.sections.iter_mut().find(|s| s.id() == id).unwrap();
    *row = super_platinum_core::canvas::wire::Section::from_raw(row.raw().remove(38).unwrap())
        .unwrap();
    let read = load(&mut shell, Arc::new(changed));
    assert!(!checklist::replacement_ops::readback::verify_now(
        &mut shell, &read
    ));
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    let read = load(&mut shell, current.clone());
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    assert!(checklist::replacement_ops::readback::verify_now(
        &mut shell, &read
    ));
    assert!(checklist::text_ops::settled(&shell, &attempt));
    assert!(TextIntent::capture(&current, &id, 100).is_ok());
    let view = checklist::presentation(&shell).unwrap();
    assert!(
        checklist::text_ops::begin_text(
            &mut shell,
            view.visit,
            view.read,
            &id,
            Arc::new(TextDraft::from_rtml("Continue").unwrap())
        )
        .is_ok()
    );
}
#[test]
fn uncertain_pair_retains_both_targets_across_reopen_even_with_matching_result() {
    let mut shell = setup();
    let (attempt, intent) = begin(&mut shell);
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    let sent = prepared.document_data().clone();
    assert!(checklist::replacement_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (_, current) = replacement_fixture::accepted(prepared, &attempt.snapshot, 100);
    assert!(checklist::finish(
        &mut shell,
        &attempt,
        checklist::Outcome::Uncertain
    ));
    checklist::close_visit(&mut shell);
    shell.shortcut_generation += 1;
    checklist::open_visit(&mut shell);
    let read = load(&mut shell, current);
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    assert_eq!(
        checklist::structure_ops::retained_request(&shell, &attempt),
        Some(sent)
    );
    assert!(matches!(
        checklist::presentation(&shell).unwrap().operation,
        Some((_, _, checklist::Stage::Uncertain))
    ));
}
#[test]
fn pair_dispatch_refuses_other_prepared_id_and_snapshot_unknown_change() {
    let mut shell = setup();
    let (attempt, intent) = begin(&mut shell);
    let other = ReplacementIntent::main_canvas(&attempt.snapshot, "paragraph", 100)
        .unwrap()
        .prepare(&attempt.snapshot, 100)
        .unwrap();
    assert!(!checklist::replacement_ops::mark_dispatched(
        &mut shell, &attempt, &other
    ));
    let mut changed = (*attempt.snapshot).clone();
    let title = changed
        .sections
        .iter_mut()
        .find(|s| s.id() == "title")
        .unwrap();
    *title = super_platinum_core::canvas::wire::Section::from_raw(
        title
            .raw()
            .replace_fields(1001, &[F::uint(1001, 2).unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(intent.prepare(&changed, 100).is_err());
}

#[test]
fn retained_old_tombstone_preserves_text_edit_but_does_not_grant_structural_eligibility() {
    let now = checklist::save::now_usec().unwrap();
    let mut shell = setup();
    let view = checklist::presentation(&shell).unwrap();
    let (attempt, intent) = checklist::replacement_ops::begin_replacement(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        now,
    )
    .unwrap();
    let prepared = intent.prepare(&attempt.snapshot, now).unwrap();
    let id = prepared.replacement_section().to_owned();
    assert!(checklist::replacement_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, current) = replacement_fixture::accepted(prepared, &attempt.snapshot, now);
    let old = accepted.deleted_section().clone();
    let mut current = (*current).clone();
    current.sections.push(old.clone());
    assert!(accepted.confirms_readback(&current).unwrap());
    assert!(checklist::replacement_ops::finish_replacement(
        &mut shell, &attempt, accepted
    ));
    let current = Arc::new(current);
    let read = load(&mut shell, current.clone());
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    assert!(checklist::replacement_ops::readback::verify_now(
        &mut shell, &read
    ));
    let map = Mapping::project(
        &shell,
        &structure_fixture::document(&current),
        Some(&current),
    );
    let item = &map.items[&id];
    assert!(item.intent.is_some());
    assert!(item.structure.is_none());
    assert!(!item.replacement);
    let certified = checklist::presentation(&shell).unwrap().snapshot.unwrap();
    let certified_map = Mapping::project(
        &shell,
        &structure_fixture::document(&certified),
        Some(&certified),
    );
    assert!(certified_map.items[&id].replacement);
    assert!(!map.items.contains_key("paragraph"));
    let edit = TextIntent::capture(&current, &id, now)
        .unwrap()
        .prepare(
            &current,
            &mut super_platinum_core::canvas::edit::EditSession::new(),
            &TextDraft::from_rtml("Continue with retained tombstone").unwrap(),
            now,
        )
        .unwrap();
    assert_eq!(edit.target_section(), id);
    assert_eq!(edit.document_data().messages(1).unwrap().len(), 1);
    assert_eq!(current.section(old.id()), Some(&old));
}
