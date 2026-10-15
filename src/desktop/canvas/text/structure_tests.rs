use super::*;
use super_platinum_core::{
    canvas::{
        edit::{
            structure::{PreparedStructureEdit, StructureIntent, paragraph_actions},
            text::TextDraft,
        },
        wire::{EncodedField as F, Section},
    },
    slack::api::CanvasCollabSnapshot,
};
fn loaded() -> (ShellState, Arc<CanvasCollabSnapshot>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    checklist::open_visit(&mut shell);
    let snapshot = structure_fixture::snapshot();
    load(&mut shell, Arc::clone(&snapshot));
    (shell, snapshot)
}
fn load(shell: &mut ShellState, snapshot: Arc<CanvasCollabSnapshot>) -> checklist::Read {
    let read = checklist::begin_read(shell).unwrap();
    assert!(checklist::finish_read(shell, &read, Ok(snapshot)));
    read
}
fn begin(
    shell: &mut ShellState,
    anchor: &str,
    action: Action,
) -> (checklist::Attempt, StructureIntent) {
    let view = checklist::presentation(shell).unwrap();
    checklist::structure_ops::begin_structure(
        shell,
        view.visit,
        view.read,
        anchor,
        action,
        &TextDraft::from_rtml("Added <b>paragraph</b>").unwrap(),
        100,
    )
    .unwrap()
}
fn prepare(
    shell: &mut ShellState,
    attempt: &checklist::Attempt,
    intent: StructureIntent,
) -> PreparedStructureEdit {
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    assert!(checklist::structure_ops::mark_dispatched(
        shell, attempt, &prepared
    ));
    prepared
}
#[test]
fn structural_actions_need_complete_canonical_inventory_and_exact_export_identity() {
    let (shell, snapshot) = loaded();
    let document = super_platinum_core::canvas::CanvasDocument::parse(
        "<div class='quip-canvas-content'><p id='paragraph'>Wrong export text</p></div>",
    )
    .unwrap();
    let map = Mapping::project(&shell, &document, Some(&snapshot));
    let item = &map.items["paragraph"];
    assert!(item.structure.as_ref().unwrap().delete);
    assert!(item.blocks.contains("First"));
    assert!(!item.blocks.contains("Wrong"));
    let ambiguous = super_platinum_core::canvas::CanvasDocument::parse(
        "<div class='quip-canvas-content'><p id='paragraph'>One</p><p id='paragraph'>Two</p></div>",
    )
    .unwrap();
    assert!(
        Mapping::project(&shell, &ambiguous, Some(&snapshot))
            .items
            .is_empty()
    );
    let mut sole = (*snapshot).clone();
    sole.sections.retain(|s| s.id() != "neighbor");
    assert!(
        !paragraph_actions(&sole, &["paragraph"], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
}
#[test]
fn structural_intent_shares_the_file_lock_and_only_unsent_cancel_releases_it() {
    let (mut shell, _) = loaded();
    let (attempt, _) = begin(&mut shell, "paragraph", Action::After);
    let view = checklist::presentation(&shell).unwrap();
    assert!(checklist::begin(&mut shell, view.visit, view.read, "paragraph", true).is_err());
    assert!(
        checklist::text_ops::begin_text(
            &mut shell,
            view.visit,
            view.read,
            "paragraph",
            Arc::new(TextDraft::default())
        )
        .is_err()
    );
    assert!(
        checklist::structure_ops::begin_structure(
            &mut shell,
            view.visit,
            view.read,
            "paragraph",
            Action::After,
            &TextDraft::default(),
            100
        )
        .is_err()
    );
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let _ = prepare(&mut shell, &attempt, intent);
    assert!(!checklist::cancel_preflight(&mut shell, &attempt));
}
#[test]
fn insertion_settles_then_new_base_zero_row_can_be_deleted_and_restore_inventory() {
    let (mut shell, original) = loaded();
    let (insert, intent) = begin(&mut shell, "paragraph", Action::After);
    let prepared = prepare(&mut shell, &insert, intent);
    let (accepted, current) =
        structure_fixture::accepted(prepared, &original, checklist::save::now_usec().unwrap());
    assert!(checklist::structure_ops::finish_structure(
        &mut shell, &insert, accepted
    ));
    let read = load(&mut shell, Arc::clone(&current));
    assert!(checklist::confirm_readback(&mut shell, &insert, &read));
    let map = Mapping::project(
        &shell,
        &structure_fixture::document(&current),
        Some(&current),
    );
    assert!(
        map.items[&insert.section]
            .structure
            .as_ref()
            .unwrap()
            .delete
    );
    let (delete, intent) = begin(&mut shell, &insert.section, Action::Delete);
    let prepared = prepare(&mut shell, &delete, intent);
    let (accepted, restored) = structure_fixture::accepted(prepared, &current, 100);
    assert!(checklist::structure_ops::finish_structure(
        &mut shell, &delete, accepted
    ));
    assert!(restored.section(&delete.section).is_none());
    let read = load(&mut shell, Arc::clone(&restored));
    assert!(checklist::confirm_readback(&mut shell, &delete, &read));
    assert_eq!(restored.sections, original.sections);
    assert!(checklist::text_ops::settled(&shell, &delete));
    checklist::invalidate(&mut shell, "T1", "Ffixture");
    assert!(checklist::text_ops::settled(&shell, &delete));
}
#[test]
fn uncertain_delete_survives_reopen_and_matching_absence_never_confirms_it() {
    let (mut shell, original) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::Delete);
    let prepared = prepare(&mut shell, &attempt, intent);
    let exact_sent = prepared.document_data().clone();
    assert!(checklist::finish(
        &mut shell,
        &attempt,
        checklist::Outcome::Uncertain
    ));
    checklist::close_visit(&mut shell);
    shell.shortcut_generation += 1;
    checklist::open_visit(&mut shell);
    let mut absent = (*original).clone();
    absent.sections.retain(|s| s.id() != "paragraph");
    let read = load(&mut shell, Arc::new(absent));
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    assert_eq!(
        checklist::structure_ops::retained_request(&shell, &attempt),
        Some(exact_sent)
    );
    let view = checklist::presentation(&shell).unwrap();
    assert!(matches!(
        view.operation,
        Some((_, _, checklist::Stage::Uncertain))
    ));
    assert!(checklist::begin(&mut shell, view.visit, view.read, "neighbor", true).is_err());
}
#[test]
fn stale_event_or_credential_epoch_prevents_structural_dispatch() {
    let (mut shell, _) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    checklist::invalidate(&mut shell, "T1", "Ffixture");
    assert!(!checklist::structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    let (mut shell, _) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let prepared = intent.prepare(&attempt.snapshot, 100).unwrap();
    shell.core.account_epoch += 1;
    assert!(!checklist::structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
}
#[test]
fn late_structural_ack_remains_retained_until_new_owned_canonical_read() {
    let (mut shell, original) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let prepared = prepare(&mut shell, &attempt, intent);
    checklist::close_visit(&mut shell);
    shell.shortcut_generation += 1;
    checklist::open_visit(&mut shell);
    let (accepted, current) =
        structure_fixture::accepted(prepared, &original, checklist::save::now_usec().unwrap());
    assert!(checklist::structure_ops::finish_structure(
        &mut shell, &attempt, accepted
    ));
    let read = load(&mut shell, current);
    assert!(checklist::confirm_readback(&mut shell, &attempt, &read));
    assert!(checklist::presentation(&shell).unwrap().operation.is_none());
}
#[test]
fn accepted_structural_readback_with_missing_history_does_not_unlock_file() {
    let (mut shell, original) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let prepared = prepare(&mut shell, &attempt, intent);
    let (accepted, current) =
        structure_fixture::accepted(prepared, &original, checklist::save::now_usec().unwrap());
    assert!(checklist::structure_ops::finish_structure(
        &mut shell, &attempt, accepted
    ));
    let mut missing = (*current).clone();
    let target = missing
        .sections
        .iter_mut()
        .find(|s| s.id() == attempt.section)
        .unwrap();
    *target = Section::from_raw(target.raw().remove(38).unwrap()).unwrap();
    let read = load(&mut shell, Arc::new(missing));
    assert!(!checklist::confirm_readback(&mut shell, &attempt, &read));
    assert!(checklist::presentation(&shell).unwrap().operation.is_some());
    let read = load(&mut shell, current);
    assert!(checklist::confirm_readback(&mut shell, &attempt, &read));
}
#[test]
fn changed_unknown_inventory_and_foreign_receipt_cannot_confirm_structure() {
    let (mut shell, original) = loaded();
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let mut changed = (*original).clone();
    let target = &mut changed.sections[0];
    *target = Section::from_raw(
        target
            .raw()
            .replace_fields(1001, &[F::uint(1001, 7).unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(intent.prepare(&changed, 100).is_err());
    assert!(checklist::cancel_preflight(&mut shell, &attempt));
    let (attempt, intent) = begin(&mut shell, "paragraph", Action::After);
    let _ = prepare(&mut shell, &attempt, intent);
    let foreign = Action::After
        .capture(&original, "paragraph", &TextDraft::default(), 100)
        .unwrap()
        .prepare(&original, 100)
        .unwrap();
    let (accepted, _) = structure_fixture::accepted(foreign, &original, 100);
    assert!(checklist::structure_ops::finish_structure(
        &mut shell, &attempt, accepted
    ));
    assert!(matches!(
        checklist::presentation(&shell).unwrap().operation,
        Some((_, _, checklist::Stage::Uncertain))
    ));
}
