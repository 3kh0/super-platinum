//! Reverse order: a retained owner remains exact while its live child is deleted.
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
    let value: Value = serde_json::from_str(include_str!("owner-first-original.json")).unwrap();
    assert_eq!(
        value["bindingsSha256"],
        "91eabfb3691fc98177f973a775b7d470ea9bd7ac049e1b60283203100518e7a6"
    );
    value["vectors"].as_array().unwrap().clone()
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
fn original_reverse_three_preserve_owner_and_exact_child_deletion_prefix() {
    for v in vectors() {
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        let mut current = snapshot(&v, "ownerDeletedRows");
        let original_owner = current.section(owner).unwrap().clone();
        assert_eq!(original_owner.deleted().unwrap(), Some(true));
        for report in v["childReports"].as_array().unwrap() {
            let next = row(&current, &report["ack"]);
            prefix(current.section(child).unwrap(), &next);
            assert_eq!(
                text::canonical_draft(&next).unwrap().rtml().unwrap(),
                report["rtml"]
            );
            current = readback(&current, &next);
            Inventory::capture(&current)
                .unwrap_or_else(|e| panic!("{:?}/report: {e:?}", v["config"]));
            assert_eq!(current.section(owner), Some(&original_owner));
        }
        let source = snapshot(&v, "childSourceRows");
        let both = snapshot(&v, "bothDeletedRows");
        let final_read = snapshot(&v, "finalCanonicalRows");
        assert_eq!(current.section(child), source.section(child));
        assert_eq!(
            split::child_delete_owner(&source.sections, source.section(child).unwrap(), 100)
                .unwrap(),
            Some(original_owner.clone())
        );
        let before = source.section(child).unwrap();
        let after = both.section(child).unwrap();
        prefix(before, after);
        assert_eq!(
            history(after)
                .get_changes(&history(before).get_heads())
                .len(),
            1
        );
        let raw = DocumentData::decode(decoded(&v["childDeleteWire"]).raw()).unwrap();
        let [update] = raw.sections() else {
            panic!("child-only report")
        };
        assert_eq!(update.id(), child);
        assert!(!update.raw().has(12));
        let outgoing = update.crdt().unwrap().unwrap();
        let acknowledged = after.crdt().unwrap().unwrap();
        let outgoing_doc = Automerge::load(outgoing.bytes(5).unwrap().unwrap()).unwrap();
        let acknowledged_doc = history(after);
        assert_eq!(
            outgoing_doc.get_heads(),
            acknowledged_doc.get_heads(),
            "{:?}/sent head",
            v["config"]
        );
        assert_eq!(
            outgoing_doc.get_changes(&[]).len(),
            acknowledged_doc.get_changes(&[]).len()
        );
        for change in outgoing_doc.get_changes(&[]) {
            assert_eq!(
                acknowledged_doc
                    .get_change_by_hash(&change.hash())
                    .unwrap()
                    .raw_bytes(),
                change.raw_bytes()
            );
        }
        assert_eq!(outgoing.uint(3).unwrap(), before.sequence().unwrap());
        assert_eq!(acknowledged.uint(3).unwrap(), after.sequence().unwrap());
        split::child_delete_transition(before, &acknowledged, after)
            .unwrap_or_else(|e| panic!("{:?}/canonical transition: {e:?}", v["config"]));
        for old in &source.sections {
            if old.id() != child {
                assert_eq!(both.section(old.id()), Some(old));
            }
        }
        for id in [owner, child] {
            assert_eq!(both.section(id), final_read.section(id));
        }
        assert_ne!(both.section("survivor"), final_read.section("survivor"));
        if v["config"]["cadence"] != "none" {
            let head: automerge::ChangeHash =
                v["truncationHead"].as_str().unwrap().parse().unwrap();
            assert!(history(&original_owner).get_change_by_hash(&head).is_some());
            assert!(
                !split::validation::processed(
                    &original_owner.crdt().unwrap().unwrap(),
                    &head.to_string()
                )
                .unwrap()
            );
        }
        inert(&both, owner, child);
        inert(&final_read, owner, child);
    }
}

