use super::*;

fn attrs(id: Option<&str>) -> RawMessage {
    let mut fields = vec![n(4, 0), n(13, 0), t(177, "opaque-a"), t(177, "opaque-b")];
    if let Some(id) = id {
        fields.insert(0, t(29, id));
    }
    let mut bytes = raw(&fields).raw().to_vec();
    bytes.extend_from_slice(&[0xc8, 0x3e, 0x81, 0x00]);
    RawMessage::decode(&bytes).unwrap()
}
fn with_attrs(s: &mut CanvasCollabSnapshot, a: &RawMessage) {
    let section = changed(s.section("item").unwrap(), m(16, a));
    replace(s, section);
}
fn tampered(record: &AnnotationRecord, a: Option<&RawMessage>) -> AnnotationRecord {
    let mut value = serde_json::to_value(record).unwrap();
    let bytes: Vec<u8> = serde_json::from_value(value["request"].clone()).unwrap();
    let request = RawMessage::decode(&bytes).unwrap();
    let mut updates = request.messages(1).unwrap();
    updates[1] = match a {
        Some(a) => updates[1].replace_fields(16, &[m(16, a)]).unwrap(),
        None => updates[1].remove(16).unwrap(),
    };
    let fields = updates.iter().map(|u| m(1, u)).collect::<Vec<_>>();
    value["request"] = serde_json::json!(request.replace_fields(1, &fields).unwrap().raw());
    serde_json::from_value(value).unwrap()
}

#[test]
fn inline_heading_content_id_full_attributes_conditionally_roundtrip_all_histories() {
    for level in 1..=3 {
        for positive in [false, true] {
            for id in [None, Some(""), Some(" "), Some("content-id")] {
                let mut s = heading(level, positive);
                let a = attrs(id);
                with_attrs(&mut s, &a);
                let before = s.sections.clone();
                let p = prepare(&s);
                let update = p.document_data().messages(1).unwrap().remove(1);
                let expected = id.filter(|id| !id.is_empty()).map(|_| a.clone());
                assert_eq!(update.message(16).unwrap(), expected);
                let record = p.record().unwrap();
                let restored: AnnotationRecord =
                    serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
                restored.validate().unwrap();
                assert!(restored == record);
                for bad in [attrs(Some("foreign")), a.remove(177).unwrap(), raw(&[])] {
                    assert!(tampered(&record, Some(&bad)).validate().is_err());
                }
                if expected.is_some() {
                    assert!(tampered(&record, None).validate().is_err());
                    let reordered = a.replace_fields(29, &[t(29, id.unwrap())]).unwrap();
                    assert!(tampered(&record, Some(&reordered)).validate().is_err());
                }
                let (control, parent) = received(&p, &s);
                assert_eq!(parent.attributes().unwrap().unwrap().raw(), a.raw());
                let accepted = p
                    .validate_ack(&ack(&[control.clone(), parent.clone()]))
                    .unwrap();
                let current = fresh(&s, control, parent);
                accepted.check_confirmation(&current).unwrap();
                assert_eq!(s.sections, before);
                for prior in before.iter().filter(|v| v.id() != "item") {
                    assert_eq!(current.section(prior.id()), Some(prior));
                }
            }
        }
    }
}

#[test]
fn inline_heading_content_id_stale_malformed_and_ack_drift_remain_refused() {
    let mut s = heading(2, true);
    with_attrs(&mut s, &attrs(Some("id")));
    let intent = AnnotationIntent::capture_range(&s, "item", 1, 4).unwrap();
    let mut changed_source = s.clone();
    with_attrs(&mut changed_source, &attrs(Some("new")));
    assert!(intent.prepare_at(&changed_source, 1_000_000).is_err());
    for a in [raw(&[n(29, 1)]), raw(&[t(29, "id"), t(29, "id")])] {
        with_attrs(&mut changed_source, &a);
        assert!(AnnotationIntent::capture_range(&changed_source, "item", 1, 4).is_err());
        assert!(
            range_seeds(&changed_source)
                .unwrap()
                .iter()
                .all(|(id, _)| id != "item")
        );
    }
    for a in [
        attrs(Some("changed")),
        attrs(Some("")),
        attrs(None),
        raw(&[]),
    ] {
        let p = prepare(&s);
        let (control, parent) = received(&p, &s);
        assert!(
            p.validate_ack(&ack(&[control, changed(&parent, m(16, &a))]))
                .is_err()
        );
        let p = prepare(&s);
        let (control, parent) = received(&p, &s);
        let accepted = p
            .validate_ack(&ack(&[control.clone(), parent.clone()]))
            .unwrap();
        let mut current = fresh(&s, control, parent);
        with_attrs(&mut current, &a);
        assert!(accepted.check_confirmation(&current).is_err());
    }
}

#[test]
fn heading_content_id_inert_journal_rejects_missing_or_changed_resend_bytes() {
    let mut s = heading(3, true);
    with_attrs(&mut s, &attrs(Some("journal-id")));
    let p = prepare(&s);
    let record = p.record().unwrap();
    let temp = Temp::new();
    let path = temp.0.join("annotation.sqlite");
    let owner = "147b5480-9f1a-4443-89da-836472cbb209";
    let mut journal = Journal::open(&path, owner).unwrap();
    assert!(journal.begin_annotation(tampered(&record, None)).is_err());
    assert!(
        journal
            .begin_annotation(tampered(&record, Some(&attrs(Some("foreign")))))
            .is_err()
    );
    let AnnotationBegin::Started(receipt) = journal.begin_annotation(record).unwrap() else {
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
    let canonical = fresh(&s, control, parent.clone());
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
    let verified = accepted.confirm(&canonical).unwrap();
    assert!(
        journal
            .annotation_anchor_confirmed(&receipt, &verified)
            .unwrap()
    );
}

#[test]
fn inline_heading_content_id_equal_and_omitted_partial_ack_attributes_merge_custody() {
    for positive in [false, true] {
        let mut s = heading(1, positive);
        with_attrs(&mut s, &attrs(Some("original")));
        for omitted in [false, true] {
            let p = prepare(&s);
            let (control, parent) = received(&p, &s);
            let received_parent = if omitted {
                Section::from_raw(parent.raw().remove(16).unwrap()).unwrap()
            } else {
                parent.clone()
            };
            let accepted = p
                .validate_ack(&ack(&[control.clone(), received_parent]))
                .unwrap();
            accepted
                .check_confirmation(&fresh(&s, control, parent))
                .unwrap();
        }
    }
}
