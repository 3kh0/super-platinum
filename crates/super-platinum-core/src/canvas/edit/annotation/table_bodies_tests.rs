//! Actual v11 intent, ACK, confirmation and retained local-observation paths.
use super::*;

fn list(style: u64) -> CanvasCollabSnapshot {
    let mut s = table_fixture();
    let at = s.sections.iter().position(|r| r.id() == "item").unwrap();
    let original = s.sections[at].clone();
    let mut root = original.raw().remove(38).unwrap();
    for field in [n(9, 1), n(10, style), m(12, &raw(&[]))] {
        root = root
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections[at] = Section::from_raw(root).unwrap();
    for (id, order, html, indent, checked) in [
        ("child-a", "aaf", "one", 0, 0),
        ("child-b", "aag", "<b>two</b>", 1, 1),
    ] {
        let mut child = original.raw().remove(38).unwrap().remove(29).unwrap();
        for field in [
            t(1, id),
            t(8, order),
            t(21, &format!("aac-{order}")),
            n(9, 0),
            n(10, 0),
            m(13, &raw(&[t(1, "item"), n(4, style)])),
            m(12, &raw(&[m(1, &raw(&[t(1, html)]))])),
            m(16, &raw(&[n(1, indent), n(2, checked)])),
        ] {
            child = child
                .merge_partial(&RawMessage::decode(field.raw()).unwrap())
                .unwrap();
        }
        s.sections.push(Section::from_raw(child).unwrap());
    }
    s
}
fn image() -> CanvasCollabSnapshot {
    let mut s = table_fixture();
    let at = s.sections.iter().position(|r| r.id() == "item").unwrap();
    let mut root = s.sections[at].raw().remove(38).unwrap();
    for field in [
        n(9, 2),
        n(10, 11),
        m(
            12,
            &raw(&[m(
                2,
                &raw(&[
                    t(15, "Original <image>"),
                    m(3, &raw(&[t(5, "FIMAGE"), n(2, 512), n(3, 512)])),
                ]),
            )]),
        ),
    ] {
        root = root
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections[at] = Section::from_raw(root).unwrap();
    s
}
fn image_prepare(s: &CanvasCollabSnapshot, outcome: ImageExport) -> PreparedAnnotation {
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_image(
        s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
        outcome,
    )
    .unwrap() else {
        panic!("new")
    };
    intent.prepare_at(s, 2_000_000).unwrap()
}
fn mutated_child(s: &CanvasCollabSnapshot, mode: u8) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let i = s.sections.iter().position(|r| r.id() == "child-b").unwrap();
    match mode {
        0 => {
            s.sections.remove(i);
        }
        1 => s.sections[i] = changed(&s.sections[i], n(2, 9999)),
        2 => s.sections[i] = changed(&s.sections[i], n(3, 1)),
        _ => s.sections[i] = changed(&s.sections[i], m(13, &raw(&[t(1, "other"), n(4, 5)]))),
    }
    s
}

#[test]
fn table_list_three_styles_keep_original_export_and_all_descendants_out_of_write_batch() {
    for style in 5..=7 {
        let s = list(style);
        let before = s.sections.clone();
        assert!(table_layout::Tables::capture(&s.sections).is_err());
        let p = table_prepare(&s);
        let r = p.record().unwrap();
        assert_eq!(serde_json::to_value(&r).unwrap()["version"], 11);
        assert_eq!(
            serde_json::to_value(&r).unwrap()["section_plan"]["table_bodies"],
            true
        );
        assert_eq!(
            p.document_data()
                .messages(1)
                .unwrap()
                .iter()
                .map(|x| x.string(1).unwrap().unwrap())
                .collect::<Vec<_>>(),
            ["z-stub", "a-archived", p.control_id()]
        );
        assert!(!r.has_parent_update());
        assert_eq!(s.sections, before);
        let rich: serde_json::Value =
            serde_json::from_str(r.snippet()["richText"].as_str().unwrap()).unwrap();
        let previews =
            section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap()).unwrap();
        assert_eq!(
            previews.iter().find(|p| p.id == "item").unwrap().blocks,
            rich["blocks"].as_array().unwrap().clone()
        );
        assert!(r.snippet()["mrkdwn"].as_str().unwrap().contains("*two*"));
        let rows = responses(&p, &s);
        let canonical = fresh(&s, &rows);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        accepted.check_confirmation(&canonical).unwrap();
        assert_eq!(
            r.review_current(&canonical)
                .unwrap()
                .preview_blocks()
                .unwrap(),
            rich["blocks"].as_array().unwrap().clone()
        );
        for mode in 0..4 {
            assert!(
                accepted
                    .check_confirmation(&mutated_child(&canonical, mode))
                    .is_err()
            );
        }
        accepted.check_confirmation(&canonical).unwrap();
    }
}

