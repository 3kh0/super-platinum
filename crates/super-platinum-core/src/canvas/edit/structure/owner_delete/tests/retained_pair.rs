//! Actual retained original remaining-owner Delete; canonical adapters are inert.
use super::*;
use crate::canvas::edit::structure::{
    Side, SplitIntent,
    recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    },
};
use automerge::transaction::Transactable;

fn vectors() -> Vec<Value> {
    let v: Value = serde_json::from_str(include_str!("retained-pair-original.json")).unwrap();
    assert_eq!(
        v["bindingsSha256"],
        "69297427b08531036dbaf30055d93b49c4bb0f26aa614e063e743e910c524527"
    );
    v["vectors"].as_array().unwrap().clone()
}
fn row(s: &CanvasCollabSnapshot, encoded: &Value) -> Section {
    let mut r = Section::from_raw(decoded(encoded)).unwrap();
    let p = r.raw().string(8).unwrap().unwrap().to_owned();
    if let Some(path) = r.raw().string(21).unwrap().filter(|path| *path != p) {
        assert_eq!(
            path,
            format!("{p}:temp"),
            "only known canonical adapter path"
        );
        r = change(&r, t(21, &p));
    }
    let source_wire = r.crdt().unwrap().unwrap();
    let wire = source_wire
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    for f in [t(6, s.thread_id()), t(7, s.document_id()), m(38, &wire)] {
        r = change(&r, f);
    }
    assert_eq!(
        r.crdt().unwrap().unwrap().bytes(5).unwrap(),
        source_wire.bytes(5).unwrap()
    );
    r
}
fn snapshot(v: &Value, key: &str) -> CanvasCollabSnapshot {
    let mut s = super::super::super::tests::fixture();
    s.sections.retain(|r| r.id() == "title");
    for encoded in v[key].as_array().unwrap() {
        let r = row(&s, encoded);
        s.sections.push(r);
    }
    Inventory::capture(&s).unwrap_or_else(|e| panic!("{:?}/{key}: {e:?}", v["config"]));
    s
}
fn history(r: &Section) -> Automerge {
    Automerge::load(r.crdt().unwrap().unwrap().bytes(5).unwrap().unwrap()).unwrap()
}
fn prefix(a: &Section, b: &Section) {
    for c in history(a).get_changes(&[]) {
        assert_eq!(
            history(b)
                .get_change_by_hash(&c.hash())
                .unwrap()
                .raw_bytes(),
            c.raw_bytes()
        );
    }
}
fn inert(s: &CanvasCollabSnapshot, owner: &str, child: &str) {
    Inventory::capture(s).unwrap();
    for id in [owner, child] {
        assert_eq!(s.section(id).unwrap().deleted().unwrap(), Some(true));
        assert!(TextIntent::capture(s, id, 100).is_err());
        assert!(StructureIntent::delete(s, id, 100).is_err());
        assert!(SplitIntent::capture(s, id, 100).is_err());
        assert!(paragraph_actions(s, &[id], 100)[0].is_none());
    }
}
fn save(s: &CanvasCollabSnapshot, id: &str, rtml: &str) -> CanvasCollabSnapshot {
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(
            s,
            &mut EditSession::new(),
            &TextDraft::from_rtml(rtml).unwrap(),
            100,
        )
        .unwrap();
    let r = canonical(s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&r)).unwrap();
    let current = readback(s, &r);
    assert!(accepted.confirms_readback(&current).unwrap());
    prefix(s.section(id).unwrap(), &r);
    for old in &s.sections {
        if old.id() != id {
            assert_eq!(current.section(old.id()), Some(old));
        }
    }
    current
}

