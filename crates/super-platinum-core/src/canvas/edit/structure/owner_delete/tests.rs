use super::*;
use crate::canvas::{
    edit::{EditSession, text::TextIntent},
    wire::{DocumentData, EditAck},
};
use serde_json::Value;
mod rich;

fn n(tag: u32, v: u64) -> F {
    F::uint(tag, v).unwrap()
}
fn t(tag: u32, v: &str) -> F {
    F::bytes(tag, v.as_bytes()).unwrap()
}
fn m(tag: u32, v: &RawMessage) -> F {
    F::message(tag, v).unwrap()
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn change(s: &Section, f: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn body(value: &str) -> RawMessage {
    msg(&[m(1, &msg(&[t(1, value), t(2, "en")]))])
}
fn decoded(value: &Value) -> RawMessage {
    RawMessage::decode(&crate::slack::api::decode_canvas_base64(value.as_str().unwrap()).unwrap())
        .unwrap()
}
fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("tests/original.json")).unwrap()
}
fn fixture(case: &Value) -> (CanvasCollabSnapshot, String, String) {
    let mut s = super::super::tests::fixture();
    let template = s.section("item").unwrap().clone();
    s.sections.retain(|r| r.id() == "title");
    let parent = case["parentId"].as_str().unwrap().to_owned();
    let child = case["childId"].as_str().unwrap().to_owned();
    for (id, text, session, pos) in [
        (&parent, &case["parentText"], &case["beforeSession"], "aae"),
        (&child, &case["childText"], &case["childSession"], "aag"),
    ] {
        let wire = decoded(session)
            .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
            .unwrap();
        let mut row = template.clone();
        for f in [
            t(1, id),
            n(2, 2000),
            n(3, 0),
            t(8, pos),
            t(21, pos),
            m(12, &body(text.as_str().unwrap())),
            m(38, &wire),
        ] {
            row = change(&row, f);
        }
        s.sections.push(row);
    }
    (s, parent, child)
}
fn prepare(s: &CanvasCollabSnapshot, id: &str) -> PreparedStructureEdit {
    StructureIntent::delete(s, id, 100)
        .unwrap()
        .prepare(s, 100)
        .unwrap()
}
fn canonical(s: &CanvasCollabSnapshot, update: &RawMessage) -> Section {
    let update = Section::from_raw(update.clone()).unwrap();
    let old = s.section(update.id());
    let seq = old.and_then(|r| r.sequence().unwrap()).unwrap_or(0) + 1000;
    let mut row = old.map_or_else(|| update.clone(), |r| r.merge_partial(&update).unwrap());
    let wire = update
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, seq)])
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
        .unwrap();
    for f in [
        n(2, seq),
        t(6, s.thread_id()),
        t(7, s.document_id()),
        m(38, &wire),
    ] {
        row = change(&row, f);
    }
    if let Some(p) = row
        .raw()
        .string(8)
        .unwrap()
        .filter(|p| p.ends_with(":temp"))
    {
        let p = position::base(p).unwrap().to_owned();
        row = change(&change(&row, t(8, &p)), t(21, &p));
    }
    row
}
fn ack(row: &Section) -> EditAck {
    EditAck::decode(msg(&[n(1, 1), m(2, &msg(&[m(1, row.raw())]))]).raw()).unwrap()
}
fn readback(s: &CanvasCollabSnapshot, row: &Section) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    if let Some(old) = s.sections.iter_mut().find(|r| r.id() == row.id()) {
        *old = row.clone();
    } else {
        s.sections.push(row.clone());
    }
    s
}

