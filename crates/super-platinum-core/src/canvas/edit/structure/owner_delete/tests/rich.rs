use super::*;
use automerge::{
    marks::{ExpandMark, Mark},
    transaction::Transactable,
};
mod gap;
mod live;
mod owner;

fn vectors() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("rich-original.json")).unwrap()["vectors"]
        .as_array()
        .unwrap()
        .clone()
}
fn source(v: &Value) -> (CanvasCollabSnapshot, String, String) {
    let original = cases()
        .into_iter()
        .find(|c| c["mode"] == v["mode"] && c["corpus"] == v["corpus"])
        .unwrap();
    let (s, parent, child) = fixture(&original);
    let wire = decoded(&v["childBeforeSession"])
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    let row = change(
        &change(
            s.section(&child).unwrap(),
            m(12, &body(v["childBeforeText"].as_str().unwrap())),
        ),
        m(38, &wire),
    );
    let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    let tail = v["originalTailHead"].as_str().unwrap();
    assert!(doc.get_change_by_hash(&tail.parse().unwrap()).is_some());
    assert_eq!(
        doc.get_changes(&[]).len(),
        if v["cadence"] == "revert" { 4 } else { 3 }
    );
    assert!(wire.fields().iter().filter(|f| f.tag() == 6).all(|f| {
        RawMessage::decode(wire.encoded_field(f).unwrap())
            .unwrap()
            .string(6)
            .unwrap()
            != Some(tail)
    }));
    (readback(&s, &row), parent, child)
}
fn original_tombstone(v: &Value) -> (CanvasCollabSnapshot, String, String) {
    let (s, parent, child) = source(v);
    let wire = decoded(&v["parentSession"])
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    let row = change(
        &change(&change(s.section(&parent).unwrap(), n(3, 1)), n(2, 3000)),
        m(38, &wire),
    );
    (readback(&s, &row), parent, child)
}

#[test]
fn original_168_rich_surviving_children_preserve_coalesced_origin_and_owner_tombstone() {
    let cases = vectors();
    assert_eq!(cases.len(), 168);
    for v in cases {
        let (s, parent, child) = original_tombstone(&v);
        Inventory::capture(&s).unwrap();
        let old_child_wire = s.section(&child).unwrap().crdt().unwrap().unwrap();
        let old_doc = Automerge::load(old_child_wire.bytes(5).unwrap().unwrap()).unwrap();
        let wire = decoded(&v["childEditWire"]);
        let row = canonical(&s, &wire.message(1).unwrap().unwrap());
        assert_eq!(
            text::canonical_draft(&row).unwrap().rtml().unwrap(),
            v["finalRtml"].as_str().unwrap()
        );
        let mut fresh = readback(&s, &row);
        // The same strict descendant grammar is independently gated by a live
        // isolated pair. Keep the original tombstone assertions below intact.
        let (live_source, _, _) = source(&v);
        let live = readback(&live_source, &row);
        assert_eq!(
            split_child(&live.sections, live.section(&parent).unwrap(), 100).unwrap(),
            vec![row.clone()]
        );
        let updated_wire = row.crdt().unwrap().unwrap();
        let doc = Automerge::load(updated_wire.bytes(5).unwrap().unwrap()).unwrap();
        assert!(
            old_doc
                .get_changes(&[])
                .iter()
                .all(|c| doc.get_change_by_hash(&c.hash()).is_some())
        );
        if v["removeFormatting"] == true && v["format"] != "softbreak" {
            let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
            assert!(
                doc.marks(&object)
                    .unwrap()
                    .iter()
                    .any(|m| m.value() == &automerge::ScalarValue::Boolean(false))
            );
        }
        assert_eq!(fresh.section(&parent), s.section(&parent));
        assert_eq!(
            tombstone(&fresh.sections, fresh.section(&parent).unwrap()).unwrap(),
            row,
            "{}/{}/{}/{}/{}",
            v["cadence"],
            v["mode"],
            v["corpus"],
            v["format"],
            v["removeFormatting"]
        );
        Inventory::capture(&fresh).unwrap_or_else(|e| {
            panic!(
                "rich original {}/{}/{}/{}/{}: {e:?}",
                v["cadence"], v["mode"], v["corpus"], v["format"], v["removeFormatting"]
            )
        });
        // Underline retains the existing structural-write limitation, while
        // the whole inventory and unrelated ordinary row remain usable.
        let neighbor = Section::from_raw(row.raw().remove(38).unwrap()).unwrap();
        let neighbor = change(
            &change(
                &change(&change(&neighbor, t(1, "neighbor")), t(8, "aai")),
                t(21, "aai"),
            ),
            m(12, &body("Unchanged neighbor")),
        );
        fresh.sections.push(neighbor);
        assert!(paragraph_actions(&fresh, &["neighbor"], 100)[0].is_some());
        let actions = paragraph_actions(&fresh, &[&child], 100);
        if v["format"] != "underline" || v["removeFormatting"] == true {
            assert!(actions[0].is_some());
        }
    }
}

