use super::*;
use crate::canvas::edit::structure::split::validation;
use crate::canvas::edit::structure::{
    Side, SplitIntent, SplitWitness,
    recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    },
};

fn originals() -> Vec<Value> {
    let v: Value = serde_json::from_str(include_str!("owner-original.json")).unwrap();
    assert_eq!(
        v["referenceBindingsSha256"],
        "93ef43f673b75047d56530de2d88aad4c07a49fe58ef96cd054a154a539327bd"
    );
    v["vectors"].as_array().unwrap().clone()
}
fn row_state(template: &Section, value: &Value) -> Section {
    let wire = decoded(&value["session"])
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    let mut row = template.clone();
    for f in [
        n(2, value["sequence"].as_u64().unwrap()),
        t(8, value["position"].as_str().unwrap()),
        t(21, value["position"].as_str().unwrap()),
        m(12, &body(value["rtml"].as_str().unwrap())),
        m(38, &wire),
    ] {
        row = change(&row, f);
    }
    row
}
fn original_pair(v: &Value) -> (CanvasCollabSnapshot, String, String) {
    let old = cases()
        .into_iter()
        .find(|c| c["mode"] == v["mode"] && c["corpus"] == v["corpus"])
        .unwrap();
    let (s, owner, child) = fixture(&old);
    let s = readback(
        &s,
        &row_state(s.section(&owner).unwrap(), &v["ownerBefore"]),
    );
    let s = readback(
        &s,
        &row_state(s.section(&child).unwrap(), &v["childBeforeFormatting"]),
    );
    (s, owner, child)
}
fn doc(row: &Section) -> Automerge {
    Automerge::load(row.crdt().unwrap().unwrap().bytes(5).unwrap().unwrap()).unwrap()
}
fn exact_prefix(before: &Section, after: &Section) {
    let a = doc(before);
    let b = doc(after);
    for change in a.get_changes(&[]) {
        assert_eq!(
            b.get_change_by_hash(&change.hash()).unwrap().raw_bytes(),
            change.raw_bytes()
        );
    }
    assert_eq!(
        before.crdt().unwrap().unwrap().message(10).unwrap(),
        after.crdt().unwrap().unwrap().message(10).unwrap()
    );
}
fn mapped(s: &CanvasCollabSnapshot, owner: &str, child: &str) {
    assert_eq!(
        split_child(&s.sections, s.section(owner).unwrap(), 100).unwrap(),
        vec![s.section(child).unwrap().clone()]
    );
    for id in [owner, child] {
        assert!(TextIntent::capture(s, id, 100).is_ok(), "capture {id}");
    }
    for a in paragraph_actions(s, &[owner, child], 100) {
        let a = a.unwrap();
        assert!(a.insert_before && a.insert_after && a.delete);
    }
}
fn insert(s: &CanvasCollabSnapshot, owner: &str) -> CanvasCollabSnapshot {
    let p = StructureIntent::insert(
        s,
        owner,
        Side::After,
        &TextDraft::from_rtml("Independent gap").unwrap(),
        100,
    )
    .unwrap()
    .prepare(s, 100)
    .unwrap();
    p.checkpoint().unwrap();
    let row = canonical(s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&row)).unwrap();
    let next = readback(s, &row);
    assert!(accepted.confirms_readback(&next).unwrap());
    next
}
fn save(s: &CanvasCollabSnapshot, id: &str, value: &str) -> CanvasCollabSnapshot {
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(
            s,
            &mut EditSession::new(),
            &TextDraft::from_rtml(value).unwrap(),
            100,
        )
        .unwrap();
    let row = canonical(s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&row)).unwrap();
    let next = readback(s, &row);
    assert!(accepted.confirms_readback(&next).unwrap());
    exact_prefix(s.section(id).unwrap(), &row);
    for old in &s.sections {
        if old.id() != id {
            assert_eq!(next.section(old.id()), Some(old));
        }
    }
    next
}