fn restart(checkpoint: Checkpoint, observed: &CanvasCollabSnapshot) {
    let checkpoint: Checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert!(checkpoint.split_owner_child().unwrap().is_none());
    let path = std::env::temp_dir().join(format!("owner-first-{}.sqlite", uuid::Uuid::new_v4()));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let review = journal.local_review(&stored, observed).unwrap();
    assert!(review.split_owner_child().is_none());
    // Capturing review cannot complete, replay, archive or unlock the receipt.
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().as_ref() == Some(&stored));
    assert!(!journal.has_local_review(&stored).unwrap());
    drop(review);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_owner_then_child_delete_keeps_rich_history_and_unrelated_actions() {
    for v in vectors() {
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        let source = snapshot(&v, "ownerSourceRows");
        let p = prepare(&source, owner);
        let deleted_owner = canonical(&source, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&deleted_owner)).unwrap();
        let mut current = readback(&source, &deleted_owner);
        assert!(accepted.confirms_readback(&current).unwrap());
        for report in v["childReports"].as_array().unwrap() {
            current = save(&current, child, report["rtml"].as_str().unwrap());
            assert_eq!(current.section(owner), Some(&deleted_owner));
        }
        assert!(
            paragraph_actions(&current, &[child], 100)[0]
                .as_ref()
                .unwrap()
                .delete
        );
        assert!(SplitIntent::capture(&current, child, 100).is_err());
        let p = prepare(&current, child);
        assert!(!p.is_split_owner_delete());
        let checkpoint = p.checkpoint().unwrap();
        let deleted_child = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&deleted_child)).unwrap();
        let both = readback(&current, &deleted_child);
        assert!(accepted.confirms_readback(&both).unwrap());
        prefix(current.section(child).unwrap(), &deleted_child);
        for old in &current.sections {
            if old.id() != child {
                assert_eq!(both.section(old.id()), Some(old));
            }
        }
        inert(&both, owner, child);
        restart(checkpoint.clone(), &current);
        restart(checkpoint.clone(), &both);
        for omitted in [vec![owner], vec![child], vec![owner, child]] {
            let mut missing = both.clone();
            missing.sections.retain(|r| !omitted.contains(&r.id()));
            assert!(
                !accepted.confirms_readback(&missing).unwrap_or(false),
                "omitted {omitted:?}"
            );
            restart(checkpoint.clone(), &missing);
        }
        let current = save(&both, "survivor", "<b>Survivor edited</b>");
        let p = StructureIntent::insert(
            &current,
            "survivor",
            Side::Before,
            &TextDraft::from_rtml("Independent added").unwrap(),
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
        let current = save(&current, added.id(), "Independent added edited");
        inert(&current, owner, child);
        assert_eq!(current.section(owner), Some(&deleted_owner));
        assert_eq!(current.section(child), Some(&deleted_child));
        for id in [owner, child] {
            assert_ne!(
                current.section(id).unwrap().raw().string(8).unwrap(),
                added.raw().string(8).unwrap()
            );
        }
    }
}

