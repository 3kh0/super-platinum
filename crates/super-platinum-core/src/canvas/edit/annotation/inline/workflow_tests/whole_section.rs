use super::*;
use automerge::ReadDoc;

fn snapshot(hidden: bool) -> (CanvasCollabSnapshot, String) {
    let (mut snapshot, old) = existing_fixture();
    if hidden {
        let container = snapshot.document.message(16).unwrap().unwrap();
        let meta = container.messages(1).unwrap()[0]
            .replace_fields(8, &[n(8, 1)])
            .unwrap();
        snapshot.document = snapshot
            .document
            .replace_fields(16, &[m(16, &raw(&[m(1, &meta)]))])
            .unwrap();
    }
    (snapshot, old)
}
fn prepare_whole(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
        s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
    )
    .unwrap() else {
        panic!("expected new section root")
    };
    intent.prepare_at(s, 2_000_000).unwrap()
}
fn reply(p: &PreparedAnnotation, s: &CanvasCollabSnapshot) -> Vec<Section> {
    p.document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|update| {
            let incoming = Section::from_raw(update).unwrap();
            let mut section = if let Some(original) = s.section(incoming.id()) {
                original.merge_partial(&incoming).unwrap()
            } else {
                incoming
            };
            let sequence = s
                .section(section.id())
                .and_then(|s| s.sequence().unwrap())
                .unwrap_or(9001)
                + 1000;
            for field in [n(2, sequence), t(6, p.thread_id()), t(7, p.document_id())] {
                section = changed(&section, field);
            }
            if let Some(path) = section.raw().string(21).unwrap() {
                // Clearing an orphaned control retains this exact sentinel;
                // only ordinary interval paths lose their temporary suffix.
                if path != "zzzzzz-orphaned-m" {
                    section = changed(&section, t(21, position::base(path).unwrap()));
                }
            }
            if let Some(history) = section.crdt().unwrap() {
                let history = history
                    .remove(12)
                    .unwrap()
                    .replace_fields(3, &[n(3, sequence)])
                    .unwrap()
                    .replace_fields(4, &[n(4, 30_000_000)])
                    .unwrap();
                section = changed(&section, m(38, &history));
            }
            section
        })
        .collect()
}
fn canonical(s: &CanvasCollabSnapshot, sections: &[Section]) -> CanvasCollabSnapshot {
    let mut current = s.clone();
    for section in sections {
        if let Some(slot) = current.sections.iter_mut().find(|s| s.id() == section.id()) {
            *slot = section.clone();
        } else {
            current.sections.push(section.clone());
        }
    }
    current
}

#[test]
fn whole_section_ordinary_ingress_refuses_visible_inline_and_keeps_its_thread_route() {
    let (s, old) = snapshot(false);
    let exact = s.clone();
    let context = AnnotationPresentation::ordinary(None, None).unwrap();
    assert!(AnnotationIntent::for_section(&s, "item", &context).is_err());
    assert!(
        section_previews(&s, &context)
            .unwrap()
            .iter()
            .all(|p| p.id != "item")
    );
    assert_eq!(s.sections, exact.sections);
    assert_eq!(s.document, exact.document);
    assert_eq!(s.section(&old), exact.section(&old));
    // The existing annotation's original identity remains in the ordinary
    // read projection; this refusal does not remove or archive its thread.
    let original = inline::original(s.section("item").unwrap()).unwrap();
    assert_eq!(original.spans[0].id, old);
}