#[test]
fn v11_cannot_be_demoted_or_strip_its_canonical_table_body_mode() {
    for s in [list(5), image()] {
        let p = if s.section("item").unwrap().section_type().unwrap() == Some(2) {
            image_prepare(&s, ImageExport::Exported)
        } else {
            table_prepare(&s)
        };
        let value = serde_json::to_value(p.record().unwrap()).unwrap();
        for version in 1..=10 {
            let mut old = value.clone();
            old["version"] = serde_json::json!(version);
            assert!(
                serde_json::from_value::<AnnotationRecord>(old)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        for key in ["table_bodies", "table_layout"] {
            let mut missing = value.clone();
            missing["section_plan"].as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<AnnotationRecord>(missing)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        let r: AnnotationRecord = serde_json::from_value(value).unwrap();
        r.validate().unwrap();
    }
    let old = table_prepare(&table_fixture()).record().unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap()["version"], 10);
    assert!(
        serde_json::to_value(&old).unwrap()["section_plan"]
            .get("table_bodies")
            .is_none()
    );
    old.validate().unwrap();
}

#[test]
fn table_image_native_outcomes_remain_distinct_and_cannot_be_fabricated_by_section_ingress() {
    let s = image();
    let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
    assert!(AnnotationIntent::for_section(&s, "item", &presentation).is_err());
    assert!(AnnotationIntent::capture(&s, "item").is_err());
    for non_image in [table_fixture(), list(5)] {
        assert!(matches!(
            AnnotationIntent::for_image(&non_image, "item", &presentation, ImageExport::Exported),
            Err(Error::Unsupported)
        ));
    }
    for outcome in [
        ImageExport::Exported,
        ImageExport::NoImage,
        ImageExport::EmptyData,
        ImageExport::ExportError,
    ] {
        // Unit input only: mounted native observation remains a separate gate.
        let p = image_prepare(&s, outcome);
        let record = p.record().unwrap();
        assert_eq!(record.snippet()["fileIds"], serde_json::json!(["FIMAGE"]));
        assert_eq!(
            serde_json::to_value(&record).unwrap()["section_plan"]["image_export"],
            serde_json::to_value(outcome).unwrap()
        );
        let rows = responses(&p, &s);
        let canonical = fresh(&s, &rows);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        accepted.check_confirmation(&canonical).unwrap();
        assert_eq!(canonical.section("item"), s.section("item"));
        let mut moved = canonical.clone();
        let i = moved
            .sections
            .iter()
            .position(|x| x.id() == "table")
            .unwrap();
        moved.sections[i] = changed(&moved.sections[i], n(2, 999));
        assert!(accepted.check_confirmation(&moved).is_err());
    }
}

#[test]
fn table_body_live_intent_refuses_removed_foreign_and_changed_parent_before_dispatch() {
    let s = list(5);
    let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
    for mode in 0..4 {
        let SectionAnnotationChoice::New(intent) =
            AnnotationIntent::for_section(&s, "item", &presentation).unwrap()
        else {
            panic!("new")
        };
        assert!(intent.prepare(&mutated_child(&s, mode)).is_err());
    }
    for field in [n(3, 1), n(35, 1), t(6, "foreign-thread")] {
        let mut bad = s.clone();
        let i = bad.sections.iter().position(|r| r.id() == "table").unwrap();
        bad.sections[i] = changed(&bad.sections[i], field);
        assert!(AnnotationIntent::for_section(&bad, "item", &presentation).is_err());
    }
    let mut missing = s.clone();
    missing.sections.retain(|r| r.id() != "table");
    assert!(AnnotationIntent::for_section(&missing, "item", &presentation).is_err());
}

#[test]
fn deleted_table_list_descendants_remain_in_confirmation_and_local_review_custody() {
    let mut s = list(7);
    let i = s.sections.iter().position(|r| r.id() == "child-b").unwrap();
    s.sections[i] = changed(&s.sections[i], n(3, 1));
    let p = table_prepare(&s);
    let record = p.record().unwrap();
    let rows = responses(&p, &s);
    let canonical = fresh(&s, &rows);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    for mode in [0, 1, 3] {
        let changed = mutated_child(&canonical, mode);
        assert!(accepted.check_confirmation(&changed).is_err());
        let observation = record.review_current(&changed).unwrap();
        let raw = observation.prior_evidence().unwrap();
        record.validate_prior_evidence(Some(&raw)).unwrap();
        assert!(observation.matches_current(&changed).unwrap());
        assert!(!observation.matches_current(&canonical).unwrap());
    }
}

#[path = "table_bodies_journal_tests.rs"]
mod journal;

#[path = "blockquote_workflow_tests.rs"]
mod quotes;

#[path = "column_workflow_tests.rs"]
mod columns;