#[test]
fn original_six_saved_pair_deletions_retain_lineage_and_full_child_history() {
    // Frozen original method vectors: bootstrap/positive/modern zero x ASCII/Unicode.
    // Canonical leases/revisions below are synthetic adapters, not server evidence.
    for case in cases() {
        let (s, parent, child) = fixture(&case);
        assert_eq!(
            live_pair(&s.sections, s.section(&parent).unwrap(), 100).unwrap(),
            *s.section(&child).unwrap()
        );
        let actual = DocumentData::decode(decoded(&case["deleteWire"]).raw()).unwrap();
        let wire = actual.sections()[0].crdt().unwrap().unwrap();
        let original_doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let head = original_doc.get_heads()[0];
        let deletion = original_doc.get_change_by_hash(&head).unwrap();
        assert_eq!(
            deletion.deps().len(),
            2,
            "original processed ancestor plus current head"
        );
        let causal_prefix = original_doc.fork_at(deletion.deps()).unwrap();
        assert_eq!(causal_prefix.get_heads().len(), 1);
        let prior_actor_sequence = causal_prefix
            .get_changes(&[])
            .iter()
            .filter(|c| c.actor_id() == deletion.actor_id())
            .map(|c| c.seq())
            .max()
            .unwrap_or(0);
        assert_eq!(deletion.seq(), prior_actor_sequence + 1);
        let draft = text::canonical_draft(s.section(&parent).unwrap()).unwrap();
        transition(
            s.section(&parent).unwrap(),
            &wire,
            s.section(&parent).unwrap(),
            &draft,
        )
        .unwrap_or_else(|e| {
            panic!(
                "original transition {}/{}: {e:?}",
                case["mode"], case["corpus"]
            )
        });
        history(
            &wire,
            s.section(&parent).unwrap(),
            &draft,
            true,
            None,
            position::modern_id(&parent),
        )
        .unwrap();
        let p = prepare(&s, &parent);
        assert!(p.is_split_owner_delete());
        let c = p.checkpoint().unwrap();
        assert_eq!(c.split_owner_child().unwrap().as_ref(), s.section(&child));
        let row = canonical(&s, &p.document_data().message(1).unwrap().unwrap());
        let accepted = p.validate_ack(&ack(&row)).unwrap();
        let fresh = readback(&s, &row);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        let actions = paragraph_actions(&fresh, &[&parent, &child], 100);
        assert!(actions[0].is_none());
        assert!(actions[1].as_ref().unwrap().insert_before);
        // The deleted owner must never witness a remaining reachable sibling.
        assert!(!actions[1].as_ref().unwrap().delete);
        assert_eq!(fresh.section(&child), s.section(&child));
        let mut omitted = fresh.clone();
        omitted.sections.retain(|r| r.id() != parent);
        assert!(!accepted.confirms_readback(&omitted).unwrap());
        let mut changed_child = fresh.clone();
        let i = changed_child
            .sections
            .iter()
            .position(|r| r.id() == child)
            .unwrap();
        changed_child.sections[i] = change(&changed_child.sections[i], n(2, 9000));
        assert!(!accepted.confirms_readback(&changed_child).unwrap());
    }
}

