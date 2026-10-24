use super::pre_enter::{history, reports};
use super::*;
use serde_json::Value;
mod recovery;
const NOW: u64 = 1_792_000_000_000_000;
fn vectors() -> Value {
    serde_json::from_str(include_str!("sequence/original.json")).unwrap()
}
fn draft(value: &str) -> TextDraft {
    TextDraft::from_rtml(value).unwrap()
}
fn attempt(v: &Value) -> (CanvasCollabSnapshot, PreparedSplitEdit) {
    let (snapshot, first) = super::pre_enter::attempt(v);
    let w = first
        .witness
        .clone()
        .with_parent_updates(&reports(&v["between"][0]))
        .unwrap()
        .with_child_updates(&reports(&v["between"][1]))
        .unwrap();
    let target = if v["secondTarget"] == "parent" {
        SplitSequenceTarget::Original
    } else {
        SplitSequenceTarget::FirstChild
    };
    let enter = if target == SplitSequenceTarget::Original {
        w.parent_draft().clone()
    } else {
        w.child_draft().clone()
    };
    let w = w
        .with_second_split(
            target,
            &enter,
            v["secondOffset"].as_u64().unwrap() as usize,
            &draft(v["secondLeft"].as_str().unwrap()),
            &draft(v["secondTail"].as_str().unwrap()),
        )
        .unwrap()
        .with_final_updates(
            &reports(&v["final"][0]),
            &reports(&v["final"][1]),
            &reports(&v["final"][2]),
        )
        .unwrap();
    let p = PreparedSplitEdit::build(Arc::clone(&first.intent), w, NOW).unwrap();
    assert_eq!(p.lineage, first.lineage);
    (snapshot, p)
}
#[test]
fn original_24_second_middle_vectors_preserve_three_histories_and_both_live_boundaries() {
    let evidence = vectors();
    assert_eq!(evidence["count"], 24);
    assert_eq!(
        evidence["provenance"]["bindings"],
        "3330c3e6aee3fb948aefafc76527c6620e651b3e66de5f550d604ff1c3499cc5"
    );
    for (ordinal, v) in evidence["vectors"].as_array().unwrap().iter().enumerate() {
        let (s, p) = attempt(v);
        let second = p.second.as_ref().unwrap();
        let wires = [&p.parent_wire, &p.child_wire, &second.wire];
        assert_eq!(p.member_ids().len(), 3);
        assert_ne!(p.child_section(), p.second_child_section().unwrap());
        assert_eq!(
            p.visual_order(),
            if v["secondTarget"] == "parent" {
                vec![0, 2, 1]
            } else {
                vec![0, 1, 2]
            }
        );
        for (index, wire) in wires.into_iter().enumerate() {
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            history(&doc, &v["histories"][index]);
            assert_eq!(
                p.member_drafts()[index].rtml().unwrap(),
                v["finalRtml"][index].as_str().unwrap()
            );
            assert_eq!(
                wire.messages(10).unwrap().len(),
                v["splitCounts"][index].as_u64().unwrap() as usize
            );
            assert!(validation::processed(wire, &doc.get_heads()[0].to_string()).unwrap());
        }
        for (i, info) in [&p.lineage, &second.lineage].into_iter().enumerate() {
            history(
                &Automerge::load(info.bytes(7).unwrap().unwrap()).unwrap(),
                &v["liveHistories"][i],
            );
        }
        assert_eq!(
            second.lineage.uint(4).unwrap(),
            v["serializedOffset"].as_u64()
        );
        assert_eq!(
            second.lineage.uint(1).unwrap(),
            Some(if v["secondTarget"] == "parent" { 1 } else { 0 })
        );
        // Identical bootstrap heads do not identify either new section.
        assert_eq!(
            p.child_wire.string(8).unwrap(),
            second.wire.string(8).unwrap()
        );
        let c = p
            .checkpoint()
            .unwrap_or_else(|e| panic!("vector {ordinal} checkpoint: {e:?}"));
        let value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], 18);
        assert_eq!(
            value["action"]["SplitSequence"]["final_updates"],
            v["final"]
        );
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let current = readback(&s, &rows);
        assert!(accepted.confirms_readback(&current).unwrap());
        for missing in 0..3 {
            let partial = rows
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != missing)
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>();
            assert!(p.validate_ack(&ack(&partial)).is_err());
            assert!(
                !accepted
                    .confirms_readback(&readback(&s, &partial))
                    .unwrap_or(false)
            );
        }
        for id in p.member_ids() {
            assert!(
                crate::canvas::edit::text::TextIntent::capture(&current, id, 100).is_ok(),
                "vector {ordinal} later row {id}"
            );
        }
    }
}
#[test]
fn second_middle_boundaries_reports_and_scope_are_not_pair_fallbacks() {
    let e = vectors();
    let (s, p) = attempt(&e["vectors"][1]);
    let w = p.witness.clone();
    assert!(w.clone().with_parent_updates(&[]).is_err());
    assert!(w.clone().with_child_updates(&[]).is_err());
    assert!(
        w.clone()
            .with_second_split(
                SplitSequenceTarget::Original,
                &draft("ab"),
                1,
                &draft("a"),
                &draft("b")
            )
            .is_err()
    );
    let mut first = w.clone();
    first.second = None;
    let live = first.parent_draft().clone();
    assert!(
        first
            .clone()
            .with_second_split(
                SplitSequenceTarget::Original,
                &draft("wrong"),
                1,
                &draft("w"),
                &draft("rong")
            )
            .is_err()
    );
    assert!(
        first
            .clone()
            .with_second_split(SplitSequenceTarget::Original, &live, 0, &draft(""), &live)
            .is_err()
    );
    let many = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(w.clone().with_final_updates(&many, &[], &[]).is_err());
    let mut changed = s.clone();
    let row = changed
        .sections
        .iter_mut()
        .find(|s| s.id() == p.target_section())
        .unwrap();
    *row = change(row, n(2, 2000));
    assert!(p.validate_snapshot(&changed, 100).is_err());
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let mut duplicate = rows.clone();
    duplicate[2] = duplicate[1].clone();
    // Duplicate section IDs are refused by the wire codec before an EditAck
    // exists. Do not unwrap that earlier refusal through the positive helper.
    let duplicate_bytes = msg(&[
        n(1, 1),
        m(
            2,
            &msg(&duplicate.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
        ),
    ]);
    assert!(matches!(
        EditAck::decode(duplicate_bytes.raw()),
        Err(crate::canvas::wire::Error::Duplicate)
    ));
    let mut forged = rows.clone();
    let wire = forged[0].crdt().unwrap().unwrap();
    let infos = wire.messages(10).unwrap();
    let bad = wire
        .replace_fields(10, &[m(10, &infos[1]), m(10, &infos[0])])
        .unwrap();
    forged[0] = change(&forged[0], m(38, &bad));
    assert!(p.validate_ack(&ack(&forged)).is_err());
    assert!(
        !accepted
            .confirms_readback(&readback(&s, &forged))
            .unwrap_or(false)
    );
}
