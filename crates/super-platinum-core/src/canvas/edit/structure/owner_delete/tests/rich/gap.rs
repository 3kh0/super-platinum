use super::*;
use crate::canvas::edit::structure::{
    Side,
    recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    },
};

#[track_caller]
fn association(s: &CanvasCollabSnapshot, parent: &str, child: &str) {
    assert_eq!(
        split_child(&s.sections, s.section(parent).unwrap(), 100).unwrap(),
        vec![s.section(child).unwrap().clone()]
    );
    assert!(TextIntent::capture(s, parent, 100).is_ok());
    assert!(TextIntent::capture(s, child, 100).is_ok());
    for action in paragraph_actions(s, &[parent, child], 100) {
        assert!(action.is_some());
    }
}

#[test]
fn original_twelve_multigap_vectors_preserve_live_pair_and_exact_rich_gap_tombstone() {
    let evidence: Value = serde_json::from_str(include_str!("gap-original.json")).unwrap();
    let vectors = evidence["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 12);
    for v in vectors {
        let original = cases()
            .into_iter()
            .find(|c| c["mode"] == v["mode"] && c["corpus"] == v["corpus"])
            .unwrap();
        let (base, parent, child) = fixture(&original);
        let template = base.section(&child).unwrap();
        let mut last_owner = None;
        for stage in v["stages"].as_array().unwrap() {
            let mut s = base.clone();
            s.sections.retain(|r| r.id() == "title");
            for value in stage["rows"].as_array().unwrap() {
                let wire = decoded(&value["session"])
                    .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
                    .unwrap();
                let mut row = template.clone();
                for field in [
                    t(1, value["id"].as_str().unwrap()),
                    n(2, value["sequence"].as_u64().unwrap()),
                    n(3, u64::from(value["deleted"].as_bool().unwrap())),
                    t(8, value["position"].as_str().unwrap()),
                    t(21, value["position"].as_str().unwrap()),
                    m(12, &body(value["rtml"].as_str().unwrap())),
                    m(38, &wire),
                ] {
                    row = change(&row, field);
                }
                if row.id() != parent && row.id() != child {
                    // Scope/known ordinary attributes are synthetic adapters;
                    // retain the reference session, body and position verbatim.
                    row = change(&row, m(16, &msg(&[n(4, 0)])));
                    // The shared item template deliberately carries tag1001
                    // as an opaque preservation sentinel. It is not present
                    // in these original independent-row vectors and must not
                    // fabricate unknown gap metadata. Keep it on both peers.
                    row = Section::from_raw(row.raw().remove(1001).unwrap()).unwrap();
                }
                s.sections.push(row);
            }
            Inventory::capture(&s).unwrap_or_else(|e| {
                panic!(
                    "{}/{}/{}/{}: {e:?}",
                    v["mode"], v["corpus"], v["cadence"], stage["label"]
                )
            });
            assert_eq!(
                split_child(&s.sections, s.section(&parent).unwrap(), 100).unwrap_or_else(
                    |e| panic!(
                        "{}/{}/{}/{} association: {e:?}",
                        v["mode"], v["corpus"], v["cadence"], stage["label"]
                    )
                ),
                vec![s.section(&child).unwrap().clone()]
            );
            association(&s, &parent, &child);
            if let Some(owner) = last_owner.as_ref() {
                assert_eq!(s.section(&parent), Some(owner));
            }
            last_owner = s.section(&parent).cloned();
            let wire = s.section(&child).unwrap().crdt().unwrap().unwrap();
            assert!(
                !super::super::super::super::split::validation::processed(
                    &wire,
                    v["originalTailHead"].as_str().unwrap()
                )
                .unwrap()
            );
            for row in s
                .sections
                .iter()
                .filter(|r| r.deleted().unwrap() == Some(true))
            {
                assert!(paragraph_actions(&s, &[row.id()], 100)[0].is_none());
                assert!(TextIntent::capture(&s, row.id(), 100).is_err());
                assert!(super::super::super::super::split::gap_tombstone(&s.sections, row).is_ok());
            }
        }
    }
}

fn insert(
    s: &CanvasCollabSnapshot,
    anchor: &str,
    side: Side,
    value: &str,
) -> (CanvasCollabSnapshot, String) {
    let prepared =
        StructureIntent::insert(s, anchor, side, &TextDraft::from_rtml(value).unwrap(), 100)
            .unwrap()
            .prepare(s, 100)
            .unwrap();
    prepared.checkpoint().unwrap();
    let row = canonical(s, &prepared.document_data().message(1).unwrap().unwrap());
    let id = row.id().to_owned();
    let accepted = prepared.validate_ack(&ack(&row)).unwrap();
    let next = readback(s, &row);
    assert!(accepted.confirms_readback(&next).unwrap());
    for old in &s.sections {
        assert_eq!(next.section(old.id()), Some(old));
    }
    (next, id)
}

fn edit(s: &CanvasCollabSnapshot, id: &str, value: &str) -> CanvasCollabSnapshot {
    let prepared = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(
            s,
            &mut EditSession::new(),
            &TextDraft::from_rtml(value).unwrap(),
            100,
        )
        .unwrap();
    let row = canonical(s, &prepared.document_data().message(1).unwrap().unwrap());
    let accepted = prepared.validate_ack(&ack(&row)).unwrap();
    let next = readback(s, &row);
    assert!(accepted.confirms_readback(&next).unwrap());
    for old in &s.sections {
        if old.id() != id {
            assert_eq!(next.section(old.id()), Some(old));
        } else {
            let a = old.crdt().unwrap().unwrap();
            let b = row.crdt().unwrap().unwrap();
            let before = Automerge::load(a.bytes(5).unwrap().unwrap()).unwrap();
            let after = Automerge::load(b.bytes(5).unwrap().unwrap()).unwrap();
            assert!(
                before
                    .get_changes(&[])
                    .iter()
                    .all(|c| after.get_change_by_hash(&c.hash()).is_some())
            );
        }
    }
    next
}

#[track_caller]
fn delete(s: &CanvasCollabSnapshot, id: &str) -> (CanvasCollabSnapshot, Checkpoint) {
    let prepared = prepare(s, id);
    let checkpoint = prepared
        .checkpoint()
        .unwrap_or_else(|e| panic!("delete {id} checkpoint: {e:?}"));
    let checkpoint: Checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    // Recovery must re-prove retained ordinary tombstones, not merely accept
    // their deleted flag after the Inventory path has done so once.
    for (index, old) in s.sections.iter().enumerate() {
        if old.deleted().unwrap() == Some(true) && !old.crdt().unwrap().unwrap().has(10) {
            let wire = old.crdt().unwrap().unwrap().remove(5).unwrap();
            let malformed = change(old, m(38, &wire));
            let mut encoded = serde_json::to_value(&checkpoint).unwrap();
            encoded["sections"][index] = serde_json::to_value(malformed.raw().raw()).unwrap();
            let invalid: Checkpoint = serde_json::from_value(encoded).unwrap();
            assert!(
                invalid.split_owner_child().is_err(),
                "{id}: tombstone history must survive recovery validation"
            );
        }
    }
    let row = canonical(s, &prepared.document_data().message(1).unwrap().unwrap());
    let accepted = prepared.validate_ack(&ack(&row)).unwrap();
    let next = readback(s, &row);
    assert!(accepted.confirms_readback(&next).unwrap());
    Inventory::capture(&next).unwrap();
    for old in &s.sections {
        if old.id() != id {
            assert_eq!(next.section(old.id()), Some(old));
        }
    }
    (next, checkpoint)
}

fn review(checkpoint: Checkpoint, current: &CanvasCollabSnapshot) {
    let path = std::env::temp_dir().join(format!("split-gap-{}.sqlite", uuid::Uuid::new_v4()));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let local = journal.local_review(&stored, current).unwrap();
    assert!(
        local
            .observed_sections()
            .any(|r| r.deleted().unwrap() == Some(true))
    );
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_some());
    drop(local);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn repeated_both_insert_routes_keep_all_edits_and_both_gap_owner_deletion_orders() {
    for v in vectors().into_iter().filter(|v| {
        v["mode"] == "retained-positive" && v["format"] == "bold" && v["removeFormatting"] == false
    }) {
        let (s, parent, child) = source(&v);
        let s = edit(&s, &child, v["finalRtml"].as_str().unwrap());
        let (s, first) = insert(&s, &parent, Side::After, "Gap one");
        let (s, second) = insert(&s, &child, Side::Before, "Gap 日本😀 &amp; two");
        let (mut s, third) = insert(&s, &parent, Side::After, "Gap three");
        association(&s, &parent, &child);
        assert_eq!(s.sections.len(), 6);
        for (id, text) in [
            (&parent, "Owner after gap"),
            (&first, "<i>Gap one edited</i>"),
            (&second, "<b>Gap 日本😀 &amp; two edited</b>"),
            (&third, "Gap three edited"),
            (&child, "<b>Child after gaps</b>"),
        ] {
            s = edit(&s, id, text);
            association(&s, &parent, &child);
        }
        for owner_first in [false, true] {
            let (first_id, last_id) = if owner_first {
                (&parent, &second)
            } else {
                (&second, &parent)
            };
            let (next, checkpoint) = delete(&s, first_id);
            review(checkpoint, &next);
            if !owner_first {
                association(&next, &parent, &child);
            }
            let (next, checkpoint) = delete(&next, last_id);
            review(checkpoint, &next);
            assert_eq!(next.section(&child), s.section(&child));
            assert_eq!(next.section(&first), s.section(&first));
            assert!(
                paragraph_actions(&next, &[&child, &first, &third], 100)
                    .iter()
                    .all(Option::is_some)
            );
            let (again, _) = insert(&next, &child, Side::Before, "After both deletions");
            Inventory::capture(&again).unwrap();
            let live = edit(&again, &first, "<b>Gap still editable</b>");
            Inventory::capture(&live).unwrap();
        }
    }
}

#[test]
fn gap_insertion_freshness_ack_and_named_pair_fail_closed() {
    let v = vectors()
        .into_iter()
        .find(|v| v["format"] == "bold" && v["removeFormatting"] == false)
        .unwrap();
    let (s, parent, child) = source(&v);
    let (s, id) = insert(&s, &parent, Side::After, "Independent");
    association(&s, &parent, &child);
    let desired = TextDraft::from_rtml("next").unwrap();
    for changed in [&parent, &child, &id] {
        let intent = StructureIntent::insert(&s, &parent, Side::After, &desired, 100).unwrap();
        let bad = readback(&s, &change(s.section(changed).unwrap(), n(2, 9999)));
        assert!(intent.prepare(&bad, 100).is_err());
    }
    let prepared = StructureIntent::insert(&s, &child, Side::Before, &desired, 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap();
    let row = canonical(&s, &prepared.document_data().message(1).unwrap().unwrap());
    let collision = change(
        &change(
            &row,
            t(8, s.section(&id).unwrap().raw().string(8).unwrap().unwrap()),
        ),
        t(
            21,
            s.section(&id).unwrap().raw().string(8).unwrap().unwrap(),
        ),
    );
    assert!(prepared.validate_ack(&ack(&collision)).is_err());
    let prepared = StructureIntent::insert(&s, &child, Side::Before, &desired, 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap();
    let row = canonical(&s, &prepared.document_data().message(1).unwrap().unwrap());
    let accepted = prepared.validate_ack(&ack(&row)).unwrap();
    let current = readback(&s, &row);
    assert!(accepted.confirms_readback(&current).unwrap());
    let mut missing = current.clone();
    missing.sections.retain(|r| r.id() != id);
    assert!(!accepted.confirms_readback(&missing).unwrap());
    let gap = s.section(&id).unwrap();
    let expired_wire = gap
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(4, &[n(4, 99)])
        .unwrap();
    // Moving the gap onto its owner's position removes it from the open
    // interval. Association alone intentionally keeps the adjacent legacy
    // route; complete inventory and insertion source validation reject it.
    let collision = readback(&s, &change(gap, t(8, "aae")));
    assert!(
        Inventory::capture(&collision).is_err(),
        "colliding raw position"
    );
    let intent = StructureIntent::insert(&s, &child, Side::Before, &desired, 100).unwrap();
    assert!(
        intent.prepare(&collision, 100).is_err(),
        "colliding insertion source"
    );
    for (label, bad) in [
        ("foreign scope", change(gap, t(6, "foreign"))),
        ("nonordinary type", change(gap, n(9, 2))),
        ("false deletion", change(gap, n(3, 1))),
        ("unknown row field", change(gap, n(900, 1))),
        ("inherited synthetic sentinel", change(gap, n(1001, 1))),
        ("unknown attribute", change(gap, m(16, &msg(&[n(29, 1)])))),
        ("expired gap lease", change(gap, m(38, &expired_wire))),
    ] {
        let changed = readback(&s, &bad);
        assert!(
            split_child(&changed.sections, changed.section(&parent).unwrap(), 100).is_err(),
            "{label}"
        );
    }
    let owner = s.section(&parent).unwrap();
    let info = owner.crdt().unwrap().unwrap().message(10).unwrap().unwrap();
    let wire = gap
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(
            10,
            &[m(10, &info.replace_fields(3, &[t(3, &parent)]).unwrap())],
        )
        .unwrap();
    assert!(
        split_child(
            &readback(&s, &change(gap, m(38, &wire))).sections,
            owner,
            100
        )
        .is_err()
    );
    let (deleted, _) = delete(&s, &id);
    let mut orphan = deleted.clone();
    orphan.sections.retain(|r| r.id() != parent);
    assert!(Inventory::capture(&orphan).is_err());
}

#[test]
fn independent_gap_history_rejects_hidden_writes_concurrency_and_forged_tombstones() {
    let v = vectors()
        .into_iter()
        .find(|v| v["format"] == "bold" && v["removeFormatting"] == false)
        .unwrap();
    let (s, parent, _child) = source(&v);
    let (s, id) = insert(&s, &parent, Side::After, "Independent");
    let row = s.section(&id).unwrap();
    let wire = row.crdt().unwrap().unwrap();
    // Even a self-consistent one-head bootstrap with the right visible text
    // cannot smuggle a transient ROOT write into creation history.
    let mut forged = Automerge::new().with_actor("66666666666600000000".parse().unwrap());
    let mut tx = forged.transaction();
    let object = tx
        .put_object(ROOT, "text", automerge::ObjType::Text)
        .unwrap();
    tx.splice_text(&object, 0, 0, "Independent\u{feff}")
        .unwrap();
    tx.put(ROOT, "deleted", false).unwrap();
    tx.put(ROOT, "hidden", true).unwrap();
    tx.delete(ROOT, "hidden").unwrap();
    tx.commit();
    let head = forged.get_heads()[0].to_string();
    let forged_wire = wire
        .replace_fields(2, &[n(2, 1000)])
        .unwrap()
        .replace_fields(5, &[F::bytes(5, &forged.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &[t(6, &head)])
        .unwrap()
        .replace_fields(8, &[t(8, &head)])
        .unwrap();
    let bad = readback(&s, &change(row, m(38, &forged_wire)));
    assert!(split_child(&bad.sections, bad.section(&parent).unwrap(), 100).is_err());
    for kind in 0..4 {
        let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
            .unwrap()
            .with_actor("55555555555500000000".parse().unwrap());
        let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
        let mut tx = doc.transaction();
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
            2 => {
                tx.put(ROOT, "deleted", true).unwrap();
                tx.put(ROOT, "deleted", false).unwrap();
            }
            _ => {
                tx.put_object(ROOT, "hidden", automerge::ObjType::Map)
                    .unwrap();
                tx.delete(ROOT, "hidden").unwrap();
            }
        }
        tx.commit();
        let bad = altered_child(&s, &id, doc);
        assert!(split_child(&bad.sections, bad.section(&parent).unwrap(), 100).is_err());
    }
    let mut branches = Vec::new();
    for actor in ["11111111111100000000", "22222222222200000000"] {
        let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
            .unwrap()
            .with_actor(actor.parse().unwrap());
        let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
        let mut tx = doc.transaction();
        tx.mark(
            &object,
            Mark::new("b".into(), false, 0, 1),
            ExpandMark::After,
        )
        .unwrap();
        tx.commit();
        branches.push(doc);
    }
    let mut right = branches.pop().unwrap();
    let mut merged = branches.pop().unwrap();
    merged.merge(&mut right).unwrap();
    merged.set_actor("33333333333300000000".parse().unwrap());
    let (_, object) = merged.get(ROOT, "text").unwrap().unwrap();
    let mut tx = merged.transaction();
    tx.mark(
        &object,
        Mark::new("i".into(), false, 0, 1),
        ExpandMark::After,
    )
    .unwrap();
    tx.commit();
    let bad = altered_child(&s, &id, merged);
    assert!(split_child(&bad.sections, bad.section(&parent).unwrap(), 100).is_err());
    let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
        .unwrap()
        .with_actor("44444444444400000000".parse().unwrap());
    let mut tx = doc.transaction();
    tx.put(ROOT, "hidden", true).unwrap();
    tx.delete(ROOT, "hidden").unwrap();
    tx.put(ROOT, "deleted", true).unwrap();
    tx.commit();
    let bad = altered_child(&s, &id, doc);
    let bad = readback(&bad, &change(bad.section(&id).unwrap(), n(3, 1)));
    assert!(Inventory::capture(&bad).is_err());
}
