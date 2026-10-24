//! Actual retained original child-delete vectors; canonical leases are synthetic.
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
    let v: Value = serde_json::from_str(include_str!("child-original.json")).unwrap();
    assert_eq!(
        v["bindingsSha256"],
        "f7d7f64bb6a0b12004393363214af003867af51dd7f00352797fcc655347c9f6"
    );
    v["vectors"].as_array().unwrap().clone()
}
fn row(s: &CanvasCollabSnapshot, encoded: &Value) -> Section {
    let partial = Section::from_raw(decoded(encoded)).unwrap();
    // Original deletion rows omit CONTENT and creation metadata. The retained
    // host uses its prior row, just as a canonical partial reducer does.
    let mut r = s.section(partial.id()).map_or_else(
        || partial.clone(),
        |saved| saved.merge_partial(&partial).unwrap(),
    );
    let position = r.raw().string(8).unwrap().unwrap().to_owned();
    if let Some(path) = r.raw().string(21).unwrap().filter(|path| *path != position) {
        // The retained helper normalizes position8 but leaves the original
        // creation path21 temporary. This is a synthetic canonical adapter,
        // not permission for production to accept inconsistent placement.
        assert_eq!(path, format!("{position}:temp"));
        r = change(&r, t(21, &position));
    }
    let wire = r
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    for f in [t(6, s.thread_id()), t(7, s.document_id()), m(38, &wire)] {
        r = change(&r, f);
    }
    r
}
fn snapshot(v: &Value, key: &str) -> CanvasCollabSnapshot {
    let mut s = if key == "savedRowWires" {
        let mut s = super::super::super::tests::fixture();
        s.sections.retain(|r| r.id() == "title");
        s
    } else {
        snapshot(v, "savedRowWires")
    };
    for encoded in v[key].as_array().unwrap() {
        if key != "savedRowWires" && v["savedRowWires"].as_array().unwrap().contains(encoded) {
            // Keep every byte of untouched owner/gap rows, including ordering.
            continue;
        }
        let adapted = row(&s, encoded);
        s = readback(&s, &adapted);
    }
    Inventory::capture(&s)
        .unwrap_or_else(|error| panic!("{:?}/{key}/inventory: {error:?}", v["config"]));
    for id in [
        v["parentId"].as_str().unwrap(),
        v["childId"].as_str().unwrap(),
    ] {
        text::canonical_draft(s.section(id).unwrap())
            .unwrap_or_else(|error| panic!("{:?}/{key}/{id}/content: {error:?}", v["config"]));
    }
    s
}
fn history(r: &Section) -> Automerge {
    Automerge::load(r.crdt().unwrap().unwrap().bytes(5).unwrap().unwrap()).unwrap()
}
fn prefix(before: &Section, after: &Section) {
    let a = history(before);
    let b = history(after);
    for c in a.get_changes(&[]) {
        assert_eq!(
            b.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
            c.raw_bytes()
        );
    }
}
fn mapped(s: &CanvasCollabSnapshot, parent: &str, child: &str) {
    let inert = s.section(child).unwrap();
    assert_eq!(inert.deleted().unwrap(), Some(true));
    assert_eq!(
        split_child(&s.sections, s.section(parent).unwrap(), 100)
            .unwrap_or_else(|error| panic!("retained {parent}->{child}/association: {error:?}")),
        vec![inert.clone()]
    );
    assert_eq!(
        TextIntent::capture(s, parent, 100)
            .unwrap()
            .associated_peer_count(),
        1
    );
    assert!(TextIntent::capture(s, child, 100).is_err());
    let actions = paragraph_actions(s, &[parent, child], 100);
    let owner = actions[0].as_ref().unwrap();
    assert!(owner.insert_before && owner.insert_after);
    // The original adjacent cohorts leave only the owner live. Independent gap
    // insertion supplies the separate surviving paragraph required for Delete.
    let has_survivor = s.sections.iter().any(|r| {
        r.id() != parent
            && r.id() != child
            && r.section_type().unwrap().unwrap_or(0) == 0
            && r.deleted().unwrap() != Some(true)
    });
    assert_eq!(owner.delete, has_survivor);
    assert!(actions[1].is_none());
    assert_eq!(
        StructureIntent::delete(s, parent, 100).is_ok(),
        has_survivor
    );
    assert!(SplitIntent::capture(s, parent, 100).is_err());
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
    let updated = canonical(s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&updated)).unwrap();
    let next = readback(s, &updated);
    assert!(accepted.confirms_readback(&next).unwrap());
    prefix(s.section(id).unwrap(), &updated);
    for old in s.sections.iter().filter(|r| r.id() != id) {
        assert_eq!(next.section(old.id()), Some(old));
    }
    next
}

