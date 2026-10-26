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
fn changed(s: &Section, field: F) -> Section {
    Section::from_raw(s.raw().merge_partial(&raw(&[field])).unwrap()).unwrap()
}
fn fixture() -> CanvasCollabSnapshot {
    let mut s = super::super::tests::fixture();
    s.sections.retain(|section| section.id() != "other");
    s
}
fn intent(s: &CanvasCollabSnapshot) -> ReplacementIntent {
    ReplacementIntent::fixture(s, "item", true, true, 100).unwrap()
}
fn prepared(s: &CanvasCollabSnapshot) -> PreparedReplacement {
    intent(s).prepare(s, 100).unwrap()
}
fn canonical(p: &PreparedStructureEdit, snapshot: &CanvasCollabSnapshot) -> Section {
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let old = snapshot.section(update.id());
    let sequence = old.and_then(|s| s.sequence().unwrap()).unwrap_or(0) + 1000;
    let mut result = old.map_or_else(|| update.clone(), |s| s.merge_partial(&update).unwrap());
    for field in [
        n(2, sequence),
        t(6, snapshot.thread_id()),
        t(7, snapshot.document_id()),
    ] {
        result = changed(&result, field);
    }
    if let Some(position) = result.raw().string(8).unwrap().map(str::to_owned) {
        if position.ends_with(":temp") {
            result = changed(&result, t(8, position::base(&position).unwrap()));
            result = changed(&result, t(21, position::base(&position).unwrap()));
        }
    }
    let wire = result
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    changed(&result, m(38, &wire))
}
fn pair(p: &PreparedReplacement, s: &CanvasCollabSnapshot) -> [Section; 2] {
    [canonical(&p.deletion, s), canonical(&p.insertion, s)]
}
fn ack(sections: &[Section]) -> EditAck {
    let data = raw(&sections.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>());
    EditAck::decode(raw(&[n(1, 1), m(2, &data)]).raw()).unwrap()
}
fn readback(s: &CanvasCollabSnapshot, pair: &[Section; 2]) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    s.sections.retain(|s| s.id() != "item");
    s.sections.extend(pair.iter().cloned());
    s
}

#[test]
fn synthetic_other_mode_policy_refuses_and_keeps_single_delete_guard() {
    let s = fixture();
    assert!(StructureIntent::delete(&s, "item", 100).is_err());
    for (hidden, disabled) in [(false, true), (true, false), (false, false)] {
        assert!(ReplacementIntent::fixture(&s, "item", hidden, disabled, 100).is_err());
    }
    assert!(
        ReplacementIntent::fixture(&super::super::tests::fixture(), "item", true, true, 100)
            .is_err()
    );
    let mut nested = s.clone();
    nested.sections[0] = changed(&nested.sections[0], m(13, &raw(&[t(1, "parent")])));
    assert!(ReplacementIntent::fixture(&nested, "item", true, true, 100).is_err());
    let mut swapped = s.clone();
    let i = swapped
        .sections
        .iter()
        .position(|s| s.id() == "title")
        .unwrap();
    swapped.sections[i] = changed(&swapped.sections[i], t(8, "aaz"));
    assert!(ReplacementIntent::fixture(&swapped, "item", true, true, 100).is_err());
}

#[test]
fn pair_is_one_ordered_request_with_shared_session_and_two_distinct_histories() {
    let s = fixture();
    let p = prepared(&s);
    assert_eq!(p.deletion.session_id(), p.insertion.session_id());
    let updates = p.document_data().messages(1).unwrap();
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].string(1).unwrap(), Some("item"));
    assert_eq!(updates[0].boolean(3).unwrap(), Some(true));
    assert_eq!(updates[1].boolean(3).unwrap(), Some(false));
    assert_eq!(
        updates[0].string(33).unwrap(),
        updates[1].string(33).unwrap()
    );
    assert_eq!(updates[1].string(8).unwrap(), Some("aad:temp"));
    assert_eq!(
        updates[1].message(38).unwrap().unwrap().uint(2).unwrap(),
        Some(0)
    );
    assert_ne!(
        updates[0].message(38).unwrap(),
        updates[1].message(38).unwrap()
    );
    assert_eq!(s.sections, fixture().sections);
    let c = p.checkpoint().unwrap();
    assert!(c.is_replacement());
    assert!(!c.is_delete());
}

#[test]
fn captured_source_pair_has_matching_projection_and_crdt_semantics() {
    let vector: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/source-pair.json")).unwrap();
    let bytes =
        crate::slack::api::decode_canvas_base64(vector["requestBase64"].as_str().unwrap()).unwrap();
    let source = DocumentData::decode(&bytes).unwrap();
    let p = prepared(&fixture());
    let native = DocumentData::from_raw(p.document_data().clone()).unwrap();
    assert_eq!(source.sections().len(), 2);
    for (index, (a, b)) in source.sections().iter().zip(native.sections()).enumerate() {
        assert_eq!(
            a.raw().fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
            b.raw().fields().iter().map(|f| f.tag()).collect::<Vec<_>>()
        );
        assert_eq!(a.deleted().unwrap(), b.deleted().unwrap());
        let draft = if index == 0 {
            TextDraft::from_rtml("&amp;&lt;<br>\u{1f600}<b>X</b>").unwrap()
        } else {
            TextDraft::default()
        };
        crdt::validate(
            &a.crdt().unwrap().unwrap(),
            a,
            &draft,
            index == 0,
            None,
            index != 0,
        )
        .unwrap();
    }
}

