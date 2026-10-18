//! Actual annotation prepare/ACK/readback paths, with canonical modern table PB.
use super::*;

fn table_fixture() -> CanvasCollabSnapshot {
    in_table(fixture())
}

fn in_table(mut s: CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    let i = s.sections.iter().position(|v| v.id() == "item").unwrap();
    s.sections[i] = changed(&s.sections[i], n(29, 1));
    let layout = raw(&[
        m(1, &raw(&[t(1, "row:one"), t(2, "aaa")])),
        m(2, &raw(&[t(1, "col:one"), t(2, "aaa")])),
        m(
            3,
            &raw(&[
                t(1, "cell:one"),
                t(2, "row:one"),
                t(3, "col:one"),
                t(4, "item"),
            ]),
        ),
        t(99, "untouched layout metadata"),
    ]);
    s.sections.push(
        Section::from_raw(raw(&[
            t(1, "table"),
            n(2, 71),
            t(6, s.thread_id()),
            t(7, s.document_id()),
            t(8, "aab"),
            n(9, 33),
            n(10, 28),
            m(12, &raw(&[m(30, &layout)])),
        ]))
        .unwrap(),
    );
    s
}

fn table_prepare(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
        s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
    )
    .unwrap() else {
        panic!("new child annotation")
    };
    intent.prepare_at(s, 2_000_000).unwrap()
}

// Actual first range preparation creates the retained Automerge history before
// the child belongs to a table. Test responses model only the server sequence
// and history acknowledgement, never replace the production history validator.
fn canonical_responses(p: &PreparedAnnotation, s: &CanvasCollabSnapshot) -> Vec<Section> {
    responses(p, s)
        .into_iter()
        .map(|mut value| {
            let sequence = s
                .section(value.id())
                .and_then(|r| r.sequence().unwrap())
                .unwrap_or(9001)
                + 1000;
            value = changed(&value, n(2, sequence));
            if let Some(history) = value.crdt().unwrap() {
                let history = history
                    .remove(12)
                    .unwrap()
                    .replace_fields(3, &[n(3, value.sequence().unwrap().unwrap())])
                    .unwrap()
                    .replace_fields(4, &[n(4, 30_000_000)])
                    .unwrap();
                value = changed(&value, m(38, &history));
            }
            value
        })
        .collect()
}

fn table_body_fixture() -> (CanvasCollabSnapshot, String) {
    let s = super::super::tests::fixture();
    let p = AnnotationIntent::capture_range(&s, "item", 0, 1)
        .unwrap()
        .prepare_at(&s, 1_000_000)
        .unwrap();
    let old = p.control_id().to_owned();
    let rows = canonical_responses(&p, &s);
    let mut s = fresh(&s, &rows);
    p.validate_ack(&ack(&rows)).unwrap().confirm(&s).unwrap();
    s.document = s
        .document
        .replace_fields(
            16,
            &[m(16, &raw(&[m(1, &raw(&[t(1, &old), n(2, 4), n(8, 1)]))]))],
        )
        .unwrap();
    add(&mut s, "z-stub", false, 0, false);
    add(&mut s, "a-archived", true, 4, false);
    add(&mut s, "visible-positioned", false, 2, true);
    (in_table(s), old)
}