#[test]
fn retained_owner_does_not_block_independent_child_edit_or_insertion_in_gap() {
    let (s, parent, child) = fixture(&cases()[1]);
    let p = prepare(&s, &parent);
    let deleted = canonical(&s, &p.document_data().message(1).unwrap().unwrap());
    let accepted = p.validate_ack(&ack(&deleted)).unwrap();
    let mut current = readback(&s, &deleted);
    assert!(accepted.confirms_readback(&current).unwrap());
    let old_child = current.section(&child).unwrap().crdt().unwrap().unwrap();
    let edit = TextIntent::capture(&current, &child, 100)
        .unwrap()
        .prepare(
            &current,
            &mut EditSession::new(),
            &TextDraft::from_rtml("Second 日本😀 &amp; paragraph edited").unwrap(),
            100,
        )
        .unwrap();
    let row = canonical(&current, &edit.document_data().message(1).unwrap().unwrap());
    let accepted_text = edit.validate_ack(&ack(&row)).unwrap();
    current = readback(&current, &row);
    assert!(accepted_text.confirms_readback(&current).unwrap());
    assert_eq!(current.section(&parent), Some(&deleted));
    let before = Automerge::load(old_child.bytes(5).unwrap().unwrap()).unwrap();
    let after_wire = row.crdt().unwrap().unwrap();
    let after = Automerge::load(after_wire.bytes(5).unwrap().unwrap()).unwrap();
    assert!(
        before
            .get_changes(&[])
            .iter()
            .all(|c| after.get_change_by_hash(&c.hash()).is_some())
    );
    let insertion = StructureIntent::insert(
        &current,
        &child,
        Side::Before,
        &TextDraft::from_rtml("Between").unwrap(),
        100,
    )
    .unwrap()
    .prepare(&current, 100)
    .unwrap();
    insertion.checkpoint().unwrap();
    let inserted = canonical(
        &current,
        &insertion.document_data().message(1).unwrap().unwrap(),
    );
    let accepted_insert = insertion.validate_ack(&ack(&inserted)).unwrap();
    current = readback(&current, &inserted);
    assert!(accepted_insert.confirms_readback(&current).unwrap());
    assert!(deleted.raw().string(8).unwrap() < inserted.raw().string(8).unwrap());
    assert!(inserted.raw().string(8).unwrap() < row.raw().string(8).unwrap());
    assert!(
        paragraph_actions(&current, &[&child, inserted.id()], 100)
            .iter()
            .all(Option::is_some)
    );
    assert!(
        paragraph_actions(&current, &[&child], 100)[0]
            .as_ref()
            .unwrap()
            .delete
    );
    assert!(StructureIntent::delete(&current, &child, 100).is_ok());
    assert!(SplitIntent::capture(&current, &child, 100).is_err());
    let collision = change(&inserted, t(8, deleted.raw().string(8).unwrap().unwrap()));
    assert!(Inventory::capture(&readback(&current, &collision)).is_err());
}

#[test]
fn owner_dispatch_lease_includes_child_and_preflight_binds_every_raw_peer() {
    let (mut s, parent, child) = fixture(&cases()[0]);
    let child_row = s.section(&child).unwrap();
    let wire = child_row
        .crdt()
        .unwrap()
        .unwrap()
        .replace_fields(4, &[n(4, 200)])
        .unwrap();
    s = readback(&s, &change(child_row, m(38, &wire)));
    let p = prepare(&s, &parent);
    assert!(p.validate_dispatch_expiry(199).is_ok());
    assert!(p.validate_dispatch_expiry(200).is_err());
    let changed = readback(
        &s,
        &change(s.section(&child).unwrap(), t(1001, "changed extension")),
    );
    assert!(p.validate_snapshot(&changed, 100).is_err());
    assert!(StructureIntent::delete(&s, &parent, 200).is_err());
}

#[test]
fn owner_delete_admits_only_proved_nonmerging_previous_shapes_and_rejects_unknown_tombstone() {
    let (s, parent, _) = fixture(&cases()[0]);
    let base = s.section(&parent).unwrap().raw().remove(38).unwrap();
    for style in [0, 1, 2, 3, 4, 5, 6, 7, 8] {
        let mut previous = Section::from_raw(base.clone()).unwrap();
        for f in [t(1, "previous"), t(8, "aad"), t(21, "aad"), n(10, style)] {
            previous = change(&previous, f);
        }
        let mut with_previous = s.clone();
        with_previous.sections.push(previous);
        assert_eq!(
            StructureIntent::delete(&with_previous, &parent, 100).is_ok(),
            style <= 3
        );
    }
    let mut unknown = Section::from_raw(base).unwrap();
    for f in [t(1, "unknown-deleted"), t(8, "aah"), t(21, "aah"), n(3, 1)] {
        unknown = change(&unknown, f);
    }
    let mut bad = s;
    bad.sections.push(unknown);
    assert!(Inventory::capture(&bad).is_err());
}

