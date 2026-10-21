//! Authored actual core/temporary-journal regressions. Root owns execution.
use super::super::*;
use crate::canvas::comments::bootstrap::BootstrapIntent;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};
use crate::canvas::wire::EditAck;

fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
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
fn changed(section: &Section, field: F) -> Section {
    Section::from_raw(
        section
            .raw()
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn fixture() -> CanvasCollabSnapshot {
    let mut snapshot = crate::canvas::edit::annotation::tests::fixture();
    let index = snapshot
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    let old = &snapshot.sections[index];
    let content = old.raw().message(12).unwrap().unwrap();
    let text = content
        .message(1)
        .unwrap()
        .unwrap()
        .replace_fields(1, &[t(1, "<b>A&amp;\u{1f600}B</b>")])
        .unwrap();
    let section = changed(
        old,
        m(12, &content.replace_fields(1, &[m(1, &text)]).unwrap()),
    );
    snapshot.sections[index] = Section::from_raw(section.raw().remove(38).unwrap()).unwrap();
    snapshot
}
fn prepare(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
    AnnotationIntent::capture_range(s, "item", 1, 4)
        .unwrap()
        .prepare_at(s, 1_000_000)
        .unwrap()
}
fn received(p: &PreparedAnnotation, s: &CanvasCollabSnapshot) -> (Section, Section) {
    let updates = p.document_data().messages(1).unwrap();
    let mut control = Section::from_raw(updates[0].clone()).unwrap();
    for field in [
        n(2, 10001),
        t(6, p.thread_id()),
        t(7, p.document_id()),
        t(21, position::base(&p.intent.path).unwrap()),
    ] {
        control = changed(&control, field);
    }
    let original = s.section("item").unwrap();
    let mut parent = original
        .merge_partial(&Section::from_raw(updates[1].clone()).unwrap())
        .unwrap();
    let sequence = original.sequence().unwrap().unwrap() + 1000;
    let wire = parent
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 30_000_000)])
        .unwrap();
    parent = changed(&changed(&parent, n(2, sequence)), m(38, &wire));
    (control, parent)
}
fn ack(sections: &[Section]) -> EditAck {
    let fields = sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>();
    EditAck::decode(raw(&[n(1, 1), m(2, &raw(&fields))]).raw()).unwrap()
}
fn fresh(s: &CanvasCollabSnapshot, control: Section, parent: Section) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let destination = s
        .sections
        .iter_mut()
        .find(|s| s.id() == parent.id())
        .unwrap();
    *destination = parent;
    s.sections.push(control);
    s
}

