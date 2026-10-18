//! Actual intent/ACK/restart paths; root owns Cargo execution.
use super::*;
use crate::canvas::edit::annotation::{AnnotationPresentation, range_seeds_with_presentation};

fn stub_fixture(reactions: u64) -> (CanvasCollabSnapshot, String) {
    let (mut s, old) = existing_fixture();
    let meta = raw(&[t(1, &old), n(2, 0), n(6, reactions), n(8, 0)]);
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
        .unwrap();
    (s, old)
}
fn stub_prepare(s: &CanvasCollabSnapshot, old: &str) -> PreparedAnnotation {
    // Partial selection of a hidden, openable stub removes that mark; exact
    // whole-stub selection is independently routed to the old thread by native.
    AnnotationIntent::capture_range_with_presentation(
        s,
        "item",
        2,
        4,
        vec![old.into()],
        &AnnotationPresentation::ordinary(None, None).unwrap(),
    )
    .unwrap()
    .prepare_at(s, 2_000_000)
    .unwrap()
}
#[test]
fn hidden_stub_exact_boundaries_open_old_thread_partial_selection_does_not() {
    let (s, old) = stub_fixture(0);
    assert!(!range_seeds(&s).unwrap().iter().any(|(id, _)| id == "item"));
    let seeds =
        range_seeds_with_presentation(&s, &AnnotationPresentation::ordinary(None, None).unwrap())
            .unwrap();
    let seed = &seeds.iter().find(|(id, _)| id == "item").unwrap().1;
    assert_eq!(seed.hidden, vec![old.clone()]);
    assert!(seed.archived.is_empty());
    assert_eq!(
        seed.existing_native_control(1, 4, Some(&old)).unwrap(),
        Some(old.as_str())
    );
    assert!(seed.existing_native_control(1, 4, None).is_err());
    assert_eq!(seed.existing_native_control(2, 4, None).unwrap(), None);
    assert!(seed.existing_native_control(2, 4, Some(&old)).is_err());
    assert!(seed.existing_native_control(2, 3, None).is_err());
}
#[test]
fn reaction_and_current_open_context_change_visibility_without_rewriting_controls() {
    for reactions in [0, 1] {
        let (s, old) = stub_fixture(reactions);
        for opened in [None, Some(old.clone())] {
            let context = AnnotationPresentation::ordinary(opened.clone(), None).unwrap();
            let seeds = range_seeds_with_presentation(&s, &context).unwrap();
            let seed = &seeds.iter().find(|(id, _)| id == "item").unwrap().1;
            assert_eq!(seed.hidden.is_empty(), reactions != 0 || opened.is_some());
            let result = AnnotationIntent::capture_range_with_presentation(
                &s,
                "item",
                2,
                4,
                vec![old.clone()],
                &context,
            );
            assert_eq!(result.is_ok(), reactions == 0 && opened.is_none());
        }
    }
}
#[test]
fn v5_stores_exact_presentation_and_old_versions_cannot_inherit_hidden_authority() {
    let (s, old) = stub_fixture(0);
    let p = stub_prepare(&s, &old);
    let record = p.record().unwrap();
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["version"], 5);
    assert_eq!(value["preserved"]["presentation"]["exact_empty"], true);
    let decoded: AnnotationRecord = serde_json::from_value(value.clone()).unwrap();
    decoded.validate().unwrap();
    assert!(record == decoded);
    for version in [1, 2, 3, 4] {
        let mut changed = value.clone();
        changed["version"] = version.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for flag in ["hide_stubs", "reaction_bar", "exact_empty"] {
        let mut changed = value.clone();
        changed["preserved"]["presentation"][flag] = false.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let (old_s, old_id) = archived_fixture(true);
    assert_eq!(
        serde_json::to_value(archived_prepare(&old_s, &old_id).record().unwrap()).unwrap()["version"],
        4
    );
}
#[test]
fn hidden_stub_ack_retains_old_raw_and_metadata_on_failed_fresh_read() {
    let (s, old) = stub_fixture(0);
    let p = stub_prepare(&s, &old);
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let evidence = accepted.evidence().unwrap();
    let current = fresh(&s, control, parent);
    assert_eq!(current.section(&old), s.section(&old));
    assert_eq!(current.document, s.document);
    accepted.check_confirmation(&current).unwrap();
    let mut changed = current.clone();
    let metadata = raw(&[t(1, &old), n(2, 1), n(6, 0), n(8, 0)]);
    changed.document = changed
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &metadata)]))])
        .unwrap();
    assert!(accepted.check_confirmation(&changed).is_err());
    assert!(evidence.anchor.is_some());
    accepted.check_confirmation(&current).unwrap();
}

#[test]
fn hidden_stub_v5_ack_and_bootstrap_not_sent_survive_journal_restart() {
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let (s, old) = stub_fixture(0);
    let p = stub_prepare(&s, &old);
    let mut journal = Journal::open(&path, owner).unwrap();
    let AnnotationBegin::Started(receipt) = journal.begin_annotation(p.record().unwrap()).unwrap()
    else {
        panic!("started")
    };
    let scope = receipt.record().scope().clone();
    let pending = journal.annotation(&scope).unwrap().unwrap();
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    assert!(
        journal
            .preserve_annotation_acknowledgement(&pending, &accepted.evidence().unwrap())
            .unwrap()
    );
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &s, 1_720_000_000_001)
            .unwrap()
            .is_none()
    );
    let current = fresh(&s, control, parent);
    let proof = accepted.confirm(&current).unwrap();
    assert!(
        journal
            .annotation_anchor_confirmed(&receipt, &proof)
            .unwrap()
    );
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    let pending = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::AnchorConfirmed);
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &current, 1_720_000_000_001)
            .unwrap()
            .is_some()
    );
    let started = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::BootstrapStarted);
    assert!(
        journal
            .continue_annotation_bootstrap(&started, &current, 1_720_000_000_002)
            .unwrap()
            .is_none()
    );
    assert!(journal.annotation_not_sent(&started).unwrap());
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    let retained = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(retained.stage(), AnnotationStage::BootstrapNotSent);
    let mut changed_original = current.clone();
    let original = changed_original
        .sections
        .iter_mut()
        .find(|s| s.id() == old)
        .unwrap();
    *original = changed(original, n(2, 999999));
    assert!(
        journal
            .continue_annotation_bootstrap(&retained, &changed_original, 1_720_000_000_003)
            .is_err()
    );
    assert!(
        journal
            .continue_annotation_bootstrap(&retained, &current, 1_720_000_000_003)
            .unwrap()
            .is_some()
    );
    assert!(!journal.annotation_not_sent(&started).unwrap());
}