#[test]
fn deleted_projection_with_extra_hidden_operation_is_not_deletion_only_evidence() {
    use automerge::transaction::Transactable;
    let (s, parent, _) = fixture(&cases()[0]);
    let source = s.section(&parent).unwrap();
    let wire = source.crdt().unwrap().unwrap();
    let mut doc = Automerge::load(wire.bytes(5).unwrap().unwrap())
        .unwrap()
        .with_actor("aabbccddeeff00000000".parse().unwrap());
    let mut tx = doc.transaction();
    tx.put(ROOT, "unreviewed", true).unwrap();
    tx.delete(ROOT, "unreviewed").unwrap();
    tx.put(ROOT, "deleted", true).unwrap();
    tx.commit();
    let head = doc.get_heads()[0].to_string();
    let old = wire
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
        .chain(std::iter::once(t(6, &head)))
        .collect::<Vec<_>>();
    let altered = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &old)
        .unwrap();
    assert!(
        history(
            &altered,
            source,
            &text::canonical_draft(source).unwrap(),
            true,
            None,
            false
        )
        .is_err()
    );
}

#[test]
fn owner_ack_and_inventory_refuse_missing_forged_or_replaced_history_and_components() {
    let (s, parent, child) = fixture(&cases()[0]);
    let p = prepare(&s, &parent);
    let good = canonical(&s, &p.document_data().message(1).unwrap().unwrap());
    let wire = good.crdt().unwrap().unwrap();
    let source_wire = s.section(&parent).unwrap().crdt().unwrap().unwrap();
    for bad in [
        wire.remove(10).unwrap(),
        wire.replace_fields(
            10,
            &[m(
                10,
                &wire
                    .message(10)
                    .unwrap()
                    .unwrap()
                    .replace_fields(4, &[n(4, 2)])
                    .unwrap(),
            )],
        )
        .unwrap(),
        wire.replace_fields(
            5,
            &[F::bytes(5, source_wire.bytes(5).unwrap().unwrap()).unwrap()],
        )
        .unwrap(),
        wire.remove(6).unwrap(),
    ] {
        let row = change(&good, m(38, &bad));
        assert!(prepare(&s, &parent).validate_ack(&ack(&row)).is_err());
        assert!(Inventory::capture(&readback(&s, &row)).is_err());
    }
    for f in [n(2, 9000), n(3, 1), t(8, "aad"), m(38, &msg(&[]))] {
        let broken = readback(&s, &change(s.section(&child).unwrap(), f));
        assert!(StructureIntent::delete(&broken, &parent, 100).is_err());
    }
    let mut missing = s.clone();
    missing.sections.retain(|r| r.id() != child);
    assert!(StructureIntent::delete(&missing, &parent, 100).is_err());
    let mut third = s.clone();
    let forged = source_wire
        .message(10)
        .unwrap()
        .unwrap()
        .replace_fields(3, &[t(3, &parent)])
        .unwrap();
    let third_row = change(
        &change(s.section(&parent).unwrap(), t(1, "third")),
        m(
            38,
            &source_wire.replace_fields(10, &[m(10, &forged)]).unwrap(),
        ),
    );
    third
        .sections
        .push(change(&change(&third_row, t(8, "aad")), t(21, "aad")));
    assert!(StructureIntent::delete(&third, &parent, 100).is_err());
    let doubled = source_wire
        .replace_fields(
            10,
            &[
                m(10, &source_wire.message(10).unwrap().unwrap()),
                m(10, &source_wire.message(10).unwrap().unwrap()),
            ],
        )
        .unwrap();
    assert!(
        StructureIntent::delete(
            &readback(&s, &change(s.section(&parent).unwrap(), m(38, &doubled))),
            &parent,
            100
        )
        .is_err()
    );
}