#[test]
fn modern_table_child_uses_layout_siblings_and_keeps_the_table_non_authorable() {
    let s = table_fixture();
    let p = table_prepare(&s);
    assert_eq!(p.intent.path, position::between(None, Some("aac")).unwrap());
    assert_ne!(
        p.intent.path,
        position::between(Some("aab"), Some("aac")).unwrap()
    );
    let record = p.record().unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap()["version"], 10);
    assert_eq!(record.anchor().unwrap(), *s.section("item").unwrap());
    let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
    assert!(AnnotationIntent::for_section(&s, "table", &presentation).is_err());
    // Only the explicitly table-owned whole-section path is extended.
    assert!(AnnotationIntent::capture(&s, "item").is_err());
    assert!(AnnotationIntent::capture_range(&s, "item", 0, 1).is_err());
    let preview = section_previews(&s, &presentation).unwrap();
    assert!(preview.iter().any(|p| p.id == "item"));
    assert!(!preview.iter().any(|p| p.id == "table"));
    for version in 1..=9 {
        let mut demoted = serde_json::to_value(&record).unwrap();
        demoted["version"] = serde_json::json!(version);
        let decoded: AnnotationRecord = serde_json::from_value(demoted).unwrap();
        assert!(decoded.validate().is_err());
    }
    let mut no_table = serde_json::to_value(record).unwrap();
    no_table["section_plan"]["table_layout"] = serde_json::json!(false);
    assert!(
        serde_json::from_value::<AnnotationRecord>(no_table)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn actual_table_ack_never_substitutes_for_fresh_exact_table_custody() {
    let s = table_fixture();
    let p = table_prepare(&s);
    let rows = responses(&p, &s);
    let canonical = fresh(&s, &rows);
    // Original table bytes remain outside the complete control update family.
    assert!(
        p.document_data()
            .messages(1)
            .unwrap()
            .iter()
            .all(|r| r.string(1).unwrap() != Some("table"))
    );
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.confirm(&canonical).unwrap();
    for field in [
        n(2, 72),
        n(35, 1),
        n(3, 1),
        t(99, "changed parent metadata"),
    ] {
        let p = table_prepare(&s);
        let rows = responses(&p, &s);
        let mut changed_snapshot = fresh(&s, &rows);
        let i = changed_snapshot
            .sections
            .iter()
            .position(|v| v.id() == "table")
            .unwrap();
        changed_snapshot.sections[i] = changed(&changed_snapshot.sections[i], field);
        assert!(
            p.validate_ack(&ack(&rows))
                .unwrap()
                .confirm(&changed_snapshot)
                .is_err()
        );
    }
    let p = table_prepare(&s);
    let rows = responses(&p, &s);
    let mut missing = fresh(&s, &rows);
    missing.sections.retain(|v| v.id() != "table");
    assert!(
        p.validate_ack(&ack(&rows))
            .unwrap()
            .confirm(&missing)
            .is_err()
    );
}

#[test]
fn actual_table_child_intent_rejects_reparenting_before_dispatch() {
    let s = table_fixture();
    let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
    let SectionAnnotationChoice::New(intent) =
        AnnotationIntent::for_section(&s, "item", &presentation).unwrap()
    else {
        panic!("new")
    };
    let mut changed_snapshot = s.clone();
    let i = changed_snapshot
        .sections
        .iter()
        .position(|v| v.id() == "table")
        .unwrap();
    changed_snapshot.sections[i] = changed(&changed_snapshot.sections[i], n(2, 72));
    assert!(intent.prepare(&changed_snapshot).is_err());
    let mut flag_only = s.clone();
    flag_only.sections.retain(|v| v.id() != "table");
    assert!(AnnotationIntent::for_section(&flag_only, "item", &presentation).is_err());
}

#[test]
fn table_whole_body_cleanup_preserves_history_and_requires_layout_flag_in_every_ack() {
    use automerge::ReadDoc;
    let (s, old) = table_body_fixture();
    let p = table_prepare(&s);
    let updates = p.document_data().messages(1).unwrap();
    assert_eq!(
        updates
            .iter()
            .map(|r| r.string(1).unwrap().unwrap())
            .collect::<Vec<_>>(),
        ["item", "z-stub", "a-archived", p.control_id()]
    );
    assert!(p.record().unwrap().has_parent_update());
    let before = automerge::Automerge::load(
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
    let after = automerge::Automerge::load(
        updates[0]
            .message(38)
            .unwrap()
            .unwrap()
            .bytes(5)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let (_, before_text) = before.get(automerge::ROOT, "text").unwrap().unwrap();
    let (_, after_text) = after.get(automerge::ROOT, "text").unwrap().unwrap();
    assert_eq!(
        before.text(&before_text).unwrap(),
        after.text(&after_text).unwrap()
    );
    assert!(
        before
            .get_changes(&[])
            .iter()
            .all(|c| after.get_change_by_hash(&c.hash()).is_some())
    );
    assert!(
        !after
            .marks(&after_text)
            .unwrap()
            .iter()
            .any(|m| m.name() == "annotation" && m.value().to_str().is_some())
    );
    let rows = canonical_responses(&p, &s);
    let canonical = fresh(&s, &rows);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    assert_eq!(canonical.section(&old), s.section(&old));
    assert_eq!(canonical.section("table"), s.section("table"));
    assert_eq!(canonical.document, s.document);
    for flag in [None, Some(false)] {
        let p = table_prepare(&s);
        let mut rows = canonical_responses(&p, &s);
        let at = rows.iter().position(|r| r.id() == "item").unwrap();
        rows[at] = if flag.is_none() {
            Section::from_raw(rows[at].raw().remove(29).unwrap()).unwrap()
        } else {
            changed(&rows[at], n(29, 0))
        };
        assert!(p.validate_ack(&ack(&rows)).is_err());
    }
    let p = table_prepare(&s);
    let rows = canonical_responses(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let mut moved = fresh(&s, &rows);
    let at = moved
        .sections
        .iter()
        .position(|r| r.id() == "item")
        .unwrap();
    moved.sections[at] = changed(&moved.sections[at], n(29, 0));
    assert!(accepted.check_confirmation(&moved).is_err());
}

#[path = "table_journal_tests.rs"]
mod journal;

#[path = "table_bodies_tests.rs"]
mod bodies;