#[test]
fn coalesced_and_exact_reverted_child_bold_save_and_remove_keep_native_mapping_and_full_history() {
    for v in vectors()
        .into_iter()
        .filter(|v| v["format"] == "bold" && v["removeFormatting"] == false)
    {
        let (s, parent, child) = source(&v);
        let deletion = prepare(&s, &parent);
        let row = canonical(&s, &deletion.document_data().message(1).unwrap().unwrap());
        let accepted = deletion.validate_ack(&ack(&row)).unwrap();
        let mut current = readback(&s, &row);
        assert!(accepted.confirms_readback(&current).unwrap());
        let tombstone = row;
        let before = current.section(&child).unwrap().crdt().unwrap().unwrap();
        let original = Automerge::load(before.bytes(5).unwrap().unwrap()).unwrap();
        let mut session = EditSession::new();
        for desired in [
            v["finalRtml"].as_str().unwrap(),
            v["childBeforeText"].as_str().unwrap(),
        ] {
            let edit = TextIntent::capture(&current, &child, 100)
                .unwrap()
                .prepare(
                    &current,
                    &mut session,
                    &TextDraft::from_rtml(desired).unwrap(),
                    100,
                )
                .unwrap();
            let update = canonical(&current, &edit.document_data().message(1).unwrap().unwrap());
            let accepted = edit.validate_ack(&ack(&update)).unwrap();
            current = readback(&current, &update);
            assert!(accepted.confirms_readback(&current).unwrap());
            session.remember_text_accepted(&accepted).unwrap();
            assert_eq!(current.section(&parent), Some(&tombstone));
            let actions = paragraph_actions(&current, &[&child], 100);
            assert!(actions[0].as_ref().unwrap().insert_before);
            let wire = update.crdt().unwrap().unwrap();
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            assert!(
                original
                    .get_changes(&[])
                    .iter()
                    .all(|c| doc.get_change_by_hash(&c.hash()).is_some())
            );
            assert!(wire.fields().iter().filter(|f| f.tag() == 6).all(|f| {
                RawMessage::decode(wire.encoded_field(f).unwrap())
                    .unwrap()
                    .string(6)
                    .unwrap()
                    != v["originalTailHead"].as_str()
            }));
        }
    }
}

fn altered_child(s: &CanvasCollabSnapshot, id: &str, mut doc: Automerge) -> CanvasCollabSnapshot {
    let child = s.section(id).unwrap();
    let wire = child.crdt().unwrap().unwrap();
    let mut processed = wire
        .fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            t(
                6,
                RawMessage::decode(wire.encoded_field(f).unwrap())
                    .unwrap()
                    .string(6)
                    .unwrap()
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    processed.extend(doc.get_heads().iter().map(|h| t(6, &h.to_string())));
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &processed)
        .unwrap();
    readback(s, &change(child, m(38, &wire)))
}

fn live_refuses(s: &CanvasCollabSnapshot, v: &Value) {
    let (source, parent, _) = source(v);
    let live = readback(s, source.section(&parent).unwrap());
    // Invalid live lineage affects its owner, not all unrelated inventory.
    Inventory::capture(&live).unwrap();
    assert!(split_child(&live.sections, live.section(&parent).unwrap(), 100).is_err());
    assert!(TextIntent::capture(&live, &parent, 100).is_err());
}