#[test]
fn inline_record_is_versioned_and_preserves_exact_legacy_paragraph_shape() {
    let s = fixture();
    let legacy = AnnotationIntent::capture(&s, "item")
        .unwrap()
        .prepare(&s)
        .unwrap()
        .record()
        .unwrap();
    let old = serde_json::to_value(&legacy).unwrap();
    assert_eq!(old["version"], 1);
    assert!(old.get("inline").is_none());
    let p = prepare(&s);
    let record = p.record().unwrap();
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["inline"], serde_json::json!({"start":1,"end":4}));
    assert!(record == serde_json::from_value(value.clone()).unwrap());
    assert_eq!(record.snippet()["mrkdwn"], "&\u{1f600}");
    // The parent bold is outside the actual annotation subtree.
    assert!(
        !record.snippet()["richText"]
            .as_str()
            .unwrap()
            .contains("bold")
    );
    for edit in 0..3 {
        let mut forged = value.clone();
        match edit {
            0 => forged["version"] = 1.into(),
            1 => forged["inline"]["end"] = 3.into(),
            _ => forged["snippet"]["mrkdwn"] = "changed".into(),
        }
        assert!(
            serde_json::from_value::<AnnotationRecord>(forged)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert!(
        AnnotationIntent::capture_range(&s, "item", 1, 4)
            .unwrap()
            .prepare(&s)
            .is_err()
    );
}

#[test]
fn exact_two_section_ack_requires_parent_history_and_never_accepts_partial_control_success() {
    let s = fixture();
    for mode in 0..6 {
        let p = prepare(&s);
        let (control, parent) = received(&p, &s);
        let sections = match mode {
            0 => vec![control],
            1 => vec![control.clone(), control],
            2 => vec![
                control,
                Section::from_raw(parent.raw().remove(38).unwrap()).unwrap(),
            ],
            3 => vec![control, changed(&parent, n(20, 2))],
            4 => vec![
                control,
                changed(
                    &parent,
                    m(
                        12,
                        &s.section("item")
                            .unwrap()
                            .raw()
                            .message(12)
                            .unwrap()
                            .unwrap(),
                    ),
                ),
            ],
            _ => vec![control, changed(&parent, t(7, "other-document"))],
        };
        if mode == 1 {
            let fields = sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>();
            assert!(matches!(
                EditAck::decode(raw(&[n(1, 1), m(2, &raw(&fields))]).raw()),
                Err(crate::canvas::wire::Error::Duplicate)
            ));
        } else {
            assert!(p.validate_ack(&ack(&sections)).is_err());
        }
    }
    let p = prepare(&s);
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[parent.clone(), control.clone()]))
        .unwrap();
    assert!(accepted.check_confirmation(&s).is_err());
    let canonical = fresh(&s, control, parent);
    assert!(accepted.check_confirmation(&canonical).is_ok());
    let mut wrong = canonical.clone();
    *wrong
        .sections
        .iter_mut()
        .find(|s| s.id() == "item")
        .unwrap() = s.section("item").unwrap().clone();
    assert!(accepted.check_confirmation(&wrong).is_err());
    assert!(
        accepted.check_confirmation(&canonical).is_ok(),
        "failed reads retain the original two-section ACK"
    );
}

#[test]
fn inline_ack_evidence_retains_both_sections_and_canonical_bootstrap_binds_exact_parent() {
    let s = fixture();
    let p = prepare(&s);
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let evidence = accepted.evidence().unwrap();
    assert_eq!(evidence.anchor.as_deref(), Some(parent.raw().raw()));
    assert!(
        evidence
            .record
            .check_acknowledged_bytes(&evidence.section, None)
            .is_err()
    );
    let canonical = fresh(&s, control, parent.clone());
    let proof = accepted.confirm(&canonical).unwrap();
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert_eq!(intent.record().canonical_anchor(), Some(parent.raw().raw()));
    let value = serde_json::to_value(intent.record()).unwrap();
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("canonical_anchor");
    assert!(
        serde_json::from_value::<crate::canvas::comments::bootstrap::BootstrapRecord>(missing)
            .unwrap()
            .validate()
            .is_err()
    );
}

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("canvas-inline-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("annotation.sqlite"));
        let _ = std::fs::remove_dir(&self.0);
    }
}
#[test]
fn two_section_ack_and_confirmed_parent_survive_restart_without_replaying_any_request() {
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let mut journal = Journal::open(&path, owner).unwrap();
    let s = fixture();
    let p = prepare(&s);
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
    let evidence = accepted.evidence().unwrap();
    assert!(
        journal
            .preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    assert!(
        journal
            .preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    let history = journal.annotation_history(&scope, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].observed["acknowledged_anchor"],
        serde_json::json!(parent.raw().raw())
    );
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &s, 1_700_000_000_001)
            .unwrap()
            .is_none()
    );
    let canonical = fresh(&s, control, parent);
    let verified = accepted.confirm(&canonical).unwrap();
    assert!(
        journal
            .annotation_anchor_confirmed(&receipt, &verified)
            .unwrap()
    );
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    let pending = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::AnchorConfirmed);
    let mut replaced = canonical.clone();
    let parent = replaced
        .sections
        .iter_mut()
        .find(|s| s.id() == "item")
        .unwrap();
    *parent = changed(parent, n(2, parent.sequence().unwrap().unwrap() + 1));
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &replaced, 1_700_000_000_001)
            .is_err()
    );
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &canonical, 1_700_000_000_001)
            .unwrap()
            .is_some()
    );
    let started = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::BootstrapStarted);
    assert!(
        journal
            .continue_annotation_bootstrap(&started, &canonical, 1_700_000_000_002)
            .unwrap()
            .is_none()
    );
}