#[test]
fn deletion_of_concurrent_live_prefix_is_refused_even_with_equal_text_projection() {
    use automerge::transaction::Transactable;
    let (s, parent, _) = fixture(&cases()[0]);
    let source = s.section(&parent).unwrap();
    let wire = source.crdt().unwrap().unwrap();
    let bytes = wire.bytes(5).unwrap().unwrap();
    let mut branches = Vec::new();
    for actor in ["11111111111100000000", "22222222222200000000"] {
        let mut doc = Automerge::load(bytes)
            .unwrap()
            .with_actor(actor.parse().unwrap());
        let mut tx = doc.transaction();
        tx.put(ROOT, "transient", true).unwrap();
        tx.delete(ROOT, "transient").unwrap();
        tx.commit();
        branches.push(doc);
    }
    let mut right = branches.pop().unwrap();
    let mut doc = branches.pop().unwrap();
    doc.merge(&mut right).unwrap();
    let concurrent = doc.get_heads();
    assert_eq!(concurrent.len(), 2);
    let draft = text::canonical_draft(source).unwrap();
    text::crdt::check_document_state(&doc, &draft, false).unwrap();
    doc.set_actor("33333333333300000000".parse().unwrap());
    let mut tx = doc.transaction();
    tx.put(ROOT, "deleted", true).unwrap();
    tx.commit();
    let head = doc.get_heads()[0];
    assert_eq!(doc.get_change_by_hash(&head).unwrap().len(), 1);
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
    processed.extend(
        concurrent
            .iter()
            .chain(std::iter::once(&head))
            .map(|h| t(6, &h.to_string())),
    );
    let altered = wire
        .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
        .unwrap()
        .replace_fields(6, &processed)
        .unwrap();
    assert!(history(&altered, source, &draft, true, None, false).is_err());
}

#[test]
fn v1_owner_checkpoint_roundtrip_rejects_peer_and_request_tampering_and_review_is_inert() {
    use super::super::recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    };
    let (s, parent, child) = fixture(&cases()[2]);
    let p = prepare(&s, &parent);
    let c = p.checkpoint().unwrap();
    let value = serde_json::to_value(&c).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["action"], "Delete");
    let restored: Checkpoint = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        restored.split_owner_child().unwrap().as_ref(),
        s.section(&child)
    );
    for request in [false, true] {
        let mut bad = value.clone();
        if request {
            let data = RawMessage::decode(
                &serde_json::from_value::<Vec<u8>>(bad["request"].clone()).unwrap(),
            )
            .unwrap();
            let update = data
                .message(1)
                .unwrap()
                .unwrap()
                .replace_fields(12, &[m(12, &body("forged"))])
                .unwrap();
            bad["request"] =
                serde_json::to_value(data.replace_fields(1, &[m(1, &update)]).unwrap().raw())
                    .unwrap();
        } else {
            bad["sections"].as_array_mut().unwrap().retain(|raw| {
                Section::decode(&serde_json::from_value::<Vec<u8>>(raw.clone()).unwrap())
                    .unwrap()
                    .id()
                    != child
            });
        }
        let bad: Checkpoint = serde_json::from_value(bad).unwrap();
        assert!(bad.split_owner_child().is_err());
    }
    let path = std::env::temp_dir().join(format!("owner-delete-{}.sqlite", uuid::Uuid::new_v4()));
    let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
    let mut journal = Journal::open(&path, account).unwrap();
    let Begin::Started(receipt) = journal.begin(c).unwrap() else {
        panic!("new journal")
    };
    drop(journal);
    let journal = Journal::open(&path, account).unwrap();
    let restored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
    let deleted = canonical(&s, &p.document_data().message(1).unwrap().unwrap());
    let review = journal
        .local_review(&restored, &readback(&s, &deleted))
        .unwrap();
    assert_eq!(review.split_owner_child(), s.section(&child));
    assert!(
        review
            .observed_sections()
            .any(|r| r.id() == parent && r.deleted().unwrap() == Some(true))
    );
    assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_some());
    drop(review);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}

mod child;
mod retained_pair;

mod owner_first;