#[test]
fn original_three_child_tombstones_preserve_owner_continuation_and_inert_peer() {
    for v in vectors() {
        let source = snapshot(&v, "savedRowWires");
        let mut current = snapshot(&v, "deletedCanonicalRows");
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        assert_eq!(source.section(owner), current.section(owner));
        let before = source.section(child).unwrap();
        let deleted = current.section(child).unwrap().clone();
        prefix(before, &deleted);
        let a = history(before);
        let b = history(&deleted);
        if v["config"]["richChild"] == true {
            let tail = a.get_changes(&[])[1].hash();
            assert!(a.get_changes(&[]).len() > 2);
            assert!(
                !split::validation::processed(&before.crdt().unwrap().unwrap(), &tail.to_string())
                    .unwrap()
            );
            assert!(
                !split::validation::processed(&deleted.crdt().unwrap().unwrap(), &tail.to_string())
                    .unwrap()
            );
            assert!(b.get_change_by_hash(&tail).is_some());
        }
        assert_eq!(b.get_changes(&a.get_heads()).len(), 1, "{:?}", v["config"]);
        mapped(&current, owner, child);
        for edit in v["ownerEdits"].as_array().unwrap() {
            let changed = row(&current, &edit["ack"]);
            prefix(current.section(owner).unwrap(), &changed);
            let lineage = current
                .section(owner)
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .messages(10)
                .unwrap();
            current = readback(&current, &changed);
            assert_eq!(
                current
                    .section(owner)
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .messages(10)
                    .unwrap(),
                lineage
            );
            assert_eq!(current.section(child), Some(&deleted));
            mapped(&current, owner, child);
        }
    }
}