#[test]
fn whole_section_hidden_unwrap_keeps_text_history_and_never_adds_new_inline_mark() {
    let (s, old) = snapshot(true);
    let p = prepare_whole(&s);
    let updates = p.document_data().messages(1).unwrap();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].string(1).unwrap(), Some("item"));
    assert_eq!(updates[1].string(1).unwrap(), Some(p.control_id()));
    assert!(
        updates
            .iter()
            .all(|u| u.string(1).unwrap() != Some(old.as_str()))
    );
    let old_doc = automerge::Automerge::load(
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
        updates[0]
            .message(38)
            .unwrap()
            .unwrap()
            .bytes(5)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let (_, object) = next.get(automerge::ROOT, "text").unwrap().unwrap();
    let (_, old_object) = old_doc.get(automerge::ROOT, "text").unwrap().unwrap();
    assert_eq!(
        next.text(&object).unwrap(),
        old_doc.text(&old_object).unwrap()
    );
    assert_eq!(
        next.get_changes(&[]).len(),
        old_doc.get_changes(&[]).len() + 1
    );
    assert!(
        old_doc
            .get_changes(&[])
            .iter()
            .all(|c| next.get_change_by_hash(&c.hash()).is_some())
    );
    assert!(
        !next
            .marks(&object)
            .unwrap()
            .iter()
            .any(|mark| mark.name() == "annotation" && mark.value().to_str().is_some())
    );
    let control = updates[1]
        .message(12)
        .unwrap()
        .unwrap()
        .message(7)
        .unwrap()
        .unwrap();
    assert_eq!(strings(&control, 3).unwrap(), ["item"]);
    let record = p.record().unwrap();
    assert!(record.has_parent_update());
    let received = reply(&p, &s);
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let current = canonical(&s, &received);
    assert_eq!(current.section(&old), s.section(&old));
    assert_eq!(current.document, s.document);
    accepted.check_confirmation(&current).unwrap();
    let bootstrap =
        BootstrapIntent::capture(accepted.confirm(&current).unwrap(), 1_720_000_000_002).unwrap();
    bootstrap.record().validate().unwrap();
}

#[test]
fn whole_section_partial_ack_and_metadata_change_do_not_consume_accepted_result() {
    let (s, _) = snapshot(true);
    for omitted in 0..2 {
        let p = prepare_whole(&s);
        let mut received = reply(&p, &s);
        received.remove(omitted);
        assert!(p.validate_ack(&ack(&received)).is_err());
    }
    let p = prepare_whole(&s);
    let received = reply(&p, &s);
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let current = canonical(&s, &received);
    let mut wrong = current.clone();
    wrong.document = wrong.document.remove(16).unwrap();
    assert!(accepted.check_confirmation(&wrong).is_err());
    accepted.check_confirmation(&current).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(evidence.anchor.is_some());
    assert_eq!(serde_json::to_value(evidence.record).unwrap()["version"], 7);
}