#[test]
fn rich_historical_child_rejects_hidden_root_unknown_marks_attributes_and_objects() {
    let v = vectors()
        .into_iter()
        .find(|v| v["cadence"] == "revert" && v["format"] == "bold")
        .unwrap();
    let (s, parent, child) = original_tombstone(&v);
    let wire = s.section(&child).unwrap().crdt().unwrap().unwrap();
    for kind in 0..6 {
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
                tx.mark(
                    &object,
                    Mark::new(
                        "a".into(),
                        "{\"href\":\"https://example.test/\",\"other\":true}",
                        0,
                        1,
                    ),
                    ExpandMark::After,
                )
                .unwrap();
                tx.mark(
                    &object,
                    Mark::new("a".into(), false, 0, 1),
                    ExpandMark::After,
                )
                .unwrap();
            }
            3 => {
                tx.put_object(ROOT, "hidden", automerge::ObjType::Map)
                    .unwrap();
                tx.delete(ROOT, "hidden").unwrap();
            }
            4 => {
                tx.put(ROOT, "deleted", true).unwrap();
                tx.put(ROOT, "deleted", false).unwrap();
            }
            _ => {
                tx.mark(
                    &object,
                    Mark::new("b".into(), "wrong type", 0, 1),
                    ExpandMark::After,
                )
                .unwrap();
                tx.mark(
                    &object,
                    Mark::new("b".into(), false, 0, 1),
                    ExpandMark::After,
                )
                .unwrap();
            }
        }
        tx.commit();
        let bad = altered_child(&s, &child, doc);
        assert!(
            tombstone(&bad.sections, bad.section(&parent).unwrap()).is_err(),
            "kind {kind}"
        );
        assert!(Inventory::capture(&bad).is_err());
        live_refuses(&bad, &v);
    }
}

#[test]
fn historical_rich_proof_rejects_concurrent_ancestry_unknown_actor_and_replaced_prefix() {
    let v = vectors()
        .into_iter()
        .find(|v| v["cadence"] == "typed" && v["format"] == "bold")
        .unwrap();
    let (s, parent, child) = original_tombstone(&v);
    let wire = s.section(&child).unwrap().crdt().unwrap().unwrap();
    let bytes = wire.bytes(5).unwrap().unwrap();
    let mut branches = Vec::new();
    for actor in ["11111111111100000000", "22222222222200000000"] {
        let mut doc = Automerge::load(bytes)
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
    let mut doc = branches.pop().unwrap();
    doc.merge(&mut right).unwrap();
    assert_eq!(doc.get_heads().len(), 2);
    doc.set_actor("33333333333300000000".parse().unwrap());
    let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
    let mut tx = doc.transaction();
    tx.mark(
        &object,
        Mark::new("i".into(), false, 0, 1),
        ExpandMark::After,
    )
    .unwrap();
    tx.commit();
    let bad = altered_child(&s, &child, doc);
    assert!(tombstone(&bad.sections, bad.section(&parent).unwrap()).is_err());
    live_refuses(&bad, &v);
    let mut doc = Automerge::load(bytes)
        .unwrap()
        .with_actor("abcd".parse().unwrap());
    let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
    let mut tx = doc.transaction();
    tx.mark(
        &object,
        Mark::new("b".into(), false, 0, 1),
        ExpandMark::After,
    )
    .unwrap();
    tx.commit();
    let bad = altered_child(&s, &child, doc);
    assert!(Inventory::capture(&bad).is_err());
    live_refuses(&bad, &v);
    // Same visible text with an unrelated bootstrap never supplies the lost
    // split-tail prefix or its historical actor identity.
    let row = s.section(&child).unwrap();
    let draft = text::canonical_draft(row).unwrap();
    let rebuilt = crdt::insert(&child, &draft, "aaaaaaaaaaaa", 100).unwrap();
    let mut doc = Automerge::load(rebuilt.bytes(5).unwrap().unwrap()).unwrap();
    let wire = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap();
    let bad = readback(&s, &change(row, m(38, &wire)));
    assert!(Inventory::capture(&bad).is_err());
    live_refuses(&bad, &v);
}

#[test]
fn isolated_live_owner_rich_child_restores_owner_actions_without_hiding_child() {
    let v = vectors()
        .into_iter()
        .find(|v| {
            v["cadence"] == "typed" && v["format"] == "bold" && v["removeFormatting"] == false
        })
        .unwrap();
    let (s, parent, child) = source(&v);
    let wire = decoded(&v["childEditWire"]);
    let row = canonical(&s, &wire.message(1).unwrap().unwrap());
    let still_live = readback(&s, &row);
    assert!(
        split_child(
            &still_live.sections,
            still_live.section(&parent).unwrap(),
            100
        )
        .is_ok()
    );
    assert!(TextIntent::capture(&still_live, &parent, 100).is_ok());
    assert!(TextIntent::capture(&still_live, &child, 100).is_ok());
    let actions = paragraph_actions(&still_live, &[&parent, &child], 100);
    for action in actions {
        let action = action.unwrap();
        assert!(action.insert_before && action.insert_after);
        assert!(action.delete);
    }
}
