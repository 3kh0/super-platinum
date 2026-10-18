use super::*;
use automerge::ReadDoc;

fn archived_fixture(with_metadata: bool) -> (CanvasCollabSnapshot, String) {
    let (mut s, old) = existing_fixture();
    if with_metadata {
        let container = s.document.message(16).unwrap().unwrap();
        let meta = container.messages(1).unwrap()[0]
            .replace_fields(8, &[n(8, 1)])
            .unwrap();
        s.document = s
            .document
            .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
            .unwrap();
    } else {
        s.document = s.document.remove(16).unwrap();
        let control = s.sections.iter_mut().find(|v| v.id() == old).unwrap();
        let content = control.raw().message(12).unwrap().unwrap();
        let highlight = content
            .message(7)
            .unwrap()
            .unwrap()
            .replace_fields(4, &[n(4, 1)])
            .unwrap();
        *control = changed(
            control,
            m(12, &content.replace_fields(7, &[m(7, &highlight)]).unwrap()),
        );
    }
    (s, old)
}
fn archived_prepare(s: &CanvasCollabSnapshot, old: &str) -> PreparedAnnotation {
    AnnotationIntent::capture_range_with_archived(s, "item", 1, 4, vec![old.into()])
        .unwrap()
        .prepare_at(s, 2_000_000)
        .unwrap()
}