#[test]
fn inline_history_expiry_is_rechecked_at_dispatch_without_relaxing_owner_identity() {
    let mut s = fixture();
    let original = s.section("item").unwrap();
    let draft = crate::canvas::edit::text::canonical_draft(original).unwrap();
    let seed =
        crate::canvas::edit::text::crdt::bootstrap(original, &draft, "111111111111", 500_000)
            .unwrap()
            .replace_fields(4, &[n(4, 2_000_000)])
            .unwrap();
    let index = s.sections.iter().position(|s| s.id() == "item").unwrap();
    s.sections[index] = changed(original, m(38, &seed));
    let prepared = prepare(&s);
    assert!(prepared.validate_snapshot(&s, 1_999_999).is_ok());
    assert!(matches!(
        prepared.validate_snapshot(&s, 2_000_000),
        Err(Error::Expired)
    ));
    let mut changed_owner = s.clone();
    changed_owner.sections[index] = changed(&changed_owner.sections[index], n(2, 9_999));
    assert!(
        prepared
            .validate_snapshot(&changed_owner, 1_999_999)
            .is_err()
    );
}

fn existing_fixture() -> (CanvasCollabSnapshot, String) {
    let s = fixture();
    let p = prepare(&s);
    let (control, parent) = received(&p, &s);
    let old = control.id().to_owned();
    let mut s = fresh(&s, control, parent);
    let metadata = raw(&[
        t(1, &old),
        n(2, 1),
        n(4, 1),
        n(6, 0),
        n(7, 1_000_000),
        n(8, 0),
    ]);
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&[m(1, &metadata)]))])
        .unwrap();
    (s, old)
}
#[test]
fn disjoint_second_annotation_keeps_actual_existing_history_control_and_metadata() {
    use automerge::ReadDoc;
    let (s, old) = existing_fixture();
    let old_control = s.section(&old).unwrap().clone();
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
    let p = AnnotationIntent::capture_range(&s, "item", 4, 5)
        .unwrap()
        .prepare_at(&s, 2_000_000)
        .unwrap();
    let record = p.record().unwrap();
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["version"], 3);
    assert!(value["preserved"].is_object());
    let restored: AnnotationRecord = serde_json::from_value(value).unwrap();
    restored.validate().unwrap();
    assert!(restored == record);
    let updates = p.document_data().messages(1).unwrap();
    assert_eq!(updates.len(), 2);
    assert!(
        updates
            .iter()
            .all(|u| u.string(1).unwrap() != Some(old.as_str()))
    );
    let sent = updates[1].message(38).unwrap().unwrap();
    let next = automerge::Automerge::load(sent.bytes(5).unwrap().unwrap()).unwrap();
    assert!(
        original
            .get_changes(&[])
            .iter()
            .all(|c| next.get_change_by_hash(&c.hash()).is_some())
    );
    let (_, text) = next.get(automerge::ROOT, "text").unwrap().unwrap();
    let marks = next.marks(&text).unwrap();
    assert_eq!(marks.iter().filter(|m| m.name() == "annotation").count(), 2);
    let (control, parent) = received(&p, &s);
    let proof = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let current = fresh(&s, control, parent);
    assert_eq!(current.section(&old), Some(&old_control));
    assert!(proof.check_confirmation(&current).is_ok());
    let confirmed = proof.confirm(&current).unwrap();
    assert!(BootstrapIntent::capture(confirmed, 1_720_000_000_002).is_ok());
}
#[test]
fn existing_annotation_overlap_hidden_metadata_and_missing_history_cannot_admit_write() {
    let (s, old) = existing_fixture();
    for (a, b) in [(1, 2), (0, 3), (0, 5)] {
        assert!(AnnotationIntent::capture_range(&s, "item", a, b).is_err());
    }
    let nohistory = changed(s.section("item").unwrap(), m(38, &raw(&[])));
    let mut nohistory_snapshot = s.clone();
    *nohistory_snapshot
        .sections
        .iter_mut()
        .find(|v| v.id() == "item")
        .unwrap() = nohistory;
    assert!(
        AnnotationIntent::capture_range(&nohistory_snapshot, "item", 4, 5)
            .unwrap()
            .prepare_at(&nohistory_snapshot, 2_000_000)
            .is_err()
    );
    for meta in [
        raw(&[t(1, &old), n(2, 0), n(8, 0)]),
        raw(&[t(1, &old), n(2, 1), n(8, 1)]),
    ] {
        let mut changed = s.clone();
        changed.document = changed
            .document
            .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
            .unwrap();
        assert!(AnnotationIntent::capture_range(&changed, "item", 4, 5).is_err());
    }
}
#[test]
fn second_annotation_inert_record_and_accepted_proof_refuse_changed_prior_control() {
    let (s, old) = existing_fixture();
    let p = AnnotationIntent::capture_range(&s, "item", 4, 5)
        .unwrap()
        .prepare_at(&s, 2_000_000)
        .unwrap();
    let record = p.record().unwrap();
    let mut value = serde_json::to_value(&record).unwrap();
    value.as_object_mut().unwrap().remove("preserved");
    assert!(
        serde_json::from_value::<AnnotationRecord>(value)
            .unwrap()
            .validate()
            .is_err()
    );
    let (control, parent) = received(&p, &s);
    let proof = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let mut current = fresh(&s, control, parent);
    let observed = record.review_current(&current).unwrap();
    let index = current.sections.iter().position(|v| v.id() == old).unwrap();
    current.sections[index] = changed(&current.sections[index], n(2, 999999));
    assert!(proof.check_confirmation(&current).is_err());
    assert!(!observed.matches_current(&current).unwrap());
    assert!(
        record
            .review_current(&current)
            .unwrap()
            .matches_current(&current)
            .unwrap()
    );
    // The still-held direct proof and original record are neither consumed nor
    // rebound to this changed control by the failed read.
    assert!(record.validate().is_ok());
}

