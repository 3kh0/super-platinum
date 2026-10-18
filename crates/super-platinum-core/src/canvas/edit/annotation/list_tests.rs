use super::*;

fn list_fixture(style: u64, controls: bool) -> CanvasCollabSnapshot {
    let mut s = if controls {
        fixture()
    } else {
        super::super::tests::fixture()
    };
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    let source = s.sections[i].clone();
    let mut list = source.raw().remove(38).unwrap();
    for field in [n(9, 1), n(10, style), m(12, &raw(&[]))] {
        list = list
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections[i] = Section::from_raw(list).unwrap();
    for (id, path, html, indent, checked) in [
        ("child-a", "aaf", "one", 0, 0),
        ("child-b", "aag", "<b>two</b>", 1, 1),
    ] {
        let mut child = source.raw().remove(38).unwrap();
        for field in [
            t(1, id),
            t(8, path),
            t(21, &format!("aac-{path}")),
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

#[test]
fn list_annotation_prepares_only_ordered_controls_and_new_root_with_exact_child_custody() {
    for style in 5..=7 {
        let s = list_fixture(style, true);
        let before = s.sections.clone();
        let p = prepare(&s);
        let record = p.record().unwrap();
        assert_eq!(serde_json::to_value(&record).unwrap()["version"], 8);
        let updates = p.document_data().messages(1).unwrap();
        assert_eq!(
            updates
                .iter()
                .map(|u| u.string(1).unwrap().unwrap())
                .collect::<Vec<_>>(),
            vec!["z-stub", "a-archived", p.control_id()]
        );
        assert_eq!(s.sections, before);
        assert!(!record.has_parent_update());
        let rich: serde_json::Value =
            serde_json::from_str(record.snippet()["richText"].as_str().unwrap()).unwrap();
        assert_eq!(rich["blocks"][0]["elements"][0]["type"], "rich_text_list");
        let preview =
            section_previews(&s, &AnnotationPresentation::ordinary(None, None).unwrap()).unwrap();
        assert_eq!(
            preview.iter().find(|v| v.id == "item").unwrap().blocks,
            rich["blocks"].as_array().unwrap().clone()
        );
        let actual = responses(&p, &s);
        let bad = prepare(&s);
        let bad_rows = responses(&bad, &s);
        assert!(bad.validate_ack(&ack(&bad_rows[1..])).is_err());
        let accepted = p.validate_ack(&ack(&actual)).unwrap();
        let evidence = accepted.evidence().unwrap();
        let canonical = fresh(&s, &actual);
        accepted.check_confirmation(&canonical).unwrap();
        assert_eq!(
            record
                .review_current(&canonical)
                .unwrap()
                .preview_blocks()
                .unwrap(),
            preview.iter().find(|v| v.id == "item").unwrap().blocks
        );
        for failure in 0..3 {
            let mut changed_read = canonical.clone();
            let i = changed_read
                .sections
                .iter()
                .position(|s| s.id() == "child-b")
                .unwrap();
            match failure {
                0 => changed_read.sections[i] = changed(&changed_read.sections[i], n(2, 9999)),
                1 => {
                    changed_read.sections.remove(i);
                }
                _ => {
                    changed_read.sections[i] =
                        changed(&changed_read.sections[i], m(16, &raw(&[n(1, 0), n(2, 0)])))
                }
            }
            assert!(accepted.check_confirmation(&changed_read).is_err());
            assert_eq!(accepted.evidence().unwrap().section, evidence.section);
            let observed = record.review_current(&changed_read).unwrap();
            // The actual changed/missing child inventory is retained for local
            // review; it never substitutes for canonical confirmation.
            record
                .validate_prior_evidence(observed.prior_evidence().as_ref())
                .unwrap();
        }
        accepted.check_confirmation(&canonical).unwrap();
    }
}

#[path = "list_journal_tests.rs"]
mod journal;

#[test]
fn list_root_empty_policy_and_visible_existing_root_use_actual_container_branch() {
    let mut s = list_fixture(5, false);
    s.sections
        .retain(|v| !matches!(v.id(), "child-a" | "child-b"));
    s.document = s.document.replace_fields(70, &[n(70, 1)]).unwrap();
    // ListContent inherits the base section policy; TextContent's ordinary
    // empty+inline-only policy is not applied to a list container.
    let p = prepare(&s);
    assert_eq!(p.record().unwrap().snippet()["mrkdwn"], "");
    assert_eq!(p.document_data().messages(1).unwrap().len(), 1);
    add(&mut s, "existing-list-root", false, 2, false);
    let before = s.sections.clone();
    assert!(
        matches!(AnnotationIntent::for_section(&s,"item",&AnnotationPresentation::ordinary(None,None).unwrap()).unwrap(),SectionAnnotationChoice::Existing(id) if id=="existing-list-root")
    );
    assert_eq!(s.sections, before);
}

#[test]
fn list_export_inherits_parent_default_without_clearing_or_replacing_body() {
    let mut s = list_fixture(7, false);
    let i = s.sections.iter().position(|v| v.id() == "item").unwrap();
    s.sections[i] = changed(&s.sections[i], m(16, &raw(&[n(13, 2)])));
    let before = s.sections.clone();
    let p = prepare(&s);
    assert_eq!(s.sections, before);
    let rich: serde_json::Value =
        serde_json::from_str(p.record().unwrap().snippet()["richText"].as_str().unwrap()).unwrap();
    assert_eq!(
        rich["blocks"][0]["elements"][0]["elements"][0]["elements"][0]["text"],
        ""
    );
    assert!(
        p.document_data()
            .messages(1)
            .unwrap()
            .iter()
            .all(|u| u.string(1).unwrap() != Some("item"))
    );
}

#[test]
fn list_child_inline_control_is_not_recursively_removed_and_keeps_nested_placement() {
    let mut s = list_fixture(5, true);
    let i = s.sections.iter().position(|s| s.id() == "child-a").unwrap();
    s.sections[i] = changed(
        &s.sections[i],
        m(
            12,
            &raw(&[m(
                1,
                &raw(&[t(1, "one <annotation id=\"z-stub\">old</annotation>")]),
            )]),
        ),
    );
    let p = prepare(&s);
    let record = p.record().unwrap();
    let updates = p.document_data().messages(1).unwrap();
    let expected = format!("aac-{}", position::between(None, Some("aaf")).unwrap());
    assert_eq!(updates[0].string(21).unwrap(), Some(expected.as_str()));
    assert!(
        updates
            .iter()
            .all(|u| !matches!(u.string(1).unwrap(), Some("child-a" | "child-b" | "item")))
    );
    let original_child = s.section("child-a").unwrap().clone();
    let actual = responses(&p, &s);
    let canonical = fresh(&s, &actual);
    let accepted = p.validate_ack(&ack(&actual)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    assert_eq!(canonical.section("child-a"), Some(&original_child));
    assert!(
        record.snippet()["mrkdwn"]
            .as_str()
            .unwrap()
            .contains("one old")
    );
    // The old control still exists and retains all unrelated metadata.
    assert_eq!(
        canonical
            .section("z-stub")
            .unwrap()
            .raw()
            .string(91)
            .unwrap(),
        Some("unrelated original")
    );
}

#[test]
fn list_control_positions_include_deleted_loaded_siblings_without_exporting_or_writing_them() {
    let mut s = list_fixture(5, true);
    let i = s.sections.iter().position(|s| s.id() == "child-b").unwrap();
    s.sections[i] = changed(
        &s.sections[i],
        m(
            12,
            &raw(&[m(
                1,
                &raw(&[t(1, "two <annotation id=\"z-stub\">old</annotation>")]),
            )]),
        ),
    );
    let mut top = s.section("child-a").unwrap().raw().remove(13).unwrap();
    for field in [t(1, "deleted-top"), t(8, "aabb"), t(21, "aabb"), n(3, 1)] {
        top = top
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    let mut child = s.section("child-a").unwrap().raw().clone();
    for field in [
        t(1, "deleted-child"),
        t(8, "aaff"),
        t(21, "aac-aaff"),
        n(3, 1),
    ] {
        child = child
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections.push(Section::from_raw(top).unwrap());
    s.sections.push(Section::from_raw(child).unwrap());
    let before = s.sections.clone();
    let p = prepare(&s);
    let updates = p.document_data().messages(1).unwrap();
    let old_path = format!(
        "aac-{}",
        position::between(Some("aaff"), Some("aag")).unwrap()
    );
    let new_path = position::between(Some("aabb"), Some("aac")).unwrap();
    assert_eq!(updates[0].string(21).unwrap(), Some(old_path.as_str()));
    assert_eq!(
        updates.last().unwrap().string(21).unwrap(),
        Some(new_path.as_str())
    );
    assert!(
        updates
            .iter()
            .all(|u| !matches!(u.string(1).unwrap(), Some("deleted-top" | "deleted-child")))
    );
    assert_eq!(s.sections, before);
    let actual = responses(&p, &s);
    let canonical = fresh(&s, &actual);
    p.validate_ack(&ack(&actual))
        .unwrap()
        .check_confirmation(&canonical)
        .unwrap();
}

#[test]
fn list_capture_refuses_changed_hierarchy_owner_or_child_before_dispatch_and_legacy_downgrade() {
    let s = list_fixture(7, false);
    let presentation = AnnotationPresentation::ordinary(None, None).unwrap();
    for failure in 0..5 {
        let SectionAnnotationChoice::New(intent) =
            AnnotationIntent::for_section(&s, "item", &presentation).unwrap()
        else {
            panic!()
        };
        let mut now = s.clone();
        let i = now
            .sections
            .iter()
            .position(|s| s.id() == "child-a")
            .unwrap();
        now.sections[i] = changed(
            &now.sections[i],
            match failure {
                0 => n(2, 999),
                1 => t(6, "foreign"),
                2 => m(13, &raw(&[t(1, "missing"), n(4, 7)])),
                3 => m(16, &raw(&[n(1, 65)])),
                _ => t(21, "aac-wrong"),
            },
        );
        assert!(intent.prepare(&now).is_err());
    }
    let record = prepare(&s).record().unwrap();
    let original = serde_json::to_value(&record).unwrap();
    for version in 1..=7 {
        let mut forged = original.clone();
        forged["version"] = version.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(forged)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut forged = original;
    forged["section_plan"]["list_layout"] = false.into();
    assert!(
        serde_json::from_value::<AnnotationRecord>(forged)
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(AnnotationIntent::capture(&s, "item").is_err());
    assert!(AnnotationIntent::capture_range(&s, "item", 0, 1).is_err());
}

#[test]
fn deleted_list_child_keeps_original_control_containment_during_membership_clear() {
    let mut s = list_fixture(5, true);
    let i = s.sections.iter().position(|s| s.id() == "child-b").unwrap();
    s.sections[i] = changed(&s.sections[i], n(3, 1));
    s.sections[i] = changed(
        &s.sections[i],
        m(
            12,
            &raw(&[m(
                1,
                &raw(&[t(
                    1,
                    "<annotation id=\"z-stub\">retained deleted content</annotation>",
                )]),
            )]),
        ),
    );
    let before = s.sections[i].clone();
    let p = prepare(&s);
    let updates = p.document_data().messages(1).unwrap();
    let path = format!(
        "aac-{}",
        position::between(Some("aaf"), Some("aag")).unwrap()
    );
    assert_eq!(updates[0].string(21).unwrap(), Some(path.as_str()));
    assert!(
        !p.record().unwrap().snippet()["mrkdwn"]
            .as_str()
            .unwrap()
            .contains("retained deleted content")
    );
    assert!(
        updates
            .iter()
            .all(|u| u.string(1).unwrap() != Some("child-b"))
    );
    let actual = responses(&p, &s);
    let canonical = fresh(&s, &actual);
    let accepted = p.validate_ack(&ack(&actual)).unwrap();
    accepted.check_confirmation(&canonical).unwrap();
    let evidence = accepted.evidence().unwrap();
    for mode in 0..3 {
        let mut stale = canonical.clone();
        let i = stale
            .sections
            .iter()
            .position(|s| s.id() == "child-b")
            .unwrap();
        match mode {
            0 => {
                stale.sections.remove(i);
            }
            1 => stale.sections[i] = changed(&stale.sections[i], n(2, 999)),
            _ => {
                stale.sections[i] = changed(
                    &stale.sections[i],
                    m(13, &raw(&[t(1, "different-list"), n(4, 5)])),
                )
            }
        }
        assert!(accepted.check_confirmation(&stale).is_err());
        assert_eq!(accepted.evidence().unwrap().section, evidence.section);
    }
    // Arrival order is not display order; raw position paths remain exact.
    let mut reordered = canonical.clone();
    reordered.sections.reverse();
    accepted.check_confirmation(&reordered).unwrap();
    assert_eq!(canonical.section("child-b"), Some(&before));
}
