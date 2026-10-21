//! Synthetic merged canonical responses; no server heading ACK is claimed.
use super::*;

fn heading(level: u64, positive: bool) -> CanvasCollabSnapshot {
    let mut s = fixture();
    let index = s.sections.iter().position(|v| v.id() == "item").unwrap();
    let mut section = changed(&s.sections[index], n(10, level));
    section = changed(&section, t(177, "opaque heading custody"));
    section = changed(&section, m(16, &raw(&[t(99, "ordinary attributes")])));
    section = changed(
        &section,
        m(
            12,
            &raw(&[m(
                1,
                &raw(&[t(
                    1,
                    "<u><b>A&amp;\u{1f642}B</b></u><br><i>\u{4e2d} tail</i>",
                )]),
            )]),
        ),
    );
    if positive {
        let draft = crate::canvas::edit::text::canonical_draft(&section).unwrap();
        let sequence = section.sequence().unwrap().unwrap();
        let wire = crate::canvas::edit::text::crdt::bootstrap(&section, &draft, "0123456789ab", 0)
            .unwrap()
            .replace_fields(2, &[n(2, sequence)])
            .unwrap()
            .replace_fields(3, &[n(3, sequence)])
            .unwrap()
            .replace_fields(4, &[n(4, 30_000_000)])
            .unwrap();
        section = changed(&section, m(38, &wire));
    }
    s.sections[index] = section.clone();
    // Same text does not establish target identity.
    let twin = changed(&changed(&section, t(1, "heading-twin")), t(8, "AAAZ"));
    s.sections.push(twin);
    s
}

fn replace(s: &mut CanvasCollabSnapshot, section: Section) {
    let id = section.id().to_owned();
    *s.sections.iter_mut().find(|v| v.id() == id).unwrap() = section;
}

#[test]
fn h1_h2_h3_inline_roundtrip_preserves_level_structure_and_original_siblings() {
    for level in 1..=3 {
        for positive in [false, true] {
            let s = heading(level, positive);
            let before = s.sections.clone();
            let seed = range_seeds(&s)
                .unwrap()
                .into_iter()
                .find(|(id, _)| id == "item")
                .unwrap()
                .1;
            assert_eq!(
                seed.draft
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>(),
                "A&\u{1f642}B\n\u{4e2d} tail"
            );
            assert!(
                seed.runs
                    .iter()
                    .any(|run| run.style.underline && run.style.bold)
            );
            assert!(
                paragraph_seeds(&s)
                    .unwrap()
                    .iter()
                    .all(|(id, _)| id != "item")
            );
            assert!(AnnotationIntent::capture(&s, "item").is_err());
            let p = prepare(&s);
            let updates = p.document_data().messages(1).unwrap();
            assert_eq!(updates.len(), 2);
            for tag in [3, 8, 10, 11, 16, 29, 177] {
                assert!(!updates[1].has(tag));
            }
            let record = p.record().unwrap();
            assert_eq!(record.snippet()["mrkdwn"], "&\u{1f642}");
            let (control, parent) = received(&p, &s);
            let original = s.section("item").unwrap();
            assert_eq!(parent.style().unwrap(), Some(level));
            for tag in [8, 177] {
                assert_eq!(
                    parent.raw().string(tag).unwrap(),
                    original.raw().string(tag).unwrap()
                );
            }
            assert_eq!(parent.attributes().unwrap(), original.attributes().unwrap());
            let accepted = p
                .validate_ack(&ack(&[control.clone(), parent.clone()]))
                .unwrap();
            let current = fresh(&s, control, parent);
            accepted.check_confirmation(&current).unwrap();
            for prior in before.iter().filter(|v| v.id() != "item") {
                assert_eq!(current.section(prior.id()), Some(prior));
            }
            assert_eq!(s.sections, before);
            accepted.confirm(&current).unwrap();
        }
    }
}

#[test]
fn heading_ack_and_later_readback_refuse_level_position_attribute_and_opaque_drift() {
    for positive in [false, true] {
        let s = heading(2, positive);
        for field in [
            n(10, 1),
            t(8, "AAAY"),
            m(16, &raw(&[t(99, "changed")])),
            t(177, "changed"),
        ] {
            let p = prepare(&s);
            let (control, parent) = received(&p, &s);
            let bad = changed(&parent, field.clone());
            assert!(p.validate_ack(&ack(&[control, bad])).is_err());
            let p = prepare(&s);
            let (control, parent) = received(&p, &s);
            let accepted = p
                .validate_ack(&ack(&[control.clone(), parent.clone()]))
                .unwrap();
            let mut current = fresh(&s, control, parent.clone());
            let sequence = parent.sequence().unwrap().unwrap() + 1;
            let wire = parent
                .crdt()
                .unwrap()
                .unwrap()
                .replace_fields(3, &[n(3, sequence)])
                .unwrap();
            let later = changed(&changed(&parent, n(2, sequence)), m(38, &wire));
            replace(&mut current, later.clone());
            accepted.check_confirmation(&current).unwrap();
            replace(&mut current, changed(&later, field));
            assert!(accepted.check_confirmation(&current).is_err());
        }
    }
}

