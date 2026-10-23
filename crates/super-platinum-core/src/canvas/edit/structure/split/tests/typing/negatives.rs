use super::*;
use automerge::transaction::{CommitOptions, Transactable};

#[test]
fn typed_split_ack_and_full_readback_bind_final_body_pair_and_history() {
    for v in vectors()["vectors"].as_array().unwrap() {
        let (s, p) = attempt(v);
        let rows = pair(&p, &s);
        for partial in [vec![], vec![rows[0].clone()], vec![rows[1].clone()]] {
            assert!(p.validate_ack(&ack(&partial)).is_err());
        }
        let mut bad = rows.clone();
        bad[1] = Section::from_raw(bad[1].raw().remove(12).unwrap()).unwrap();
        assert!(p.validate_ack(&ack(&bad)).is_err());
        for (index, field) in [
            (1, m(12, &body(&p.child_draft().rtml().unwrap(), true))),
            (1, m(12, &body("wrong child", false))),
            (1, t(6, "foreign-thread")),
            (1, t(8, "zzz")),
            (1, n(20, 2)),
            (0, t(33, "wrong-0")),
        ] {
            let mut bad = rows.clone();
            bad[index] = change(&bad[index], field);
            assert!(p.validate_ack(&ack(&bad)).is_err());
        }
        let wire = rows[1].crdt().unwrap().unwrap();
        let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let tail = doc.get_changes(&[])[1].hash().to_string();
        // Historical tail alone cannot substitute for the actually sent final head.
        let bad_wire = wire.replace_fields(6, &[t(6, &tail)]).unwrap();
        let mut bad = rows.clone();
        bad[1] = change(&bad[1], m(38, &bad_wire));
        assert!(p.validate_ack(&ack(&bad)).is_err());
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        for id in ["item", p.child_section(), "other", "title"] {
            let mut missing = fresh.clone();
            missing.sections.retain(|r| r.id() != id);
            assert!(!accepted.confirms_readback(&missing).unwrap());
        }
        assert!(
            !accepted
                .confirms_readback(&readback(&s, &bad))
                .unwrap_or(false)
        );
        assert!(p.checkpoint().is_ok()); // failed ACK never consumes the whole attempt
    }
}

#[test]
fn typed_split_lineage_rejects_unbound_child_history_branch_or_nontext_descendants() {
    let v = &vectors()["vectors"][0];
    let (s, p) = attempt(v);
    let rows = pair(&p, &s);
    let good = readback(&s, &rows);
    let wire = rows[1].crdt().unwrap().unwrap();
    let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
    let tail = doc.get_changes(&[])[1].hash();
    for kind in 0..6 {
        let bad_wire = match kind {
            0 => wire.remove(4).unwrap(),
            1 => wire.replace_fields(4, &[n(4, 100)]).unwrap(),
            2 => wire.replace_fields(6, &[t(6, &tail.to_string())]).unwrap(),
            3 => {
                let replacement =
                    crdt::insert(p.child_section(), p.child_draft(), p.session_id(), 100).unwrap();
                replacement
                    .remove(12)
                    .unwrap()
                    .replace_fields(3, &[n(3, 1000)])
                    .unwrap()
                    .replace_fields(4, &[n(4, 1_000_000)])
                    .unwrap()
            }
            _ => {
                let mut changed = if kind == 4 {
                    doc.clone()
                } else {
                    doc.fork_at(&[tail]).unwrap()
                }
                .with_actor("aaaaaaaaaaaa00000000".parse().unwrap());
                let mut tx = changed.transaction();
                tx.put(automerge::ROOT, "deleted", true).unwrap();
                tx.commit_with(CommitOptions::default());
                let mut tx = changed.transaction();
                tx.put(automerge::ROOT, "deleted", false).unwrap();
                tx.commit_with(CommitOptions::default());
                if kind == 5 {
                    changed.merge(&mut doc.clone()).unwrap();
                }
                wire.replace_fields(5, &[F::bytes(5, &changed.save()).unwrap()])
                    .unwrap()
                    .replace_fields(
                        6,
                        &changed
                            .get_heads()
                            .iter()
                            .map(|h| t(6, &h.to_string()))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap()
            }
        };
        let bad = readback(&good, &[change(&rows[1], m(38, &bad_wire))]);
        assert!(
            TextIntent::capture(&bad, "item", 100).is_err(),
            "history case {kind}"
        );
    }
    let intent = TextIntent::capture(&good, "item", 100).unwrap();
    let stale = readback(&good, &[change(&rows[1], n(2, 2000))]);
    assert!(
        intent
            .prepare(
                &stale,
                &mut EditSession::new(),
                &draft("parent edited"),
                100
            )
            .is_err()
    );
    for id in ["item", "other", "title"] {
        let mut fresh = s.clone();
        let row = fresh.sections.iter_mut().find(|r| r.id() == id).unwrap();
        *row = change(row, n(2, 20000));
        assert!(p.validate_snapshot(&fresh, 100).is_err());
    }
}