#[test]
fn pair_rechecks_complete_inventory_before_preparation_and_dispatch() {
    let s = fixture();
    for field in [t(1001, "unknown altered"), t(8, "aae"), n(2, 10100)] {
        let mut changed_snapshot = s.clone();
        changed_snapshot.sections[0] = changed(&changed_snapshot.sections[0], field);
        assert!(intent(&s).prepare(&changed_snapshot, 100).is_err());
        assert!(
            prepared(&s)
                .validate_snapshot(&changed_snapshot, 100)
                .is_err()
        );
    }
}

#[test]
fn only_a_complete_direct_pair_can_create_a_receipt_in_either_ack_order() {
    let s = fixture();
    for reversed in [false, true] {
        let p = prepared(&s);
        let mut rows = pair(&p, &s);
        if reversed {
            rows.reverse();
        }
        assert!(p.validate_ack(&ack(&rows)).is_ok());
    }
    for variant in 0..7 {
        let p = prepared(&s);
        let rows = pair(&p, &s);
        let mut members = rows.to_vec();
        match variant {
            0 => {
                members.pop();
            }
            1 => {
                members.remove(0);
            }
            2 => members[0] = members[1].clone(),
            3 => members.push(rows[0].clone()),
            4 => members[1] = changed(&members[1], t(1, "foreign")),
            _ => {}
        }
        let data = raw(&members.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>());
        let mut response = raw(&[n(1, 1), m(2, &data)]);
        if variant == 5 {
            response = response.replace_fields(1, &[n(1, 0)]).unwrap();
        }
        if variant == 6 {
            response = response.replace_fields(1001, &[n(1001, 1)]).unwrap();
        }
        if matches!(variant, 2 | 3) {
            assert!(
                matches!(
                    EditAck::decode(response.raw()),
                    Err(crate::canvas::wire::Error::Duplicate)
                ),
                "duplicate variant {variant}"
            );
        } else {
            assert!(
                p.validate_ack(&EditAck::decode(response.raw()).unwrap())
                    .is_err(),
                "variant {variant}"
            );
        }
    }
}

#[test]
fn each_members_exact_identity_revision_sent_head_and_history_remain_mandatory() {
    let s = fixture();
    for index in 0..2 {
        for variant in 0..8 {
            let p = prepared(&s);
            let mut rows = pair(&p, &s);
            rows[index] = match variant {
                0 => changed(&rows[index], n(20, 2)),
                1 => changed(&rows[index], t(7, "different-document")),
                2 => Section::from_raw(rows[index].raw().remove(38).unwrap()).unwrap(),
                7 => changed(&rows[index], t(33, "different-0")),
                _ => {
                    let mut wire = rows[index].crdt().unwrap().unwrap();
                    wire = match variant {
                        3 => wire.remove(6).unwrap(),
                        4 => wire
                            .replace_fields(5, &[F::bytes(5, b"invalid history").unwrap()])
                            .unwrap(),
                        5 => wire.replace_fields(7, &[n(7, 1)]).unwrap(),
                        _ => wire.replace_fields(1, &[t(1, "foreign")]).unwrap(),
                    };
                    changed(&rows[index], m(38, &wire))
                }
            };
            assert!(
                p.validate_ack(&ack(&rows)).is_err(),
                "member {index} variant {variant}"
            );
        }
    }
}

#[test]
fn whole_pair_readback_requires_new_history_and_exact_old_tombstone_or_sealed_absence() {
    let s = fixture();
    let p = prepared(&s);
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let canonical = readback(&s, &rows);
    assert!(accepted.confirms_readback(&canonical).unwrap());
    let mut absent = canonical.clone();
    absent.sections.retain(|s| s.id() != "item");
    assert!(accepted.confirms_readback(&absent).unwrap());
    let mut missing_new = canonical.clone();
    missing_new.sections.pop();
    assert!(!accepted.confirms_readback(&missing_new).unwrap());
    for id in [rows[0].id(), rows[1].id()] {
        let mut missing_history = canonical.clone();
        let section = missing_history
            .sections
            .iter_mut()
            .find(|s| s.id() == id)
            .unwrap();
        *section = Section::from_raw(section.raw().remove(38).unwrap()).unwrap();
        assert!(!accepted.confirms_readback(&missing_history).unwrap());
    }
    let mut duplicate = canonical.clone();
    duplicate.sections.push(rows[1].clone());
    assert!(!accepted.confirms_readback(&duplicate).unwrap());
    let mut changed_unknown = canonical.clone();
    changed_unknown.sections[0] = changed(&changed_unknown.sections[0], t(1001, "different"));
    assert!(!accepted.confirms_readback(&changed_unknown).unwrap());
    let mut changed_old = canonical;
    let i = changed_old
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    changed_old.sections[i] = changed(&changed_old.sections[i], t(1001, "different"));
    assert!(!accepted.confirms_readback(&changed_old).unwrap());
}

mod journal;

mod local_review;

mod retained;

mod native_opaque;

mod durable;
