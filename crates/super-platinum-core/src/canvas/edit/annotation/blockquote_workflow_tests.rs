//! Real annotation builders and canonical ACKs; no native visibility substitute.
use super::*;

fn quoted(mut s: CanvasCollabSnapshot, in_table: bool) -> CanvasCollabSnapshot {
    let item = s.section("item").unwrap();
    let mut quote = item.raw().remove(38).unwrap().remove(15).unwrap();
    for f in [
        t(1, "quote"),
        t(8, "aaba"),
        t(21, "aaba"),
        n(2, 51),
        n(9, 71),
        n(10, 50),
        n(29, u64::from(in_table)),
        m(12, &raw(&[m(65, &raw(&[t(1, "item")]))])),
    ] {
        quote = quote
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap();
    }
    s.sections.push(Section::from_raw(quote).unwrap());
    if in_table {
        let i = s.sections.iter().position(|s| s.id() == "table").unwrap();
        let content = s.sections[i].raw().message(12).unwrap().unwrap();
        let layout = content.message(30).unwrap().unwrap();
        let cells = layout
            .messages(3)
            .unwrap()
            .iter()
            .map(|c| m(3, &c.replace_fields(4, &[t(4, "quote")]).unwrap()))
            .collect::<Vec<_>>();
        s.sections[i] = changed(
            &s.sections[i],
            m(
                12,
                &raw(&[m(30, &layout.replace_fields(3, &cells).unwrap())]),
            ),
        );
    } else {
        s.sections.retain(|s| s.id() != "table");
    }
    s
}

#[test]
fn actual_quote_text_and_list_workflows_keep_layout_roots_out_of_every_write() {
    for table in [false, true] {
        for source in [table_fixture(), list(5), list(6), list(7)] {
            let s = quoted(source, table);
            let p = table_prepare(&s);
            let record = p.record().unwrap();
            let value = serde_json::to_value(&record).unwrap();
            assert_eq!(value["version"], 12);
            assert_eq!(value["section_plan"]["quote_layout"], true);
            assert_eq!(
                value["section_plan"]
                    .get("table_layout")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                table
            );
            assert_eq!(p.intent.path, position::between(None, Some("aac")).unwrap());
            assert!(
                p.document_data()
                    .messages(1)
                    .unwrap()
                    .iter()
                    .all(|v| !matches!(
                        v.string(1).unwrap(),
                        Some("quote" | "table" | "child-a" | "child-b")
                    ))
            );
            let previews =
                section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap())
                    .unwrap();
            assert!(previews.iter().any(|v| v.id == "item"));
            assert!(!previews.iter().any(|v| v.id == "quote"));
            let rows = responses(&p, &s);
            let canonical = fresh(&s, &rows);
            p.validate_ack(&ack(&rows))
                .unwrap()
                .confirm(&canonical)
                .unwrap();
            assert_eq!(canonical.section("quote"), s.section("quote"));
            record.validate().unwrap();
        }
    }
}