#[test]
fn archived_mark_is_replaced_without_deleting_old_control_metadata_or_text() {
    for has_meta in [false, true] {
        let (s, old) = archived_fixture(has_meta);
        let p = archived_prepare(&s, &old);
        let record = p.record().unwrap();
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["version"], 4);
        assert_eq!(value["removal"]["ids"], serde_json::json!([old]));
        assert_eq!(value["preserved"]["metadata"][0].is_null(), !has_meta);
        let restored: AnnotationRecord = serde_json::from_value(value).unwrap();
        restored.validate().unwrap();
        assert!(record == restored);
        let updates = p.document_data().messages(1).unwrap();
        assert_eq!(updates.len(), 2);
        assert!(
            updates
                .iter()
                .all(|s| s.string(1).unwrap() != Some(old.as_str()))
        );
        let original = automerge::Automerge::load(
            s.section("item")
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .bytes(5)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let next = automerge::Automerge::load(
            updates[1]
                .message(38)
                .unwrap()
                .unwrap()
                .bytes(5)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(
            original
                .get_changes(&[])
                .iter()
                .all(|c| next.get_change_by_hash(&c.hash()).is_some())
        );
        assert_eq!(
            next.get_changes(&[]).len(),
            original.get_changes(&[]).len() + 1
        );
        let (_, object) = next.get(automerge::ROOT, "text").unwrap().unwrap();
        let (_, old_object) = original.get(automerge::ROOT, "text").unwrap().unwrap();
        assert_eq!(
            next.text(&object).unwrap(),
            original.text(&old_object).unwrap()
        );
        let marks = next.marks(&object).unwrap();
        assert!(
            !marks
                .iter()
                .any(|m| m.name() == "annotation" && m.value().to_str() == Some(old.as_str()))
        );
        assert!(
            marks
                .iter()
                .any(|m| m.name() == "annotation" && m.value().to_str() == Some(p.control_id()))
        );
        let (control, parent) = received(&p, &s);
        let accepted = p
            .validate_ack(&ack(&[control.clone(), parent.clone()]))
            .unwrap();
        let current = fresh(&s, control, parent);
        assert_eq!(current.section(&old), s.section(&old));
        assert_eq!(current.document, s.document);
        assert!(
            BootstrapIntent::capture(accepted.confirm(&current).unwrap(), 1_720_000_000_002)
                .is_ok()
        );
    }
}

#[test]
fn ordinary_capture_and_old_record_versions_do_not_acquire_archived_removal_authority() {
    let (s, old) = archived_fixture(true);
    assert!(AnnotationIntent::capture_range(&s, "item", 4, 5).is_err());
    let record = archived_prepare(&s, &old).record().unwrap();
    let value = serde_json::to_value(record).unwrap();
    for version in [1, 2, 3] {
        let mut v = value.clone();
        v["version"] = version.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(v)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for key in ["removal", "preserved"] {
        let mut v = value.clone();
        v.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<AnnotationRecord>(v)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn native_removal_subset_refuses_foreign_duplicate_active_and_unremoved_overlap() {
    let (s, old) = archived_fixture(true);
    for ids in [
        vec![],
        vec!["foreign".into()],
        vec![old.clone(), old.clone()],
    ] {
        assert!(AnnotationIntent::capture_range_with_archived(&s, "item", 1, 4, ids).is_err());
    }
    let (active, id) = existing_fixture();
    assert!(
        AnnotationIntent::capture_range_with_archived(&active, "item", 1, 4, vec![id]).is_err()
    );
    // A plain-text common ancestor may remove none. A broader native ancestor
    // may select a disjoint archived wrapper: both explicit subsets are legal.
    for ids in [vec![], vec![old.clone()]] {
        AnnotationIntent::capture_range_with_archived(&s, "item", 4, 5, ids)
            .unwrap()
            .prepare_at(&s, 2_000_000)
            .unwrap()
            .record()
            .unwrap()
            .validate()
            .unwrap();
    }
}

#[test]
fn archived_metadata_precedence_and_zero_count_ambiguity_are_not_guessed() {
    let (mut s, old) = archived_fixture(false);
    // Explicit metadata false overrides the raw Highlight.hidden true.
    let meta = raw(&[t(1, &old), n(2, 1), n(8, 0)]);
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
        .unwrap();
    assert!(
        AnnotationIntent::capture_range_with_archived(&s, "item", 1, 4, vec![old.clone()]).is_err()
    );
    assert!(
        range_seeds(&s)
            .unwrap()
            .iter()
            .all(|(_, p)| p.archived.is_empty())
    );
    let meta = raw(&[t(1, &old), n(2, 0), n(8, 0)]);
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
        .unwrap();
    assert!(!range_seeds(&s).unwrap().iter().any(|(id, _)| id == "item"));
}

#[test]
fn removing_archived_mark_keeps_disjoint_active_mark_and_rejects_active_removal() {
    let (s, old) = existing_fixture();
    let p = AnnotationIntent::capture_range(&s, "item", 4, 5)
        .unwrap()
        .prepare_at(&s, 2_000_000)
        .unwrap();
    let (control, parent) = received(&p, &s);
    let active = control.id().to_owned();
    let mut s = fresh(&s, control, parent);
    let prior = raw(&[t(1, &old), n(2, 1), n(8, 1)]);
    let current = raw(&[t(1, &active), n(2, 1), n(8, 0)]);
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &prior), m(1, &current)]))])
        .unwrap();
    let p = archived_prepare(&s, &old);
    let (control, parent) = received(&p, &s);
    let after = fresh(&s, control, parent);
    assert_eq!(after.document, s.document);
    assert_eq!(after.section(&old), s.section(&old));
    assert_eq!(after.section(&active), s.section(&active));
    let model =
        crate::canvas::edit::annotation::inline::original(after.section("item").unwrap()).unwrap();
    assert!(model.spans.iter().any(|s| s.id == active));
    assert!(!model.spans.iter().any(|s| s.id == old));
    assert!(
        AnnotationIntent::capture_range_with_archived(
            &s,
            "item",
            1,
            4,
            vec![old.clone(), active.clone()]
        )
        .is_err()
    );
    assert!(AnnotationIntent::capture_range_with_archived(&s, "item", 1, 5, vec![old]).is_err());
}

#[test]
fn partial_ack_and_changed_archived_original_refuse_but_keep_actual_evidence() {
    let (s, old) = archived_fixture(true);
    let p = archived_prepare(&s, &old);
    let (control, _) = received(&p, &s);
    assert!(p.validate_ack(&ack(&[control])).is_err());
    let p = archived_prepare(&s, &old);
    let record = p.record().unwrap();
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let evidence = accepted.evidence().unwrap();
    let mut current = fresh(&s, control, parent);
    assert!(accepted.check_confirmation(&current).is_ok());
    let index = current.sections.iter().position(|s| s.id() == old).unwrap();
    current.sections[index] = changed(&current.sections[index], n(2, 999999));
    assert!(accepted.check_confirmation(&current).is_err());
    let review = record.review_current(&current).unwrap();
    assert!(review.matches_current(&current).unwrap());
    assert!(
        record
            .validate_prior_evidence(review.prior_evidence().as_ref())
            .is_ok()
    );
    assert!(evidence.anchor.is_some());
}

#[test]
fn archived_two_section_ack_and_bootstrap_not_sent_survive_journal_restart() {
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let (s, old) = archived_fixture(false);
    let p = archived_prepare(&s, &old);
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

mod hidden;