fn restart(checkpoint: Checkpoint, snapshot: &CanvasCollabSnapshot) {
    let checkpoint: Checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert!(checkpoint.split_owner_child().unwrap().is_none());
    let path = std::env::temp_dir().join(format!("child-delete-{}.sqlite", uuid::Uuid::new_v4()));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let _review = journal.local_review(&stored, snapshot).unwrap();
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_some());
    drop(_review);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_child_delete_ack_requires_retained_peer_then_owner_edits_and_insertion_work() {
    for v in vectors() {
        let source = snapshot(&v, "savedRowWires");
        let owner = v["parentId"].as_str().unwrap();
        let child = v["childId"].as_str().unwrap();
        assert!(
            paragraph_actions(&source, &[child], 100)[0]
                .as_ref()
                .unwrap()
                .delete
        );
        let prepared = prepare(&source, child);
        let checkpoint = prepared.checkpoint().unwrap();
        let deleted = canonical(
            &source,
            &prepared.document_data().message(1).unwrap().unwrap(),
        );
        let accepted = prepared.validate_ack(&ack(&deleted)).unwrap();
        let mut current = readback(&source, &deleted);
        assert!(accepted.confirms_readback(&current).unwrap());
        assert_eq!(current.section(owner), source.section(owner));
        mapped(&current, owner, child);
        let mut omitted = current.clone();
        omitted.sections.retain(|r| r.id() != child);
        assert!(!accepted.confirms_readback(&omitted).unwrap());
        assert!(TextIntent::capture(&omitted, owner, 100).is_err());
        restart(checkpoint.clone(), &current);
        restart(checkpoint, &omitted);
        for edit in v["ownerEdits"].as_array().unwrap() {
            current = save(&current, owner, edit["rtml"].as_str().unwrap());
            mapped(&current, owner, child);
            assert_eq!(current.section(child), Some(&deleted));
        }
        let p = StructureIntent::insert(
            &current,
            owner,
            Side::After,
            &TextDraft::from_rtml("New independent gap").unwrap(),
            100,
        )
        .unwrap()
        .prepare(&current, 100)
        .unwrap();
        p.checkpoint().unwrap();
        let added = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
        assert_ne!(
            added.raw().string(8).unwrap(),
            deleted.raw().string(8).unwrap()
        );
        let accepted = p.validate_ack(&ack(&added)).unwrap();
        current = readback(&current, &added);
        assert!(accepted.confirms_readback(&current).unwrap());
        mapped(&current, owner, child);
        current = save(&current, owner, "Owner still editable");
        mapped(&current, owner, child);
        assert_eq!(current.section(child), Some(&deleted));
        let p = prepare(&current, added.id());
        p.checkpoint().unwrap();
        let gap_tombstone = canonical(&current, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&gap_tombstone)).unwrap();
        current = readback(&current, &gap_tombstone);
        assert!(accepted.confirms_readback(&current).unwrap());
        mapped(&current, owner, child);
        current = save(&current, owner, "Owner after gap deletion");
        mapped(&current, owner, child);
    }
}