#[test]
fn quote_v12_cannot_be_demoted_and_legacy_records_keep_their_versions() {
    for table in [false, true] {
        let p = table_prepare(&quoted(list(7), table));
        let value = serde_json::to_value(p.record().unwrap()).unwrap();
        for version in 1..=11 {
            let mut old = value.clone();
            old["version"] = serde_json::json!(version);
            assert!(
                serde_json::from_value::<AnnotationRecord>(old)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        let mut missing = value;
        missing["section_plan"]
            .as_object_mut()
            .unwrap()
            .remove("quote_layout");
        assert!(
            serde_json::from_value::<AnnotationRecord>(missing)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for (s, version) in [(table_fixture(), 10), (list(5), 11)] {
        let r = table_prepare(&s).record().unwrap();
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["version"], version);
        assert!(v["section_plan"].get("quote_layout").is_none());
        r.validate().unwrap();
    }
}

fn changed_quote(s: &CanvasCollabSnapshot, mode: u8) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let i = s.sections.iter().position(|s| s.id() == "quote").unwrap();
    match mode {
        0 => {
            s.sections.remove(i);
        }
        1 => s.sections[i] = changed(&s.sections[i], n(2, 999)),
        2 => s.sections[i] = changed(&s.sections[i], n(35, 1)),
        _ => s.sections[i] = changed(&s.sections[i], m(12, &raw(&[m(65, &raw(&[]))]))),
    }
    s
}

#[test]
fn actual_quote_ack_requires_same_complete_tree_for_confirmation_and_local_review() {
    let s = quoted(list(7), true);
    let p = table_prepare(&s);
    let r = p.record().unwrap();
    let rows = responses(&p, &s);
    let canonical = fresh(&s, &rows);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    for mode in 0..4 {
        let changed = changed_quote(&canonical, mode);
        assert!(accepted.check_confirmation(&changed).is_err());
        let observation = r.review_current(&changed).unwrap();
        let prior = observation.prior_evidence().unwrap();
        r.validate_prior_evidence(Some(&prior)).unwrap();
        assert!(observation.preview_blocks().is_ok());
        assert!(observation.matches_current(&changed).unwrap());
        assert!(!observation.matches_current(&canonical).unwrap());
    }
    for mode in 0..4 {
        assert!(
            accepted
                .check_confirmation(&mutated_child(&canonical, mode))
                .is_err()
        );
    }
}

#[test]
fn quote_images_keep_actual_export_outcomes_and_never_use_plain_section_ingress() {
    for table in [false, true] {
        let s = quoted(image(), table);
        let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
        assert!(AnnotationIntent::for_section(&s, "item", &presentation).is_err());
        for outcome in [
            ImageExport::Exported,
            ImageExport::NoImage,
            ImageExport::EmptyData,
            ImageExport::ExportError,
        ] {
            let p = image_prepare(&s, outcome);
            let r = p.record().unwrap();
            assert_eq!(
                serde_json::to_value(&r).unwrap()["section_plan"]["image_export"],
                serde_json::to_value(outcome).unwrap()
            );
            let rows = responses(&p, &s);
            let canonical = fresh(&s, &rows);
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            accepted.check_confirmation(&canonical).unwrap();
            assert!(
                accepted
                    .check_confirmation(&changed_quote(&canonical, 0))
                    .is_err()
            );
        }
    }
}

#[test]
fn quote_body_reparenting_or_read_only_outer_owner_refuses_before_dispatch() {
    let s = quoted(list(5), true);
    for (id, field) in [("quote", n(35, 1)), ("table", n(35, 1)), ("quote", n(3, 1))] {
        let mut now = s.clone();
        let i = now.sections.iter().position(|s| s.id() == id).unwrap();
        now.sections[i] = changed(&now.sections[i], field);
        assert!(
            AnnotationIntent::for_section(
                &now,
                "item",
                &AnnotationPresentation::ordinary(None, None).unwrap()
            )
            .is_err()
        );
    }
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_section(
        &s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
    )
    .unwrap() else {
        panic!("new")
    };
    assert!(intent.prepare(&changed_quote(&s, 3)).is_err());
}

#[path = "blockquote_journal_tests.rs"]
mod journal;

#[test]
fn actual_quote_whole_body_cleanup_requires_original_parent_history_and_all_controls() {
    let (source, old) = table_body_fixture();
    let s = quoted(source, true);
    let p = table_prepare(&s);
    let record = p.record().unwrap();
    assert!(record.has_parent_update());
    assert_eq!(
        p.document_data()
            .messages(1)
            .unwrap()
            .iter()
            .map(|r| r.string(1).unwrap().unwrap())
            .collect::<Vec<_>>(),
        ["item", "z-stub", "a-archived", p.control_id()]
    );
    let rows = canonical_responses(&p, &s);
    assert!(
        rows.iter()
            .any(|r| r.id() == "item" && r.crdt().unwrap().is_some())
    );
    assert_eq!(s.section(&old), fresh(&s, &rows).section(&old));
    // Each negative owns its own generated annotation identity and receipt.
    let negative = table_prepare(&s);
    let mut missing = canonical_responses(&negative, &s);
    missing.retain(|r| r.id() != "item");
    assert!(negative.validate_ack(&ack(&missing)).is_err());
    let canonical = fresh(&s, &rows);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    assert!(
        accepted
            .check_confirmation(&changed_quote(&canonical, 1))
            .is_err()
    );
    accepted.confirm(&canonical).unwrap();
}