#[test]
fn remaining_child_refuses_last_live_unsupported_neighbors_and_stale_peers() {
    let v = &vectors()[0];
    let source = snapshot(v, "childSourceRows");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    eprintln!("owner_first negative: last-live source");
    let mut alone = source.clone();
    alone
        .sections
        .retain(|r| ["title", owner, child].contains(&r.id()));
    Inventory::capture(&alone).unwrap();
    assert!(TextIntent::capture(&alone, child, 100).is_ok());
    assert!(
        !paragraph_actions(&alone, &[child], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
    assert!(StructureIntent::delete(&alone, child, 100).is_err());
    assert!(SplitIntent::capture(&alone, child, 100).is_err());
    eprintln!("owner_first negative: rich-gap source");
    let v = &vectors()[2];
    let source = snapshot(v, "childSourceRows");
    for id in [owner, child, "survivor", "independent-gap"] {
        eprintln!("owner_first negative: stale {id}");
        let stale = change(source.section(id).unwrap(), n(2, 9999));
        assert!(
            StructureIntent::delete(&source, child, 100)
                .unwrap()
                .prepare(&readback(&source, &stale), 100)
                .is_err(),
            "stale {id}"
        );
    }
    for id in ["independent-gap", "survivor"] {
        for (label, style) in [("list", 5), ("code", 4), ("unknown", 999)] {
            eprintln!("owner_first negative: neighbor {id}/{label}");
            let bad = readback(&source, &change(source.section(id).unwrap(), n(10, style)));
            assert!(
                StructureIntent::delete(&bad, child, 100).is_err(),
                "neighbor {id}/{label}"
            );
        }
    }
    eprintln!("owner_first negative: foreign edge");
    let row = source.section(owner).unwrap();
    let foreign = change(
        &change(&change(row, t(1, "foreign-owner")), t(8, "aam")),
        t(21, "aam"),
    );
    let mut bad = source.clone();
    bad.sections.push(foreign);
    assert!(StructureIntent::delete(&bad, child, 100).is_err());
    eprintln!("owner_first negative: valid checkpoint");
    let checkpoint = prepare(&source, child).checkpoint().unwrap();
    for id in [owner, child] {
        eprintln!("owner_first negative: forged checkpoint {id}");
        let mut value = serde_json::to_value(&checkpoint).unwrap();
        for raw in value["sections"].as_array_mut().unwrap() {
            let bytes: Vec<u8> = serde_json::from_value(raw.clone()).unwrap();
            let row = Section::decode(&bytes).unwrap();
            if row.id() == id {
                *raw =
                    serde_json::to_value(change(&row, m(12, &body("forged"))).raw().raw()).unwrap();
            }
        }
        let forged: Checkpoint = serde_json::from_value(value).unwrap();
        assert!(forged.split_owner_child().is_err(), "forged {id}");
    }
    eprintln!("owner_first negative: forged session");
    let mut value = serde_json::to_value(checkpoint).unwrap();
    value["session"] = Value::String("aaaaaaaaaaaa".into());
    let forged: Checkpoint = serde_json::from_value(value).unwrap();
    assert!(forged.split_owner_child().is_err());
}

#[test]
fn remaining_child_binds_actual_leases_and_rejects_hidden_or_replaced_history() {
    let v = &vectors()[0];
    let source = snapshot(v, "childSourceRows");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    for id in [owner, child] {
        eprintln!("owner_first history: actual lease {id}");
        let row = source.section(id).unwrap();
        let wire = row
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(4, &[n(4, 150)])
            .unwrap();
        let limited = readback(&source, &change(row, m(38, &wire)));
        let p = prepare(&limited, child);
        p.validate_dispatch_expiry(149).unwrap();
        assert!(p.validate_dispatch_expiry(150).is_err(), "lease {id}");
        assert!(
            StructureIntent::delete(&limited, child, 150)
                .and_then(|i| i.prepare(&limited, 150))
                .is_err()
        );
    }
    eprintln!("owner_first history: baseline delete");
    let p = prepare(&source, child);
    let deleted = canonical(&source, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&deleted)).unwrap();
    let both = readback(&source, &deleted);
    for id in [owner, child] {
        eprintln!("owner_first history: construct hidden suffix {id}");
        let row = both.section(id).unwrap();
        let mut doc = history(row).with_actor("abcdef01234500000000".parse().unwrap());
        let mut tx = doc.transaction();
        tx.put(ROOT, "hidden", true).unwrap();
        tx.commit();
        let head = doc.get_heads()[0].to_string();
        let wire = row
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
            .unwrap()
            .replace_fields(6, &[t(6, &head)])
            .unwrap();
        for (label, mutated) in [
            ("hidden root", change(row, m(38, &wire))),
            ("content", change(row, m(12, &body("forged")))),
            (
                "missing processed",
                change(row, m(38, &row.crdt().unwrap().unwrap().remove(6).unwrap())),
            ),
            (
                "collision",
                change(
                    row,
                    t(
                        8,
                        both.section("survivor")
                            .unwrap()
                            .raw()
                            .string(8)
                            .unwrap()
                            .unwrap(),
                    ),
                ),
            ),
            ("scope", change(row, t(7, "wrong-document"))),
        ] {
            eprintln!("owner_first history: reject {id}/{label}");
            let bad = readback(&both, &mutated);
            assert!(Inventory::capture(&bad).is_err(), "{id}/{label}");
            assert!(
                !accepted.confirms_readback(&bad).unwrap_or(false),
                "{id}/{label}"
            );
            if id == child {
                assert!(
                    prepare(&source, child)
                        .validate_ack(&ack(&mutated))
                        .is_err(),
                    "ACK {label}"
                );
            }
        }
    }
    eprintln!("owner_first history: concurrent prefix");
    // A single final deletion must not launder a concurrent causal prefix.
    let mut first =
        history(source.section(child).unwrap()).with_actor("11111111111100000000".parse().unwrap());
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
        prepare(&source, child)
            .validate_ack(&ack(&concurrent))
            .is_err()
    );
}

