use super::*;
use crate::canvas::edit::structure::split::validation;

#[test]
fn twelve_original_live_pairs_keep_owner_and_exact_coalesced_history_after_rich_and_removal() {
    let vectors: Value = serde_json::from_str(include_str!("live-original.json")).unwrap();
    let vectors = vectors["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 12);
    for v in vectors {
        let (s, parent, child) = source(v);
        let owner_wire = decoded(&v["ownerSession"])
            .replace_fields(4, &[n(4, 8_000_000_000_000_000_000)])
            .unwrap();
        let owner = change(s.section(&parent).unwrap(), m(38, &owner_wire));
        assert_eq!(
            text::canonical_draft(&owner).unwrap().rtml().unwrap(),
            v["ownerText"].as_str().unwrap()
        );
        let mut current = readback(&s, &owner);
        let before = current.section(&child).unwrap().crdt().unwrap().unwrap();
        let original = Automerge::load(before.bytes(5).unwrap().unwrap()).unwrap();
        for (key, expected) in [
            ("formattedWire", "formatted"),
            ("removedWire", "removedText"),
        ] {
            let data = decoded(&v[key]);
            let row = canonical(&current, &data.message(1).unwrap().unwrap());
            current = readback(&current, &row);
            assert_eq!(
                text::canonical_draft(&row).unwrap().rtml().unwrap(),
                v[expected].as_str().unwrap()
            );
            assert_eq!(current.section(&parent), Some(&owner));
            assert_eq!(
                split_child(&current.sections, &owner, 100).unwrap(),
                vec![row.clone()]
            );
            assert!(TextIntent::capture(&current, &parent, 100).is_ok());
            assert!(TextIntent::capture(&current, &child, 100).is_ok());
            for actions in paragraph_actions(&current, &[&parent, &child], 100) {
                let actions = actions.unwrap();
                assert!(actions.insert_before && actions.insert_after && actions.delete);
            }
            let wire = row.crdt().unwrap().unwrap();
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            assert!(
                original
                    .get_changes(&[])
                    .iter()
                    .all(|c| doc.get_change_by_hash(&c.hash()).is_some())
            );
            assert!(
                !validation::processed(&wire, v["originalTailHead"].as_str().unwrap()).unwrap()
            );
            if key == "removedWire" && v["format"] != "softbreak" {
                let (_, object) = doc.get(ROOT, "text").unwrap().unwrap();
                assert!(
                    doc.marks(&object)
                        .unwrap()
                        .iter()
                        .any(|m| m.value() == &automerge::ScalarValue::Boolean(false))
                );
            }
        }
    }
}

#[test]
fn live_child_bold_then_owner_text_and_delete_keep_exact_peer_and_inert_recovery() {
    use super::super::super::super::recovery::{
        Checkpoint,
        journal::{Begin, Journal},
    };
    for v in vectors().into_iter().filter(|v| {
        v["cadence"] == "revert"
            && v["mode"] == "retained-positive"
            && v["format"] == "bold"
            && v["removeFormatting"] == false
    }) {
        let (s, parent, child) = source(&v);
        let mut session = EditSession::new();
        let bold = TextIntent::capture(&s, &child, 100)
            .unwrap()
            .prepare(
                &s,
                &mut session,
                &TextDraft::from_rtml(v["finalRtml"].as_str().unwrap()).unwrap(),
                100,
            )
            .unwrap();
        let rich_child = canonical(&s, &bold.document_data().message(1).unwrap().unwrap());
        let before_wire = s.section(&child).unwrap().crdt().unwrap().unwrap();
        let before_doc = Automerge::load(before_wire.bytes(5).unwrap().unwrap()).unwrap();
        let rich_wire = rich_child.crdt().unwrap().unwrap();
        let rich_doc = Automerge::load(rich_wire.bytes(5).unwrap().unwrap()).unwrap();
        assert!(
            before_doc
                .get_changes(&[])
                .iter()
                .all(|c| rich_doc.get_change_by_hash(&c.hash()).is_some())
        );
        assert!(
            !validation::processed(&rich_wire, v["originalTailHead"].as_str().unwrap()).unwrap()
        );
        let accepted = bold.validate_ack(&ack(&rich_child)).unwrap();
        let s = readback(&s, &rich_child);
        assert!(accepted.confirms_readback(&s).unwrap());
        session.remember_text_accepted(&accepted).unwrap();
        let intent = TextIntent::capture(&s, &parent, 100).unwrap();
        let desired = TextDraft::from_rtml("Owner edited after child Bold").unwrap();
        let edit = intent.prepare(&s, &mut session, &desired, 100).unwrap();
        let owner = canonical(&s, &edit.document_data().message(1).unwrap().unwrap());
        let accepted = edit.validate_ack(&ack(&owner)).unwrap();
        let current = readback(&s, &owner);
        assert!(accepted.confirms_readback(&current).unwrap());
        assert_eq!(current.section(&child), Some(&rich_child));
        let changed_peer = change(&rich_child, m(12, &body("Foreign child")));
        assert!(
            !accepted
                .confirms_readback(&readback(&current, &changed_peer))
                .unwrap_or(false)
        );
        let stale = readback(&s, &changed_peer);
        assert!(
            TextIntent::capture(&s, &parent, 100)
                .unwrap()
                .prepare(&stale, &mut EditSession::new(), &desired, 100)
                .is_err()
        );
        let deletion = prepare(&current, &parent);
        let checkpoint = deletion.checkpoint().unwrap();
        let restored: Checkpoint =
            serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
        assert_eq!(
            restored.split_owner_child().unwrap(),
            Some(rich_child.clone())
        );
        let mut tampered = serde_json::to_value(&restored).unwrap();
        for raw in tampered["sections"].as_array_mut().unwrap() {
            let peer =
                Section::decode(&serde_json::from_value::<Vec<u8>>(raw.clone()).unwrap()).unwrap();
            if peer.id() == child {
                *raw = serde_json::to_value(
                    change(&peer, m(12, &body("Forged rich peer"))).raw().raw(),
                )
                .unwrap();
            }
        }
        let tampered: Checkpoint = serde_json::from_value(tampered).unwrap();
        assert!(tampered.split_owner_child().is_err());
        let deleted = canonical(
            &current,
            &deletion.document_data().message(1).unwrap().unwrap(),
        );
        let accepted = deletion.validate_ack(&ack(&deleted)).unwrap();
        let final_snapshot = readback(&current, &deleted);
        assert!(accepted.confirms_readback(&final_snapshot).unwrap());
        assert_eq!(final_snapshot.section(&child), Some(&rich_child));
        assert_eq!(
            tombstone(&final_snapshot.sections, &deleted).unwrap(),
            rich_child
        );
        let path =
            std::env::temp_dir().join(format!("live-rich-owner-{}.sqlite", uuid::Uuid::new_v4()));
        let account = "d1b586dc-c6d3-4615-8316-a60de869a5e8";
        let mut journal = Journal::open(&path, account).unwrap();
        let Begin::Started(receipt) = journal.begin(restored).unwrap() else {
            panic!("new journal")
        };
        drop(journal);
        let journal = Journal::open(&path, account).unwrap();
        let stored = journal.get(receipt.checkpoint().scope()).unwrap().unwrap();
        let review = journal.local_review(&stored, &final_snapshot).unwrap();
        assert_eq!(review.split_owner_child(), Some(&rich_child));
        assert!(journal.get(receipt.checkpoint().scope()).unwrap().is_some());
        drop(review);
        drop(journal);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn live_rich_pair_refuses_foreign_edges_ambiguous_members_and_expired_or_pending_source() {
    let v = vectors()
        .into_iter()
        .find(|v| {
            v["cadence"] == "revert" && v["format"] == "bold" && v["removeFormatting"] == false
        })
        .unwrap();
    let (s, parent, child) = source(&v);
    let row = canonical(
        &s,
        &decoded(&v["childEditWire"]).message(1).unwrap().unwrap(),
    );
    let s = readback(&s, &row);
    let owner = s.section(&parent).unwrap();
    let wire = owner.crdt().unwrap().unwrap();
    let info = wire.message(10).unwrap().unwrap();
    let unrelated = Section::from_raw(owner.raw().remove(38).unwrap()).unwrap();
    let unrelated = change(
        &change(&change(&unrelated, t(1, "unrelated")), t(8, "aai")),
        t(21, "aai"),
    );
    let mut independent = s.clone();
    independent.sections.push(unrelated);
    assert!(split_child(&independent.sections, owner, 100).is_ok());
    for target in [&parent, &child] {
        let incoming = wire
            .replace_fields(
                10,
                &[m(10, &info.replace_fields(3, &[t(3, target)]).unwrap())],
            )
            .unwrap();
        let stranger = change(
            &change(
                &change(&change(owner, t(1, "foreign")), t(8, "aai")),
                t(21, "aai"),
            ),
            m(38, &incoming),
        );
        let mut bad = s.clone();
        bad.sections.push(stranger);
        assert!(split_child(&bad.sections, owner, 100).is_err());
    }
    let mut duplicate = s.clone();
    duplicate.sections.push(owner.clone());
    assert!(split_child(&duplicate.sections, owner, 100).is_err());
    let child_wire = row.crdt().unwrap().unwrap();
    let child_edge = child_wire
        .replace_fields(
            10,
            &[m(10, &info.replace_fields(3, &[t(3, &parent)]).unwrap())],
        )
        .unwrap();
    assert!(
        split_child(
            &readback(&s, &change(&row, m(38, &child_edge))).sections,
            owner,
            100
        )
        .is_err()
    );
    for target in [owner, &row] {
        let wire = target.crdt().unwrap().unwrap();
        for field in [n(4, 99), t(12, wire.string(8).unwrap().unwrap())] {
            let bad_wire = wire
                .merge_partial(&RawMessage::decode(field.raw()).unwrap())
                .unwrap();
            let bad = readback(&s, &change(target, m(38, &bad_wire)));
            assert!(split_child(&bad.sections, bad.section(&parent).unwrap(), 100).is_err());
        }
    }
}