#[test]
fn heading_range_permission_shape_and_surrogate_boundaries_remain_closed() {
    let s = heading(1, false);
    for (a, b) in [(2, 3), (3, 4), (1, 1), (0, 99)] {
        assert!(AnnotationIntent::capture_range(&s, "item", a, b).is_err());
    }
    for field in [
        n(10, 4),
        n(10, 48),
        n(9, 1),
        n(29, 1),
        m(13, &raw(&[t(1, "nested")])),
        n(11, 1),
        n(3, 1),
    ] {
        let mut bad = s.clone();
        replace(&mut bad, changed(s.section("item").unwrap(), field));
        assert!(AnnotationIntent::capture_range(&bad, "item", 1, 4).is_err());
        assert!(
            range_seeds(&bad)
                .unwrap()
                .iter()
                .all(|(id, _)| id != "item")
        );
    }
    let mut bad = s.clone();
    bad.file
        .extra
        .insert("access".into(), serde_json::json!("read"));
    assert!(AnnotationIntent::capture_range(&bad, "item", 1, 4).is_err());
    assert!(range_seeds(&bad).is_err());
    bad = s.clone();
    bad.thread = bad.thread.replace_fields(23, &[n(23, 13)]).unwrap();
    assert!(AnnotationIntent::capture_range(&bad, "item", 1, 4).is_err());
}

#[test]
fn heading_capture_cannot_prepare_after_level_or_sibling_change() {
    let s = heading(3, false);
    for id in ["item", "heading-twin"] {
        let intent = AnnotationIntent::capture_range(&s, "item", 1, 4).unwrap();
        let mut bad = s.clone();
        replace(&mut bad, changed(s.section(id).unwrap(), n(10, 1)));
        assert!(intent.prepare_at(&bad, 1_000_000).is_err());
    }
}

#[test]
fn heading_existing_control_routes_without_duplicate_and_disjoint_range_keeps_custody() {
    for level in 1..=3 {
        let (mut s, old) = existing_fixture();
        let parent = changed(s.section("item").unwrap(), n(10, level));
        replace(&mut s, parent);
        let original_control = s.section(&old).unwrap().clone();
        let seed = range_seeds(&s)
            .unwrap()
            .into_iter()
            .find(|(id, _)| id == "item")
            .unwrap()
            .1;
        assert_eq!(seed.existing_control(1, 4).unwrap(), Some(old.as_str()));
        assert!(AnnotationIntent::capture_range(&s, "item", 1, 4).is_err());
        let p = AnnotationIntent::capture_range(&s, "item", 4, 5)
            .unwrap()
            .prepare_at(&s, 2_000_000)
            .unwrap();
        let (control, parent) = received(&p, &s);
        let accepted = p
            .validate_ack(&ack(&[control.clone(), parent.clone()]))
            .unwrap();
        let current = fresh(&s, control, parent);
        assert_eq!(current.section(&old), Some(&original_control));
        accepted.check_confirmation(&current).unwrap();
    }
}

#[test]
fn heading_malformed_content_id_is_not_an_ordinary_content_update() {
    for field in [n(29, 1)] {
        let mut s = heading(1, false);
        let parent = changed(s.section("item").unwrap(), m(16, &raw(&[field])));
        replace(&mut s, parent);
        assert!(AnnotationIntent::capture_range(&s, "item", 1, 4).is_err());
        assert!(range_seeds(&s).unwrap().iter().all(|(id, _)| id != "item"));
    }
    let mut s = heading(1, false);
    let parent = changed(s.section("item").unwrap(), m(16, &raw(&[t(29, "")])));
    replace(&mut s, parent);
    assert!(AnnotationIntent::capture_range(&s, "item", 1, 4).is_ok());
}

#[test]
fn heading_ack_journal_preserves_inert_original_and_requires_canonical_readback() {
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let s = heading(3, true);
    let p = prepare(&s);
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
    journal
        .preserve_annotation_acknowledgement(&pending, &accepted.evidence().unwrap())
        .unwrap();
    drop(journal);
    let mut journal = Journal::open(&path, owner).unwrap();
    assert_eq!(
        journal.annotation_history(&scope, None).unwrap()[0].observed["acknowledged_anchor"],
        serde_json::json!(parent.raw().raw())
    );
    assert!(
        journal
            .continue_annotation_bootstrap(&pending, &s, 1_700_000_000_001)
            .unwrap()
            .is_none()
    );
    let verified = accepted.confirm(&fresh(&s, control, parent)).unwrap();
    assert!(
        journal
            .annotation_anchor_confirmed(&receipt, &verified)
            .unwrap()
    );
}

mod content_id;