#[test]
fn whole_section_record_version_and_all_hidden_set_are_not_forgeable() {
    let (s, _) = snapshot(true);
    let record = prepare_whole(&s).record().unwrap();
    let original = serde_json::to_value(&record).unwrap();
    for version in [1, 2, 3, 4, 5, 6] {
        let mut v = original.clone();
        v["version"] = version.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(v)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for removed in [serde_json::json!([]), serde_json::json!(["foreign"])] {
        let mut v = original.clone();
        v["section_plan"]["body"]["removed"] = removed;
        assert!(
            serde_json::from_value::<AnnotationRecord>(v)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let restored: AnnotationRecord = serde_json::from_value(original).unwrap();
    restored.validate().unwrap();
    assert!(restored == record);
    assert!(AnnotationIntent::capture(&s, "item").is_err());
}

fn with_hidden_section_control(mut s: CanvasCollabSnapshot, old: &str) -> CanvasCollabSnapshot {
    let old = s.section(old).unwrap();
    let highlight = old
        .raw()
        .message(12)
        .unwrap()
        .unwrap()
        .message(7)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[t(3, "item"), t(3, "unloaded-section")])
        .unwrap();
    let control = changed(
        &changed(old, t(1, "section-hidden")),
        m(12, &raw(&[m(7, &highlight)])),
    );
    // An actual complete-load order with this control preceding the anchor.
    s.sections.insert(0, control);
    let container = s.document.message(16).unwrap().unwrap();
    let mut metadata = container.messages(1).unwrap();
    metadata.push(raw(&[t(1, "section-hidden"), n(2, 0)]));
    let fields = metadata.iter().map(|x| m(1, x)).collect::<Vec<_>>();
    s.document = s
        .document
        .replace_fields(16, &[m(16, &raw(&fields))])
        .unwrap();
    s
}

#[test]
fn section_and_inline_cleanup_share_one_exact_ordered_ack_and_two_prior_inventories() {
    let (s, old) = snapshot(true);
    let s = with_hidden_section_control(s, &old);
    let p = prepare_whole(&s);
    let record = p.record().unwrap();
    let ids = p
        .document_data()
        .messages(1)
        .unwrap()
        .iter()
        .map(|v| v.string(1).unwrap().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "section-hidden".to_owned(),
            "item".to_owned(),
            p.control_id().to_owned()
        ]
    );
    let prior = record.review_current(&s).unwrap().prior_evidence().unwrap();
    assert!(prior["section"].is_object());
    assert!(prior["inline"].is_object());
    record.validate_prior_evidence(Some(&prior)).unwrap();
    let mut missing = prior.clone();
    missing["inline"] = serde_json::Value::Null;
    assert!(record.validate_prior_evidence(Some(&missing)).is_err());
    let mut missing = prior.clone();
    missing["section"] = serde_json::Value::Null;
    assert!(record.validate_prior_evidence(Some(&missing)).is_err());
    let received = reply(&p, &s);
    assert_eq!(received[0].id(), "section-hidden");
    assert_eq!(
        received[0].raw().string(21).unwrap(),
        Some("zzzzzz-orphaned-m")
    );
    let negative = prepare_whole(&s);
    let mut malformed = reply(&negative, &s);
    assert_eq!(malformed[0].id(), "section-hidden");
    assert_eq!(
        malformed[0].raw().string(21).unwrap(),
        Some("zzzzzz-orphaned-m")
    );
    assert_eq!(malformed.last().unwrap().id(), negative.control_id());
    malformed[0] = changed(&malformed[0], t(21, "zzzzzz-orphaned-other"));
    assert!(matches!(
        negative.validate_ack(&ack(&malformed)),
        Err(Error::Unsupported)
    ));
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let canonical = canonical(&s, &received);
    assert_eq!(canonical.section(&old), s.section(&old));
    assert_eq!(canonical.document, s.document);
    let proof = accepted.confirm(&canonical).unwrap();
    assert_eq!(proof.changed_controls().len(), 1);
    assert!(proof.anchor().is_some());
    BootstrapIntent::capture(proof, 1_720_000_000_002)
        .unwrap()
        .record()
        .validate()
        .unwrap();
    for omitted in 0..3 {
        let p = prepare_whole(&s);
        let mut received = reply(&p, &s);
        received.remove(omitted);
        assert!(p.validate_ack(&ack(&received)).is_err());
    }
}

struct WholeTemp(std::path::PathBuf);
impl WholeTemp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("canvas-whole-section-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn open(&self) -> Journal {
        Journal::open(
            self.0.join("annotation.sqlite"),
            "43f8ff92-2f24-4aba-8388-2bf9dd8798d6",
        )
        .unwrap()
    }
}
impl Drop for WholeTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("annotation.sqlite"));
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn whole_section_actual_journal_restart_preserves_parent_batch_and_explicit_not_sent_continuation()
{
    let dir = WholeTemp::new();
    let (s, old) = snapshot(true);
    let s = with_hidden_section_control(s, &old);
    let p = prepare_whole(&s);
    let received = reply(&p, &s);
    let mut journal = dir.open();
    let AnnotationBegin::Started(receipt) = journal.begin_annotation(p.record().unwrap()).unwrap()
    else {
        panic!("start")
    };
    let scope = receipt.record().scope().clone();
    drop(journal);
    let mut journal = dir.open();
    let started = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::AnchorStarted);
    assert!(
        journal
            .continue_annotation_bootstrap(&started, &s, 1_720_000_000_002)
            .unwrap()
            .is_none()
    );
    let canonical = canonical(&s, &received);
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(evidence.anchor.is_some());
    assert_eq!(evidence.changed_controls.len(), 1);
    let proof = accepted.confirm(&canonical).unwrap();
    assert!(
        journal
            .annotation_anchor_confirmed(&receipt, &proof)
            .unwrap()
    );
    let exact = serde_json::to_value(journal.annotation(&scope).unwrap().unwrap()).unwrap();
    assert!(exact["canonical_anchor"].is_array());
    assert_eq!(
        exact["canonical_changed_controls"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(journal);
    let mut journal = dir.open();
    assert_eq!(
        serde_json::to_value(journal.annotation(&scope).unwrap().unwrap()).unwrap(),
        exact
    );
    let intent = BootstrapIntent::capture(proof, 1_720_000_000_002).unwrap();
    assert!(
        journal
            .begin_annotation_bootstrap(&receipt, &intent)
            .unwrap()
    );
    let pending = journal.annotation(&scope).unwrap().unwrap();
    assert!(journal.annotation_not_sent(&pending).unwrap());
    drop(journal);
    let mut journal = dir.open();
    let pending = journal.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::BootstrapNotSent);
    let mut changed_read = canonical.clone();
    changed_read.document = changed_read.document.remove(16).unwrap();
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &changed_read, 1_720_000_000_003)
            .is_err()
    );
    assert!(journal.annotation(&scope).unwrap().unwrap() == pending);
    let retry = journal
        .continue_annotation_bootstrap(&pending, &canonical, 1_720_000_000_003)
        .unwrap()
        .unwrap();
    retry.record().validate().unwrap();
    let raw = serde_json::to_value(retry.record()).unwrap();
    assert_eq!(raw["canonical_anchor"], exact["canonical_anchor"]);
    assert_eq!(
        raw["canonical_changed_controls"],
        exact["canonical_changed_controls"]
    );
}

