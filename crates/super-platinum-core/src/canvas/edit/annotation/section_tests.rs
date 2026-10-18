//! Actual multi-control prepare/ACK/readback and journal transitions. No network.
use super::*;
use crate::canvas::wire::EditAck;

fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn t(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
fn raw(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
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
fn add(
    snapshot: &mut CanvasCollabSnapshot,
    id: &str,
    archived: bool,
    count: u64,
    positioned: bool,
) {
    let mut h = vec![
        t(1, "original-creator"),
        t(3, "item"),
        t(3, "other"),
        t(3, "not-loaded"),
    ];
    if positioned {
        h.push(m(
            11,
            &RawMessage::decode(&[9, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        ));
    }
    let section = Section::from_raw(raw(&[
        t(1, id),
        n(2, 37),
        t(6, snapshot.thread_id()),
        t(7, snapshot.document_id()),
        n(9, 9),
        n(11, 1),
        m(12, &raw(&[m(7, &raw(&h))])),
        t(21, "old-position"),
        t(91, "unrelated original"),
    ]))
    .unwrap();
    snapshot.sections.push(section);
    let container = snapshot.document.message(16).unwrap().unwrap_or(raw(&[]));
    let mut fields = container
        .messages(1)
        .unwrap()
        .iter()
        .map(|v| m(1, v))
        .collect::<Vec<_>>();
    fields.push(m(
        1,
        &raw(&[
            t(1, id),
            n(2, count),
            n(8, u64::from(archived)),
            t(99, "original metadata"),
        ]),
    ));
    snapshot.document = snapshot
        .document
        .replace_fields(16, &[m(16, &raw(&fields))])
        .unwrap();
}
fn fixture() -> CanvasCollabSnapshot {
    let mut s = super::tests::fixture();
    // Source initial registry order is not lexical; the missing associated ID
    // is retained too. Both hidden controls clear their entire associations.
    add(&mut s, "z-stub", false, 0, false);
    add(&mut s, "a-archived", true, 4, false);
    add(&mut s, "visible-positioned", false, 2, true);
    s
}
fn prepare(s: &CanvasCollabSnapshot) -> PreparedAnnotation {
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
        s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
    )
    .unwrap() else {
        panic!("new section annotation")
    };
    intent.prepare(s).unwrap()
}
fn responses(p: &PreparedAnnotation, original: &CanvasCollabSnapshot) -> Vec<Section> {
    p.document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|request| {
            let request = Section::from_raw(request).unwrap();
            let mut value = original.section(request.id()).map_or_else(
                || request.clone(),
                |old| old.merge_partial(&request).unwrap(),
            );
            for f in [n(2, 10001), t(6, p.thread_id()), t(7, p.document_id())] {
                value = changed(&value, f);
            }
            value
        })
        .collect()
}
fn ack(sections: &[Section]) -> EditAck {
    EditAck::decode(
        raw(&[
            n(1, 1),
            m(
                2,
                &raw(&sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
            ),
        ])
        .raw(),
    )
    .unwrap()
}
fn fresh(s: &CanvasCollabSnapshot, sections: &[Section]) -> CanvasCollabSnapshot {
    let mut current = s.clone();
    for section in sections {
        if let Some(old) = current
            .sections
            .iter_mut()
            .find(|old| old.id() == section.id())
        {
            *old = section.clone();
        } else {
            current.sections.push(section.clone());
        }
    }
    current
}

#[test]
fn ordered_section_cleanup_retains_visible_control_all_originals_and_version_six_evidence() {
    let s = fixture();
    let before = s.sections.clone();
    let p = prepare(&s);
    let updates = p.document_data().messages(1).unwrap();
    assert_eq!(
        updates
            .iter()
            .map(|u| u.string(1).unwrap().unwrap())
            .collect::<Vec<_>>(),
        vec!["z-stub", "a-archived", p.control_id()]
    );
    for update in &updates[..2] {
        assert!(
            strings(
                &update
                    .message(12)
                    .unwrap()
                    .unwrap()
                    .message(7)
                    .unwrap()
                    .unwrap(),
                3
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(update.string(21).unwrap(), Some("zzzzzz-orphaned-m"));
        assert!(!update.has(3)); // Association clearing does not delete the root.
    }
    assert_eq!(s.sections, before);
    let record = p.record().unwrap();
    let encoded = serde_json::to_value(&record).unwrap();
    assert_eq!(encoded["version"], 6);
    serde_json::from_value::<AnnotationRecord>(encoded.clone())
        .unwrap()
        .validate()
        .unwrap();
    for key in ["section_plan", "request"] {
        let mut forged = encoded.clone();
        forged[key] = if key == "request" {
            serde_json::json!([])
        } else {
            serde_json::Value::Null
        };
        assert!(
            serde_json::from_value::<AnnotationRecord>(forged)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut legacy = encoded;
    legacy["version"] = serde_json::json!(1);
    assert!(
        serde_json::from_value::<AnnotationRecord>(legacy)
            .unwrap()
            .validate()
            .is_err()
    );
    let received = responses(&p, &s);
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let canonical = fresh(&s, &received);
    let proof = accepted.confirm(&canonical).unwrap();
    assert_eq!(proof.changed_controls().len(), 2);
    assert_eq!(
        canonical.section("visible-positioned"),
        s.section("visible-positioned")
    );
    assert_eq!(canonical.document, s.document);
}

#[test]
fn last_existing_section_root_is_read_only_and_hidden_context_or_complete_owner_changes_refuse() {
    let mut s = fixture();
    add(&mut s, "first-visible", false, 1, false);
    add(&mut s, "last-visible", false, 2, false);
    let present = AnnotationPresentation::ordinary(None, None).unwrap();
    assert!(
        matches!(AnnotationIntent::for_section(&s,"item",&present).unwrap(),SectionAnnotationChoice::Existing(id) if id=="last-visible")
    );
    let mut s = fixture();
    assert!(
        matches!(AnnotationIntent::for_section(&s,"item",&AnnotationPresentation::ordinary(Some("z-stub".into()),None).unwrap()).unwrap(),SectionAnnotationChoice::Existing(id) if id=="z-stub")
    );
    let SectionAnnotationChoice::New(intent) =
        AnnotationIntent::for_section(&s, "item", &present).unwrap()
    else {
        panic!()
    };
    let index = s
        .sections
        .iter()
        .position(|v| v.id() == "visible-positioned")
        .unwrap();
    s.sections[index] = changed(&s.sections[index], n(2, 38));
    assert!(intent.prepare(&s).is_err());
    let mut s = fixture();
    let index = s.sections.iter().position(|v| v.id() == "item").unwrap();
    s.sections[index] = changed(&s.sections[index], t(6, "foreign-thread"));
    assert!(AnnotationIntent::for_section(&s, "item", &present).is_err());
}

#[test]
fn full_ack_and_fresh_metadata_are_required_and_failed_confirmation_keeps_all_received_bytes() {
    let s = fixture();
    for failure in 0..3 {
        let p = prepare(&s);
        let mut received = responses(&p, &s);
        match failure {
            0 => {
                received.remove(0);
            }
            1 => {
                received[0] = changed(&received[0], t(7, "foreign"));
            }
            _ => {
                received[0] = s.section("z-stub").unwrap().clone();
            }
        }
        assert!(p.validate_ack(&ack(&received)).is_err());
    }
    let p = prepare(&s);
    let received = responses(&p, &s);
    // Duplicate IDs are rejected by the actual wire decoder before validation.
    let duplicate = raw(&[
        n(1, 1),
        m(
            2,
            &raw(&[
                m(1, received[0].raw()),
                m(1, received[0].raw()),
                m(1, received[2].raw()),
            ]),
        ),
    ]);
    assert!(EditAck::decode(duplicate.raw()).is_err());
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert_eq!(evidence.changed_controls.len(), 2);
    let mut canonical = fresh(&s, &received);
    let index = canonical
        .sections
        .iter()
        .position(|v| v.id() == "a-archived")
        .unwrap();
    canonical.sections[index] = s.section("a-archived").unwrap().clone();
    assert!(accepted.check_confirmation(&canonical).is_err());
    assert_eq!(
        accepted.evidence().unwrap().changed_controls,
        evidence.changed_controls
    );
    canonical = fresh(&s, &received);
    canonical.document = canonical.document.remove(16).unwrap();
    assert!(accepted.check_confirmation(&canonical).is_err());
    assert_eq!(
        accepted.evidence().unwrap().changed_controls,
        evidence.changed_controls
    );
    accepted.check_confirmation(&fresh(&s, &received)).unwrap();
}

#[test]
fn complete_topology_includes_title_interval_and_rejects_opaque_or_duplicate_body() {
    let mut s = fixture();
    let topology = control_topology::ControlTopology::capture(&s).unwrap();
    assert_eq!(topology.after_clear("z-stub").unwrap(), "zzzzzz-orphaned-m");
    let index = s.sections.iter().position(|s| s.id() == "other").unwrap();
    let content = raw(&[m(
        1,
        &raw(&[t(1, "x<annotation id=\"z-stub\">label</annotation>y")]),
    )]);
    s.sections[index] = changed(&s.sections[index], m(12, &content));
    let topology = control_topology::ControlTopology::capture(&s).unwrap();
    assert_eq!(
        topology.after_clear("z-stub").unwrap(),
        position::between(Some("aac"), Some("aae")).unwrap()
    );
    topology.current(&s).unwrap();
    let mut opaque = s.clone();
    opaque.sections[index] = changed(&opaque.sections[index], n(9, 44));
    assert!(control_topology::ControlTopology::capture(&opaque).is_err());
    assert!(topology.current(&opaque).is_err());
    s.sections.push(s.sections[index].clone());
    assert!(control_topology::ControlTopology::capture(&s).is_err());
}

mod journal;
#[path = "list_tests.rs"]
mod list_tests;

#[test]
fn section_styles_use_actual_snippet_mapping_without_changing_legacy_or_body_authority() {
    let plain = super::tests::fixture();
    for (style, kind, level) in [
        (0, "rich_text_section", None),
        (1, "rich_text_header", Some(1)),
        (2, "rich_text_header", Some(2)),
        (3, "rich_text_header", Some(3)),
        (39, "rich_text_section", None),
        (40, "rich_text_section", None),
        (41, "rich_text_section", None),
        (4, "rich_text_preformatted", None),
        (16, "rich_text_quote", None),
        (17, "rich_text_quote", None),
        (47, "rich_text_section", None),
    ] {
        let mut s = plain.clone();
        let i = s.sections.iter().position(|v| v.id() == "item").unwrap();
        s.sections[i] = changed(&s.sections[i], n(10, style));
        let p = prepare(&s);
        let record = p.record().unwrap();
        assert_eq!(p.document_data().messages(1).unwrap().len(), 1);
        let rich: serde_json::Value =
            serde_json::from_str(record.snippet()["richText"].as_str().unwrap()).unwrap();
        assert_eq!(rich["blocks"][0]["elements"][0]["type"], kind);
        assert_eq!(
            rich["blocks"][0]["elements"][0]
                .get("level")
                .and_then(serde_json::Value::as_u64),
            level
        );
        assert_eq!(record.anchor().unwrap(), s.sections[i]);
        let preview =
            section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap()).unwrap();
        assert_eq!(
            preview.iter().find(|v| v.id == "item").unwrap().blocks,
            rich["blocks"].as_array().unwrap().clone()
        );
        if style != 0 {
            assert!(AnnotationIntent::capture_range(&s, "item", 0, 1).is_err());
        }
    }
    // The new source export behavior is explicitly version 6. Historical v1
    // evidence keeps its original plain snippet even with default-content data.
    let mut s = plain;
    let i = s.sections.iter().position(|v| v.id() == "item").unwrap();
    let attrs = s.sections[i].raw().message(16).unwrap().unwrap_or(raw(&[]));
    s.sections[i] = changed(
        &s.sections[i],
        m(16, &attrs.replace_fields(13, &[n(13, 1)]).unwrap()),
    );
    let legacy = AnnotationIntent::capture(&s, "item")
        .unwrap()
        .prepare(&s)
        .unwrap()
        .record()
        .unwrap();
    let current = prepare(&s).record().unwrap();
    assert!(!legacy.snippet()["mrkdwn"].as_str().unwrap().is_empty());
    assert_eq!(current.snippet()["mrkdwn"], "");
    serde_json::from_value::<AnnotationRecord>(serde_json::to_value(&legacy).unwrap())
        .unwrap()
        .validate()
        .unwrap();
}

#[path = "image_tests.rs"]
mod image_tests;

#[path = "table_tests.rs"]
mod table_tests;