#[test]
fn original_three_remaining_owner_deletions_keep_both_full_histories_and_survivor() {
    for v in vectors() {
        let source = snapshot(&v, "ownerSourceRows");
        let both = snapshot(&v, "bothDeletedRows");
        let final_read = snapshot(&v, "finalCanonicalRows");
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        let before = source.section(owner).unwrap();
        let after = both.section(owner).unwrap();
        let peer = source.section(child).unwrap();
        assert_eq!(live_pair(&source.sections, before, 100).unwrap(), *peer);
        assert_eq!(source.section(child), both.section(child));
        prefix(before, after);
        if v["config"]["cadence"] != "none" {
            let truncation: automerge::ChangeHash =
                v["truncationHead"].as_str().unwrap().parse().unwrap();
            assert!(history(after).get_change_by_hash(&truncation).is_some());
            assert!(
                !split::validation::processed(
                    &after.crdt().unwrap().unwrap(),
                    &truncation.to_string()
                )
                .unwrap()
            );
        }
        assert_eq!(
            history(after)
                .get_changes(&history(before).get_heads())
                .len(),
            1
        );
        assert_eq!(
            before.crdt().unwrap().unwrap().messages(10).unwrap(),
            after.crdt().unwrap().unwrap().messages(10).unwrap()
        );
        let original = DocumentData::decode(decoded(&v["ownerDeleteWire"]).raw()).unwrap();
        assert_eq!(original.sections().len(), 1);
        let update = &original.sections()[0];
        assert_eq!(update.id(), owner);
        assert!(!update.raw().has(12));
        transition(
            before,
            &update.crdt().unwrap().unwrap(),
            after,
            &text::canonical_draft(before).unwrap(),
        )
        .unwrap();
        for old in &source.sections {
            if old.id() != owner {
                assert_eq!(both.section(old.id()), Some(old));
            }
        }
        assert_eq!(both.section(owner), final_read.section(owner));
        assert_eq!(both.section(child), final_read.section(child));
        assert_ne!(both.section("survivor"), final_read.section("survivor"));
        inert(&both, owner, child);
        inert(&final_read, owner, child);
    }
}

fn restart(checkpoint: Checkpoint, observed: &CanvasCollabSnapshot, child: &Section) {
    let checkpoint: Checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert_eq!(
        checkpoint.split_owner_child().unwrap().as_ref(),
        Some(child)
    );
    let path = std::env::temp_dir().join(format!("retained-pair-{}.sqlite", uuid::Uuid::new_v4()));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let review = journal.local_review(&stored, observed).unwrap();
    assert_eq!(review.split_owner_child(), Some(child));
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_some());
    drop(review);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_child_then_owner_delete_confirms_retained_pair_and_unrelated_text_insert() {
    for v in vectors() {
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        let reference = snapshot(&v, "ownerSourceRows");
        let mut source = snapshot(&v, "savedRowWires");
        source = readback(&source, reference.section("survivor").unwrap());
        let p = prepare(&source, child);
        let deleted_child = canonical(&source, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&deleted_child)).unwrap();
        let mut current = readback(&source, &deleted_child);
        assert!(accepted.confirms_readback(&current).unwrap());
        let desired = text::canonical_draft(reference.section(owner).unwrap()).unwrap();
        if text::canonical_draft(current.section(owner).unwrap()).unwrap() != desired {
            current = save(&current, owner, &desired.rtml().unwrap());
        }
        assert!(
            paragraph_actions(&current, &[owner], 100)[0]
                .as_ref()
                .unwrap()
                .delete
        );
        let p = prepare(&current, owner);
        let checkpoint = p.checkpoint().unwrap();
        assert_eq!(
            checkpoint.split_owner_child().unwrap(),
            Some(deleted_child.clone())
        );
        let deleted_owner = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&deleted_owner)).unwrap();
        let both = readback(&current, &deleted_owner);
        assert!(accepted.confirms_readback(&both).unwrap());
        prefix(current.section(owner).unwrap(), &deleted_owner);
        for old in &current.sections {
            if old.id() != owner {
                assert_eq!(both.section(old.id()), Some(old));
            }
        }
        inert(&both, owner, child);
        restart(checkpoint.clone(), &both, &deleted_child);
        for id in [owner, child] {
            let mut omitted = both.clone();
            omitted.sections.retain(|r| r.id() != id);
            assert!(
                !accepted.confirms_readback(&omitted).unwrap_or(false),
                "missing {id}"
            );
            assert!(Inventory::capture(&omitted).is_err(), "missing {id}");
            restart(checkpoint.clone(), &omitted, &deleted_child);
        }
        let current = save(&both, "survivor", "<b>Surviving paragraph edited</b>");
        let p = StructureIntent::insert(
            &current,
            "survivor",
            Side::Before,
            &TextDraft::from_rtml("New independent paragraph").unwrap(),
            100,
        )
        .unwrap()
        .prepare(&current, 100)
        .unwrap();
        p.checkpoint().unwrap();
        let added = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&added)).unwrap();
        let current = readback(&current, &added);
        assert!(accepted.confirms_readback(&current).unwrap());
        inert(&current, owner, child);
        assert_eq!(current.section(owner), Some(&deleted_owner));
        assert_eq!(current.section(child), Some(&deleted_child));
        if current.section("independent-gap").is_some() {
            let p = prepare(&current, "independent-gap");
            p.checkpoint().unwrap();
            let gap = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
            let accepted = p.validate_ack(&ack(&gap)).unwrap();
            let current = readback(&current, &gap);
            assert!(accepted.confirms_readback(&current).unwrap());
            inert(&current, owner, child);
            assert_eq!(current.section(owner), Some(&deleted_owner));
            assert_eq!(current.section(child), Some(&deleted_child));
            let current = save(&current, "survivor", "Survivor after gap deletion");
            inert(&current, owner, child);
        }
        for old in [owner, child] {
            assert_ne!(
                current.section(old).unwrap().raw().string(8).unwrap(),
                added.raw().string(8).unwrap()
            );
        }
        let current = save(&current, added.id(), "New independent paragraph edited");
        inert(&current, owner, child);
        assert_eq!(current.section(owner), Some(&deleted_owner));
        assert_eq!(current.section(child), Some(&deleted_child));
    }
}