#[test]
fn child_delete_rejects_stale_peer_hidden_prefix_foreign_edge_and_forged_checkpoint() {
    let v = &vectors()[0];
    let source = snapshot(v, "savedRowWires");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    let prepared = prepare(&source, child);
    let checkpoint = prepared.checkpoint().unwrap();
    let deleted = canonical(
        &source,
        &prepared.document_data().message(1).unwrap().unwrap(),
    );
    let accepted = prepared.validate_ack(&ack(&deleted)).unwrap();
    let current = readback(&source, &deleted);
    let stale = change(source.section(owner).unwrap(), n(2, 3000));
    assert!(
        StructureIntent::delete(&source, child, 100)
            .unwrap()
            .prepare(&readback(&source, &stale), 100)
            .is_err()
    );
    let wrong_content = change(&deleted, m(12, &body("forged")));
    let collision = change(
        &deleted,
        t(
            8,
            source
                .section(owner)
                .unwrap()
                .raw()
                .string(8)
                .unwrap()
                .unwrap(),
        ),
    );
    for (label, target) in [("content", wrong_content), ("position", collision)] {
        let bad = readback(&current, &target);
        assert!(
            !accepted.confirms_readback(&bad).unwrap_or(false),
            "{label}"
        );
        assert!(Inventory::capture(&bad).is_err(), "{label}");
    }
    let mut d = history(&deleted).with_actor("abcdef01234500000000".parse().unwrap());
    let mut tx = d.transaction();
    tx.put(ROOT, "hidden", true).unwrap();
    tx.commit();
    let head = d.get_heads()[0].to_string();
    let wire = deleted
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(5, &[F::bytes(5, &d.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &[t(6, &head)])
        .unwrap();
    let hidden = change(&deleted, m(38, &wire));
    assert!(Inventory::capture(&readback(&current, &hidden)).is_err());
    assert!(
        !accepted
            .confirms_readback(&readback(&current, &hidden))
            .unwrap_or(false)
    );
    assert!(prepare(&source, child).validate_ack(&ack(&hidden)).is_err());
    // A final deletion depending on two concurrent text branches is not a
    // deletion of one completely saved causal prefix.
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
    let content = first.text(&object).unwrap();
    let wire = deleted
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(5, &[F::bytes(5, &first.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &[t(6, &head)])
        .unwrap();
    let concurrent = change(
        &change(&deleted, m(38, &wire)),
        m(12, &body(content.strip_suffix('\u{feff}').unwrap())),
    );
    assert!(Inventory::capture(&readback(&current, &concurrent)).is_err());
    assert!(
        prepare(&source, child)
            .validate_ack(&ack(&concurrent))
            .is_err()
    );
    let mut foreign = source.section(owner).unwrap().clone();
    foreign = change(
        &change(&change(&foreign, t(1, "foreign-owner")), t(8, "aah")),
        t(21, "aah"),
    );
    let mut bad = source.clone();
    bad.sections.push(foreign);
    assert!(StructureIntent::delete(&bad, child, 100).is_err());
    let mut encoded = serde_json::to_value(&checkpoint).unwrap();
    let mut rows = encoded["sections"].as_array().unwrap().clone();
    for r in &mut rows {
        let bytes: Vec<u8> = serde_json::from_value(r.clone()).unwrap();
        if Section::decode(&bytes).unwrap().id() == owner {
            *r = serde_json::to_value(stale.raw().raw()).unwrap();
        }
    }
    encoded["sections"] = Value::Array(rows);
    let bad: Checkpoint = serde_json::from_value(encoded).unwrap();
    assert!(bad.split_owner_child().is_err());
}

#[test]
fn dependent_child_delete_dispatch_lease_includes_owner() {
    let v = &vectors()[0];
    let source = snapshot(v, "savedRowWires");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    let row = source.section(owner).unwrap();
    let row = change(
        row,
        m(
            38,
            &row.crdt()
                .unwrap()
                .unwrap()
                .replace_fields(4, &[n(4, 150)])
                .unwrap(),
        ),
    );
    let source = readback(&source, &row);
    let p = prepare(&source, child);
    p.validate_dispatch_expiry(149).unwrap();
    assert!(p.validate_dispatch_expiry(150).is_err());
}

#[test]
fn original_gap_user_edit_origin_is_preserved_and_other_origin_encodings_refuse() {
    let v = &vectors()[2];
    assert_eq!(v["config"]["richChild"], true);
    let source = snapshot(v, "savedRowWires");
    let owner = v["parentId"].as_str().unwrap();
    let child = v["childId"].as_str().unwrap();
    let gap = source.section("independent-gap").unwrap();
    assert_eq!(gap.raw().uint(25).unwrap(), Some(0));
    assert_eq!(
        gap.raw().fields().iter().filter(|f| f.tag() == 25).count(),
        1
    );
    let request = prepare(&source, child);
    let deleted = canonical(
        &source,
        &request.document_data().message(1).unwrap().unwrap(),
    );
    let accepted = request.validate_ack(&ack(&deleted)).unwrap();
    let current = readback(&source, &deleted);
    assert!(accepted.confirms_readback(&current).unwrap());
    assert_eq!(current.section(gap.id()), Some(gap));
    mapped(&current, owner, child);
    for (label, fields) in [
        ("non-user origin", vec![n(25, 1)]),
        ("unknown origin", vec![n(25, u64::MAX)]),
        ("duplicate zero", vec![n(25, 0), n(25, 0)]),
        ("wrong wire type", vec![t(25, "0")]),
    ] {
        let altered = Section::from_raw(gap.raw().replace_fields(25, &fields).unwrap()).unwrap();
        let fresh = readback(&source, &altered);
        assert!(
            StructureIntent::delete(&fresh, child, 100).is_err(),
            "{label}"
        );
        assert!(TextIntent::capture(&fresh, owner, 100).is_err(), "{label}");
        let retained = readback(&current, &altered);
        assert!(Inventory::capture(&retained).is_err(), "{label}");
        assert!(
            !accepted.confirms_readback(&retained).unwrap_or(false),
            "{label}"
        );
    }
}
