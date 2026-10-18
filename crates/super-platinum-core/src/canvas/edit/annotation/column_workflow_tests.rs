//! Real typed annotation/ACK/canonical and journal tests for both column roots.
use super::*;
fn columned(mut s: CanvasCollabSnapshot, callout: bool, nested: bool) -> CanvasCollabSnapshot {
    s.sections.retain(|s| s.id() != "table");
    let original = s.section("item").unwrap().clone();
    let (kind, style, tag, member) = if callout {
        (69, 49, 63, 2)
    } else {
        (26, 21, 24, 1)
    };
    let mut root = original.raw().remove(38).unwrap().remove(15).unwrap();
    let column = |name: &str, ids: &[&str]| {
        let mut f = if callout {
            vec![t(1, name)]
        } else {
            vec![n(2, 3)]
        };
        f.extend(ids.iter().map(|id| t(member, id)));
        f.push(t(99, "future column metadata"));
        m(1, &raw(&f))
    };
    let target = if nested { "quote" } else { "item" };
    for f in [
        t(1, "columns"),
        t(8, "aab"),
        t(21, "aab"),
        n(2, 60),
        n(9, kind),
        n(10, style),
        n(29, 0),
        m(
            12,
            &raw(&[m(
                tag,
                &raw(&[
                    column("left", &[target]),
                    column("right", &["sibling"]),
                    n(2, 0),
                ]),
            )]),
        ),
    ] {
        root = root
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap();
    }
    s.sections.push(Section::from_raw(root).unwrap());
    let mut sibling = original.raw().remove(38).unwrap().remove(15).unwrap();
    for f in [
        t(1, "sibling"),
        t(8, "aad"),
        t(21, "aad"),
        n(9, 0),
        n(10, 0),
        n(3, 1),
        m(12, &raw(&[m(1, &raw(&[t(1, "deleted original sibling")]))])),
    ] {
        sibling = sibling
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap();
    }
    s.sections.push(Section::from_raw(sibling).unwrap());
    if nested {
        let mut quote = original.raw().remove(38).unwrap().remove(15).unwrap();
        for f in [
            t(1, "quote"),
            t(8, "aaba"),
            t(21, "aaba"),
            n(9, 71),
            n(10, 50),
            m(12, &raw(&[m(65, &raw(&[t(1, "item")]))])),
        ] {
            quote = quote
                .merge_partial(&RawMessage::decode(f.raw()).unwrap())
                .unwrap();
        }
        s.sections.push(Section::from_raw(quote).unwrap());
    }
    s
}
fn changed_columns(s: &CanvasCollabSnapshot, mode: u8) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let i = s.sections.iter().position(|r| r.id() == "columns").unwrap();
    match mode {
        0 => {
            s.sections.remove(i);
        }
        1 => s.sections[i] = changed(&s.sections[i], n(2, 999)),
        2 => s.sections[i] = changed(&s.sections[i], n(35, 1)),
        _ => {
            let tag = if s.sections[i].section_type().unwrap() == Some(69) {
                63
            } else {
                24
            };
            s.sections[i] = changed(&s.sections[i], m(12, &raw(&[m(tag, &raw(&[]))])));
        }
    }
    s
}
#[test]
fn actual_columns_text_and_list_keep_roots_and_deleted_siblings_out_of_writes() {
    for callout in [false, true] {
        for source in [table_fixture(), list(5), list(6), list(7)] {
            let s = columned(source, callout, false);
            let p = table_prepare(&s);
            let record = p.record().unwrap();
            let value = serde_json::to_value(&record).unwrap();
            assert_eq!(value["version"], 13);
            assert_eq!(value["section_plan"]["column_layout"], true);
            assert_eq!(
                value["section_plan"]
                    .get("table_layout")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                false
            );
            assert_eq!(p.intent.path, position::between(None, Some("aac")).unwrap());
            assert!(
                p.document_data()
                    .messages(1)
                    .unwrap()
                    .iter()
                    .all(|v| !matches!(
                        v.string(1).unwrap(),
                        Some("columns" | "sibling" | "quote" | "table" | "child-a" | "child-b")
                    ))
            );
            let previews =
                section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap())
                    .unwrap();
            assert!(previews.iter().any(|v| v.id == "item"));
            assert!(
                !previews
                    .iter()
                    .any(|v| matches!(v.id.as_str(), "columns" | "quote" | "sibling"))
            );
            let rows = responses(&p, &s);
            let canonical = fresh(&s, &rows);
            p.validate_ack(&ack(&rows))
                .unwrap()
                .confirm(&canonical)
                .unwrap();
            assert_eq!(canonical.section("columns"), s.section("columns"));
            record.validate().unwrap();
        }
    }
}