#[test]
fn changed_prior_control_review_can_release_only_local_lock_and_survives_restart() {
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let (s, old) = existing_fixture();
    let p = AnnotationIntent::capture_range(&s, "item", 4, 5)
        .unwrap()
        .prepare_at(&s, 2_000_000)
        .unwrap();
    let record = p.record().unwrap();
    let mut journal = Journal::open(&path, owner).unwrap();
    let AnnotationBegin::Started(receipt) = journal.begin_annotation(record.clone()).unwrap()
    else {
        panic!("started")
    };
    let scope = receipt.record().scope().clone();
    let pending = journal.annotation(&scope).unwrap().unwrap();
    let (control, parent) = received(&p, &s);
    let accepted = p
        .validate_ack(&ack(&[control.clone(), parent.clone()]))
        .unwrap();
    let evidence = accepted.evidence().unwrap();
    let mut current = fresh(&s, control, parent);
    let index = current.sections.iter().position(|s| s.id() == old).unwrap();
    current.sections[index] = changed(&current.sections[index], n(2, 999999));
    assert!(accepted.check_confirmation(&current).is_err());
    let review = record.review_current(&current).unwrap();
    assert!(review.matches_current(&current).unwrap());
    let prior = review.prior_evidence().unwrap();
    assert!(record.validate_prior_evidence(Some(&prior)).is_ok());
    assert!(record.validate_prior_evidence(None).is_err());
    assert!(
        journal
            .resolve_acknowledged_annotation_locally(
                &pending,
                review,
                evidence,
                AnnotationDecision::RemoveLocalLockOutcomeUnknown
            )
            .unwrap()
    );
    drop(journal);
    let journal = Journal::open(&path, owner).unwrap();
    assert!(journal.annotation(&scope).unwrap().is_none());
    let history = journal.annotation_history(&scope, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, "acknowledged_unconfirmed");
    assert_eq!(history[0].observed["prior"], prior);
    assert!(history[0].observed.get("acknowledged_anchor").is_some());
}

mod archived;

mod whole_section;

mod underline;

mod headings;