#[test]
fn six_original_coalesced_owner_rich_histories_and_removals_keep_exact_boundary_and_peers() {
    let vectors = originals();
    assert_eq!(vectors.len(), 6);
    for v in vectors {
        let (source, owner, child) = original_pair(&v);
        let before = source.section(&owner).unwrap();
        let split: automerge::ChangeHash = v["originalTruncationHead"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let saved: automerge::ChangeHash = v["firstProcessedPostTruncation"]["head"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(doc(before).get_heads(), [saved]);
        assert_eq!(
            doc(before).get_changes(&[]).len(),
            v["firstProcessedPostTruncation"]["plainPrefixLength"]
                .as_u64()
                .unwrap() as usize
        );
        for with_gap in [false, true] {
            let mut current = if with_gap {
                insert(&source, &owner)
            } else {
                source.clone()
            };
            for key in ["ownerAfterFormattedSave", "ownerAfterRemovedSave"] {
                let row = row_state(current.section(&owner).unwrap(), &v[key]);
                exact_prefix(before, &row);
                assert!(doc(&row).get_change_by_hash(&split).is_some());
                assert!(
                    !validation::processed(&row.crdt().unwrap().unwrap(), &split.to_string())
                        .unwrap()
                );
                assert!(
                    validation::processed(&row.crdt().unwrap().unwrap(), &saved.to_string())
                        .unwrap()
                );
                let previous = current.clone();
                current = readback(&current, &row);
                split_child(&current.sections, &row, 100).unwrap_or_else(|e| {
                    panic!(
                        "{}/{}/{key}/gap={with_gap}: {e:?}",
                        v["corpus"], v["cadence"]
                    )
                });
                mapped(&current, &owner, &child);
                for old in previous.sections.iter().filter(|r| r.id() != owner) {
                    assert_eq!(current.section(old.id()), Some(old));
                }
                if key == "ownerAfterRemovedSave" {
                    let d = doc(&row);
                    let (_, text) = d.get(ROOT, "text").unwrap().unwrap();
                    assert!(
                        d.marks(&text)
                            .unwrap()
                            .iter()
                            .any(|m| m.value() == &automerge::ScalarValue::Boolean(false))
                    );
                }
            }
        }
    }
}

fn prepared_pair(v: &Value) -> (CanvasCollabSnapshot, String, String, automerge::ChangeHash) {
    let (mut s, owner, child) = original_pair(v);
    s.sections.retain(|r| r.id() != child);
    let original = row_state(s.section(&owner).unwrap(), &v["originalSource"]);
    s = readback(&s, &original);
    let before = text::canonical_draft(&original).unwrap();
    let left = TextDraft::from_rtml(v["left"].as_str().unwrap()).unwrap();
    let tail = TextDraft::from_rtml(v["tail"].as_str().unwrap()).unwrap();
    let offset = left
        .runs
        .iter()
        .map(|r| r.text.encode_utf16().count())
        .sum();
    let reports = v["parentReports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| TextDraft::from_rtml(t.as_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    let witness = SplitWitness::new(&before, offset, &left, &tail)
        .unwrap()
        .with_parent_updates(&reports)
        .unwrap();
    let p = SplitIntent::capture(&s, &owner, 100)
        .unwrap()
        .prepare(&s, &witness, 100)
        .unwrap();
    p.checkpoint().unwrap();
    let child = p.child_section().to_owned();
    let rows = p
        .document_data()
        .messages(1)
        .unwrap()
        .iter()
        .map(|u| canonical(&s, u))
        .collect::<Vec<_>>();
    let data = msg(&rows.iter().map(|r| m(1, r.raw())).collect::<Vec<_>>());
    let a = EditAck::decode(msg(&[n(1, 1), m(2, &data)]).raw()).unwrap();
    let accepted = p.validate_ack(&a).unwrap();
    for row in rows {
        s = readback(&s, &row);
    }
    assert!(accepted.confirms_readback(&s).unwrap());
    let d = doc(s.section(&owner).unwrap());
    let original_heads = doc(&original).get_heads();
    let split = d.get_changes(&original_heads)[0].hash();
    assert!(
        !validation::processed(
            &s.section(&owner).unwrap().crdt().unwrap().unwrap(),
            &split.to_string()
        )
        .unwrap()
    );
    (s, owner, child, split)
}

fn delete_review(s: &CanvasCollabSnapshot, owner: &str, child: &str) {
    let p = prepare(s, owner);
    let checkpoint = p.checkpoint().unwrap();
    let checkpoint: Checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert_eq!(
        checkpoint.split_owner_child().unwrap().as_ref(),
        s.section(child)
    );
    let row = canonical(s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&row)).unwrap();
    let current = readback(s, &row);
    assert!(accepted.confirms_readback(&current).unwrap());
    Inventory::capture(&current).unwrap();
    for old in s.sections.iter().filter(|r| r.id() != owner) {
        assert_eq!(current.section(old.id()), Some(old));
    }
    let path = std::env::temp_dir().join(format!(
        "coalesced-owner-rich-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let review = journal.local_review(&stored, &current).unwrap();
    assert_eq!(review.split_owner_child(), s.section(child));
    drop(review);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_parent_coalescence_then_owner_rich_ack_removal_gap_and_delete_recovery() {
    for v in originals() {
        let (source, owner, child, split) = prepared_pair(&v);
        for with_gap in [false, true] {
            let s = if with_gap {
                insert(&source, &owner)
            } else {
                source.clone()
            };
            let rich = save(&s, &owner, v["formatted"].as_str().unwrap());
            if v["cadence"] == "erase" {
                // One original report inserts text, then commits its mark. The
                // intermediate text head is historical, never processed.
                let before = doc(s.section(&owner).unwrap());
                let after = doc(rich.section(&owner).unwrap());
                let suffix = after.get_changes(&before.get_heads());
                assert_eq!(suffix.len(), 2, "erase/{}/gap={with_gap}", v["corpus"]);
                let intermediate = after.fork_at(&[suffix[0].hash()]).unwrap();
                let plain = TextDraft::from_rtml(v["plain"].as_str().unwrap()).unwrap();
                text::crdt::check_document_state(&intermediate, &plain, false).unwrap();
                let wire = rich.section(&owner).unwrap().crdt().unwrap().unwrap();
                assert!(!validation::processed(&wire, &suffix[0].hash().to_string()).unwrap());
                assert!(validation::processed(&wire, &suffix[1].hash().to_string()).unwrap());
                assert_eq!(suffix[1].deps(), &[suffix[0].hash()]);
                assert_eq!(suffix[1].seq(), suffix[0].seq() + 1);
                assert_eq!(suffix[1].actor_id(), suffix[0].actor_id());
            }
            mapped(&rich, &owner, &child);
            assert!(
                !validation::processed(
                    &rich.section(&owner).unwrap().crdt().unwrap().unwrap(),
                    &split.to_string()
                )
                .unwrap()
            );
            delete_review(&rich, &owner, &child);
            let plain = save(&rich, &owner, v["plain"].as_str().unwrap());
            mapped(&plain, &owner, &child);
            let next = save(&plain, &child, "Child still editable");
            mapped(&next, &owner, &child);
            exact_prefix(
                source.section(&owner).unwrap(),
                next.section(&owner).unwrap(),
            );
        }
    }
}

#[test]
fn coalesced_owner_rich_rejects_false_boundary_hidden_history_and_foreign_component() {
    let v = &originals()[0];
    let (source, owner, child) = original_pair(v);
    let rich = save(&source, &owner, v["formatted"].as_str().unwrap());
    let row = rich.section(&owner).unwrap();
    let wire = row.crdt().unwrap().unwrap();
    let boundary = v["firstProcessedPostTruncation"]["head"].as_str().unwrap();
    let retained = wire
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            RawMessage::decode(wire.encoded_field(f).unwrap())
                .unwrap()
                .string(6)
                .unwrap()
                .unwrap()
                .to_owned()
        })
        .filter(|h| h != boundary)
        .map(|h| t(6, &h))
        .collect::<Vec<_>>();
    let missing_boundary = change(row, m(38, &wire.replace_fields(6, &retained).unwrap()));
    assert!(
        TextIntent::capture(&readback(&rich, &missing_boundary), &owner, 100).is_err(),
        "first saved plain boundary missing"
    );
    let unknown = wire.replace_fields(6, &[t(6, &"f".repeat(64))]).unwrap();
    assert!(
        TextIntent::capture(&readback(&rich, &change(row, m(38, &unknown))), &owner, 100).is_err(),
        "unknown processed boundary"
    );
    // The complete original prefix is part of lineage, not merely current text.
    let info = wire.message(10).unwrap().unwrap();
    let false_origin = info
        .replace_fields(7, &[F::bytes(7, &doc(row).save()).unwrap()])
        .unwrap();
    let false_wire = wire.replace_fields(10, &[m(10, &false_origin)]).unwrap();
    assert!(
        TextIntent::capture(
            &readback(&rich, &change(row, m(38, &false_wire))),
            &owner,
            100
        )
        .is_err(),
        "false original source"
    );
    for kind in 0..3 {
        let mut d = doc(row).with_actor("55555555555500000000".parse().unwrap());
        let (_, object) = d.get(ROOT, "text").unwrap().unwrap();
        let mut tx = d.transaction();
        match kind {
            0 => {
                tx.put(ROOT, "hidden", true).unwrap();
                tx.delete(ROOT, "hidden").unwrap();
            }
            1 => {
                tx.mark(
                    &object,
                    Mark::new("unknown".into(), false, 0, 1),
                    ExpandMark::After,
                )
                .unwrap();
            }
            _ => {
                tx.put_object(ROOT, "hidden", automerge::ObjType::Map)
                    .unwrap();
                tx.delete(ROOT, "hidden").unwrap();
            }
        }
        tx.commit();
        assert!(
            TextIntent::capture(&altered_child(&rich, &owner, d), &owner, 100).is_err(),
            "hidden operation case {kind}"
        );
    }
    // Two individually plausible mark branches, subsequently merged into one
    // head, are still concurrent history rather than linear CONTENT reports.
    let mut a = doc(row).with_actor("44444444444400000000".parse().unwrap());
    let mut b = a.fork().with_actor("33333333333300000000".parse().unwrap());
    for d in [&mut a, &mut b] {
        let (_, object) = d.get(ROOT, "text").unwrap().unwrap();
        let mut tx = d.transaction();
        tx.mark(
            &object,
            Mark::new("b".into(), true, 0, 1),
            ExpandMark::After,
        )
        .unwrap();
        tx.commit();
    }
    a.merge(&mut b).unwrap();
    let (_, object) = a.get(ROOT, "text").unwrap().unwrap();
    let mut tx = a.transaction();
    tx.splice_text(&object, 0, 0, "x").unwrap();
    tx.splice_text(&object, 0, 1, "").unwrap();
    tx.commit();
    assert_eq!(a.get_heads().len(), 1);
    assert!(
        TextIntent::capture(&altered_child(&rich, &owner, a), &owner, 100).is_err(),
        "concurrent ancestry with merged final head"
    );
    let foreign_info = info.replace_fields(3, &[t(3, &owner)]).unwrap();
    let foreign_wire = wire.replace_fields(10, &[m(10, &foreign_info)]).unwrap();
    let foreign = change(
        &change(&change(row, t(1, "foreign")), t(8, "aai")),
        m(38, &foreign_wire),
    );
    let foreign = readback(&rich, &foreign);
    assert!(
        split_child(&foreign.sections, row, 100).is_err(),
        "incoming component edge"
    );
    let altered_peer = change(
        rich.section(&child).unwrap(),
        m(12, &body("Unrelated later child")),
    );
    let captured = TextIntent::capture(&rich, &owner, 100).unwrap();
    assert!(
        captured
            .prepare(
                &readback(&rich, &altered_peer),
                &mut EditSession::new(),
                &TextDraft::from_rtml("Later owner").unwrap(),
                100
            )
            .is_err(),
        "stale exact peer"
    );
}

#[test]
fn two_split_owner_keeps_plain_coalesced_policy() {
    use crate::canvas::edit::structure::SplitSequenceTarget;
    let v = &originals()[0];
    let (mut s, owner, child) = original_pair(v);
    s.sections.retain(|r| r.id() != child);
    let original = row_state(s.section(&owner).unwrap(), &v["originalSource"]);
    s = readback(&s, &original);
    let draft = |s: &str| TextDraft::from_rtml(s).unwrap();
    let w = SplitWitness::new(
        &text::canonical_draft(&original).unwrap(),
        9,
        &draft("Keep this"),
        &draft("Second paragraph"),
    )
    .unwrap()
    .with_parent_updates(&[draft("Keep this revised")])
    .unwrap()
    .with_second_split(
        SplitSequenceTarget::Original,
        &draft("Keep this revised"),
        5,
        &draft("Keep "),
        &draft("this revised"),
    )
    .unwrap()
    .with_final_updates(&[draft("Keep final")], &[], &[])
    .unwrap();
    let p = SplitIntent::capture(&s, &owner, 100)
        .unwrap()
        .prepare(&s, &w, 100)
        .unwrap();
    let rows = p
        .document_data()
        .messages(1)
        .unwrap()
        .iter()
        .map(|u| canonical(&s, u))
        .collect::<Vec<_>>();
    let data = msg(&rows.iter().map(|r| m(1, r.raw())).collect::<Vec<_>>());
    let accepted = p
        .validate_ack(&EditAck::decode(msg(&[n(1, 1), m(2, &data)]).raw()).unwrap())
        .unwrap();
    for row in rows {
        s = readback(&s, &row);
    }
    assert!(accepted.confirms_readback(&s).unwrap());
    let edit = TextIntent::capture(&s, &owner, 100)
        .unwrap()
        .prepare(
            &s,
            &mut EditSession::new(),
            &draft("<b>Keep final</b>"),
            100,
        )
        .unwrap();
    let row = canonical(&s, &edit.document_data().message(1).unwrap().unwrap());
    assert!(
        edit.validate_ack(&ack(&row)).is_err(),
        "two-split history must not select new single-pair rich fallback"
    );
}