#[test]
fn column_v13_cannot_be_demoted_or_strip_membership_and_legacy_records_stay_exact() {
    for callout in [false, true] {
        let p = table_prepare(&columned(list(7), callout, true));
        let value = serde_json::to_value(p.record().unwrap()).unwrap();
        for version in 1..=12 {
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
            .remove("column_layout");
        assert!(
            serde_json::from_value::<AnnotationRecord>(missing)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut quote_only = columned(list(7), false, true);
    quote_only
        .sections
        .retain(|s| !matches!(s.id(), "columns" | "sibling"));
    let i = quote_only
        .sections
        .iter()
        .position(|s| s.id() == "quote")
        .unwrap();
    quote_only.sections[i] = changed(&quote_only.sections[i], n(29, 0));
    for (s, version) in [(table_fixture(), 10), (list(5), 11), (quote_only, 12)] {
        let r = table_prepare(&s).record().unwrap();
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["version"], version);
        assert!(v["section_plan"].get("column_layout").is_none());
        assert_eq!(
            v["section_plan"]
                .get("quote_layout")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            version == 12
        );
        r.validate().unwrap();
    }
}

#[test]
fn actual_column_ack_requires_complete_root_membership_and_deleted_child_custody() {
    let s = columned(list(7), true, true);
    let p = table_prepare(&s);
    let r = p.record().unwrap();
    let rows = responses(&p, &s);
    let canonical = fresh(&s, &rows);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    for mode in 0..4 {
        let changed = changed_columns(&canonical, mode);
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
fn actual_column_confirmation_retains_deleted_direct_sibling_and_exact_column_order() {
    for callout in [false, true] {
        let s = columned(table_fixture(), callout, false);
        let p = table_prepare(&s);
        let rows = responses(&p, &s);
        let canonical = fresh(&s, &rows);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        accepted.check_confirmation(&canonical).unwrap();
        for field in [n(2, 999), n(3, 0), t(99, "changed hidden original")] {
            let mut now = canonical.clone();
            let i = now
                .sections
                .iter()
                .position(|r| r.id() == "sibling")
                .unwrap();
            now.sections[i] = changed(&now.sections[i], field);
            assert!(accepted.check_confirmation(&now).is_err());
        }
        let mut now = canonical.clone();
        now.sections.retain(|s| s.id() != "sibling");
        assert!(accepted.check_confirmation(&now).is_err());
        let mut now = canonical.clone();
        let i = now
            .sections
            .iter()
            .position(|r| r.id() == "columns")
            .unwrap();
        let tag = if callout { 63 } else { 24 };
        let root = now.sections[i]
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(tag)
            .unwrap()
            .unwrap();
        let reversed = root
            .messages(1)
            .unwrap()
            .iter()
            .rev()
            .map(|c| m(1, c))
            .collect::<Vec<_>>();
        now.sections[i] = changed(
            &now.sections[i],
            m(
                12,
                &raw(&[m(tag, &root.replace_fields(1, &reversed).unwrap())]),
            ),
        );
        assert!(accepted.check_confirmation(&now).is_err());
        accepted.check_confirmation(&canonical).unwrap();
    }
}

#[test]
fn column_nested_quote_images_keep_actual_export_outcomes_and_owned_ingress() {
    for callout in [false, true] {
        let s = columned(image(), callout, true);
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
                    .check_confirmation(&changed_columns(&canonical, 0))
                    .is_err()
            );
        }
    }
}

#[test]
fn column_body_reparenting_or_read_only_ancestor_refuses_before_dispatch() {
    let s = columned(list(5), false, true);
    for (id, field) in [
        ("quote", n(35, 1)),
        ("columns", n(35, 1)),
        ("quote", n(3, 1)),
    ] {
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
    assert!(intent.prepare(&changed_columns(&s, 3)).is_err());
}

#[path = "column_journal_tests.rs"]
mod journal;

#[test]
fn actual_column_whole_body_cleanup_keeps_parent_history_and_all_control_acks() {
    let (source, old) = table_body_fixture();
    let s = columned(source, true, true);
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
            .check_confirmation(&changed_columns(&canonical, 1))
            .is_err()
    );
    accepted.confirm(&canonical).unwrap();
}