#[test]
fn whole_section_late_actual_ack_keeps_parent_and_both_prior_sets_after_disposition() {
    let dir = WholeTemp::new();
    let (s, old) = snapshot(true);
    let s = with_hidden_section_control(s, &old);
    let p = prepare_whole(&s);
    let received = reply(&p, &s);
    let mut journal = dir.open();
    let AnnotationBegin::Started(receipt) = journal.begin_annotation(p.record().unwrap()).unwrap()
    else {
        panic!("start")
    };
    let pending = journal
        .annotation(receipt.record().scope())
        .unwrap()
        .unwrap();
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(accepted.check_confirmation(&s).is_err());
    assert!(
        journal
            .resolve_acknowledged_annotation_locally(
                &pending,
                receipt.record().review_current(&s).unwrap(),
                evidence.clone(),
                AnnotationDecision::RemoveLocalLockOutcomeUnknown
            )
            .unwrap()
    );
    let AnnotationBegin::Started(next) = journal
        .begin_annotation(prepare_whole(&s).record().unwrap())
        .unwrap()
    else {
        panic!("next")
    };
    let exact = journal.annotation(next.record().scope()).unwrap().unwrap();
    assert!(
        journal
            .preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    drop(journal);
    let mut journal = dir.open();
    assert!(
        journal
            .preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    assert!(journal.annotation(next.record().scope()).unwrap().unwrap() == exact);
    let rows = journal
        .annotation_history(next.record().scope(), None)
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(
            row.observed["acknowledged_anchor"],
            serde_json::json!(evidence.anchor)
        );
        assert_eq!(
            row.observed["acknowledged_changed_controls"],
            serde_json::json!(evidence.changed_controls)
        );
    }
    let reviewed = rows
        .iter()
        .find(|r| r.status == "acknowledged_unconfirmed")
        .unwrap();
    assert_eq!(
        reviewed.observed["prior"]["inline"]["controls"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        reviewed.observed["prior"]["section"]["controls"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn whole_section_styled_parent_keeps_style_and_refuses_a_different_canonical_style() {
    for style in [1, 2, 3, 4, 16, 17, 39, 40, 41, 47] {
        let (mut s, _) = snapshot(true);
        let index = s.sections.iter().position(|v| v.id() == "item").unwrap();
        s.sections[index] = changed(&s.sections[index], n(10, style));
        let p = prepare_whole(&s);
        let received = reply(&p, &s);
        let accepted = p.validate_ack(&ack(&received)).unwrap();
        let current = canonical(&s, &received);
        accepted.check_confirmation(&current).unwrap();
        let mut wrong = current.clone();
        let index = wrong
            .sections
            .iter()
            .position(|v| v.id() == "item")
            .unwrap();
        wrong.sections[index] = changed(&wrong.sections[index], n(10, 0));
        assert!(accepted.check_confirmation(&wrong).is_err());
        accepted.check_confirmation(&current).unwrap();
    }
}

#[test]
fn whole_section_hidden_default_content_requires_separate_template_edit_authority() {
    for default_content in [1, 2, 3, 99] {
        let (mut s, _) = snapshot(true);
        let index = s.sections.iter().position(|v| v.id() == "item").unwrap();
        let attributes = s.sections[index]
            .raw()
            .message(16)
            .unwrap()
            .unwrap_or_else(|| raw(&[]))
            .replace_fields(13, &[n(13, default_content)])
            .unwrap();
        s.sections[index] = changed(&s.sections[index], m(16, &attributes));
        let original = s.clone();
        assert!(
            AnnotationIntent::for_section(
                &s,
                "item",
                &AnnotationPresentation::ordinary(None, None).unwrap()
            )
            .is_err()
        );
        assert_eq!(s.sections, original.sections);
        assert_eq!(s.document, original.document);
    }
    let (mut s, _) = snapshot(true);
    let index = s.sections.iter().position(|v| v.id() == "item").unwrap();
    s.sections[index] = changed(&s.sections[index], m(16, &raw(&[n(13, 0)])));
    assert!(prepare_whole(&s).record().unwrap().has_parent_update());
}