#[test]
fn remaining_owner_refuses_last_live_unproved_neighbors_stale_or_foreign_peers() {
    let v = &vectors()[0];
    let source = snapshot(v, "ownerSourceRows");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    let mut alone = source.clone();
    alone
        .sections
        .retain(|r| ["title", owner, child].contains(&r.id()));
    Inventory::capture(&alone).unwrap();
    assert!(TextIntent::capture(&alone, owner, 100).is_ok());
    assert!(
        !paragraph_actions(&alone, &[owner], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
    assert!(StructureIntent::delete(&alone, owner, 100).is_err());
    let mut no_survivor = snapshot(v, "bothDeletedRows");
    no_survivor
        .sections
        .retain(|r| ["title", owner, child].contains(&r.id()));
    assert!(Inventory::capture(&no_survivor).is_err());

    // Two-sided merge candidates must both be proved ordinary/plain headings.
    let preceding = source.section("survivor").unwrap();
    let preceding = change(
        &change(&change(preceding, t(1, "previous")), t(8, "aac")),
        t(21, "aac"),
    );
    let mut neighbors = source.clone();
    neighbors.sections.push(preceding.clone());
    assert!(StructureIntent::delete(&neighbors, owner, 100).is_ok());
    for (label, style) in [("list", 5), ("code", 4), ("unknown", 999)] {
        let bad = readback(&neighbors, &change(&preceding, n(10, style)));
        assert!(
            StructureIntent::delete(&bad, owner, 100).is_err(),
            "{label}"
        );
    }
    for id in [owner, child] {
        let stale = change(source.section(id).unwrap(), n(2, 9999));
        assert!(
            StructureIntent::delete(&source, owner, 100)
                .unwrap()
                .prepare(&readback(&source, &stale), 100)
                .is_err(),
            "stale {id}"
        );
    }
    let mut foreign = source.section(owner).unwrap().clone();
    foreign = change(
        &change(&change(&foreign, t(1, "foreign-owner")), t(8, "aak")),
        t(21, "aak"),
    );
    let mut bad = source.clone();
    bad.sections.push(foreign);
    assert!(StructureIntent::delete(&bad, owner, 100).is_err());
    let checkpoint = prepare(&source, owner).checkpoint().unwrap();
    for id in [owner, child] {
        let mut encoded = serde_json::to_value(&checkpoint).unwrap();
        let rows = encoded["sections"].as_array_mut().unwrap();
        for raw in rows {
            let b: Vec<u8> = serde_json::from_value(raw.clone()).unwrap();
            let r = Section::decode(&b).unwrap();
            if r.id() == id {
                *raw =
                    serde_json::to_value(change(&r, m(12, &body("forged"))).raw().raw()).unwrap();
            }
        }
        let forged: Checkpoint = serde_json::from_value(encoded).unwrap();
        assert!(forged.split_owner_child().is_err(), "checkpoint {id}");
    }
}

#[test]
fn both_tombstones_reject_mutated_histories_and_bind_minimum_actual_leases() {
    let v = &vectors()[0];
    let source = snapshot(v, "ownerSourceRows");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    for id in [owner, child] {
        let r = source.section(id).unwrap();
        let wire = r
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(4, &[n(4, 150)])
            .unwrap();
        let limited = readback(&source, &change(r, m(38, &wire)));
        let p = prepare(&limited, owner);
        p.validate_dispatch_expiry(149).unwrap();
        assert!(p.validate_dispatch_expiry(150).is_err(), "lease {id}");
    }
    let p = prepare(&source, owner);
    let deleted = canonical(&source, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&deleted)).unwrap();
    let both = readback(&source, &deleted);
    for id in [owner, child] {
        let r = both.section(id).unwrap();
        let mut d = history(r).with_actor("abcdef01234500000000".parse().unwrap());
        let mut tx = d.transaction();
        tx.put(ROOT, "hidden", true).unwrap();
        tx.commit();
        let head = d.get_heads()[0].to_string();
        let wire = r
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(5, &[F::bytes(5, &d.save()).unwrap()])
            .unwrap()
            .replace_fields(6, &[t(6, &head)])
            .unwrap();
        let mutations = [
            ("hidden root", change(r, m(38, &wire))),
            ("content", change(r, m(12, &body("forged")))),
            (
                "position collision",
                change(
                    r,
                    t(
                        8,
                        source
                            .section("survivor")
                            .unwrap()
                            .raw()
                            .string(8)
                            .unwrap()
                            .unwrap(),
                    ),
                ),
            ),
            ("wrong scope", change(r, t(7, "other-document"))),
        ];
        for (label, mutated) in mutations {
            let bad = readback(&both, &mutated);
            assert!(Inventory::capture(&bad).is_err(), "{id}/{label}");
            assert!(
                !accepted.confirms_readback(&bad).unwrap_or(false),
                "{id}/{label}"
            );
            if id == owner {
                assert!(
                    prepare(&source, owner)
                        .validate_ack(&ack(&mutated))
                        .is_err(),
                    "ACK/{label}"
                );
            }
        }
    }
    // A concurrent prefix cannot become trusted just because its final change
    // is deletion-only and its visible text agrees with the canonical row.
    let mut first =
        history(source.section(owner).unwrap()).with_actor("11111111111100000000".parse().unwrap());
    let mut second = first
        .fork()
        .with_actor("22222222222200000000".parse().unwrap());
    let object = first.get(ROOT, "text").unwrap().unwrap().1;
    let mut tx = first.transaction();
    tx.splice_text(&object, 0, 0, "A").unwrap();
    tx.commit();
    let mut tx = second.transaction();
    tx.splice_text(&object, 0, 0, "B").unwrap();
    tx.commit();
    first.merge(&mut second).unwrap();
    assert_eq!(first.get_heads().len(), 2);
    let mut tx = first.transaction();
    tx.put(ROOT, "deleted", true).unwrap();
    tx.commit();
    let head = first.get_heads()[0].to_string();
    let wire = deleted
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(5, &[F::bytes(5, &first.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &[t(6, &head)])
        .unwrap();
    let content = first.text(&object).unwrap();
    let concurrent = change(
        &change(&deleted, m(38, &wire)),
        m(12, &body(content.strip_suffix('\u{feff}').unwrap())),
    );
    assert!(Inventory::capture(&readback(&both, &concurrent)).is_err());
    assert!(
        prepare(&source, owner)
            .validate_ack(&ack(&concurrent))
            .is_err()
    );
}