#[test]
fn inherited_link_append_matches_original_nine_change_child_history() {
    let v = vectors()
        .into_iter()
        .find(|v| v["config"]["richChild"] == true)
        .unwrap();
    let child = v["childId"].as_str().unwrap();
    let owner = v["parentId"].as_str().unwrap();
    let mut current = snapshot(&v, "ownerDeletedRows");
    let unchanged_owner = current.section(owner).unwrap().clone();
    assert_eq!(
        history(current.section(child).unwrap())
            .get_changes(&[])
            .len(),
        5
    );
    for (report_index, report) in v["childReports"].as_array().unwrap().iter().enumerate() {
        let before = current.section(child).unwrap().clone();
        current = save(&current, child, report["rtml"].as_str().unwrap());
        let actual = history(current.section(child).unwrap());
        let expected = history(&row(&current, &report["ack"]));
        let actual_changes = actual.get_changes(&[]);
        let expected_changes = expected.get_changes(&[]);
        assert_eq!(actual_changes.len(), [8, 9][report_index]);
        assert_eq!(actual_changes.len(), expected_changes.len());
        let mut a = Automerge::new();
        let mut e = Automerge::new();
        for (index, (ac, ec)) in actual_changes
            .iter()
            .zip(expected_changes.iter())
            .enumerate()
        {
            a.apply_changes([(*ac).clone()]).unwrap();
            e.apply_changes([(*ec).clone()]).unwrap();
            let ao = a.get(ROOT, "text").unwrap().unwrap().1;
            let eo = e.get(ROOT, "text").unwrap().unwrap().1;
            assert_eq!(
                a.text(&ao).unwrap(),
                e.text(&eo).unwrap(),
                "report {report_index}/change {index}"
            );
            assert_eq!(
                a.marks(&ao).unwrap(),
                e.marks(&eo).unwrap(),
                "report {report_index}/change {index}"
            );
            // Compare emitted operations, including the visually redundant suffix
            // mark and false-removal phases, independent of actor IDs/timestamps.
            let actions = |c: &automerge::Change| {
                c.decode()
                    .operations
                    .into_iter()
                    .map(|o| o.action)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                actions(ac),
                actions(ec),
                "report {report_index}/change {index}"
            );
        }
        prefix(&before, current.section(child).unwrap());
        assert_eq!(current.section(owner), Some(&unchanged_owner));
        let wire = current.section(child).unwrap().crdt().unwrap().unwrap();
        for c in actual.get_changes(&history(&before).get_heads()) {
            assert_eq!(
                split::validation::processed(&wire, &c.hash().to_string()).unwrap(),
                c.hash() == actual.get_heads()[0]
            );
        }
    }
}
