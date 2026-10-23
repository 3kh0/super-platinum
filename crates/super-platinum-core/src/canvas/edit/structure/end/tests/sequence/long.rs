use super::*;
use crate::canvas::edit::structure::end::sequence::budget;

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("long/original.json")).unwrap()
}

#[test]
fn long_sequence_eighteen_original_vectors_preserve_late_reports_all_rows_and_later_edits() {
    let e = evidence();
    assert_eq!(
        e["contractSha256"],
        "7d3277c529d0564e619e104d2716a2704041ee366d3d7c24fa0ac10131097479"
    );
    let vectors = e["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 18);
    check_vectors(&e);
    for (i, v) in vectors.iter().enumerate() {
        let s = source(&e["bindings"][i]);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = attempt(&s, &id, &reports(v));
        let rows = p.data.messages(1).unwrap();
        for (index, row) in rows.iter().enumerate() {
            let expected = &v["rows"][index];
            let wire = row.message(38).unwrap().unwrap();
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            let changes = doc.get_changes(&[]);
            let history = expected["history"].as_array().unwrap();
            let mapped = |head: &str| {
                let n = history.iter().position(|h| h["hash"] == head).unwrap();
                changes[n].hash().to_string()
            };
            let expected_processed = expected["session"]["processed_heads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| mapped(h.as_str().unwrap()))
                .collect::<Vec<_>>();
            let processed = wire
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
                .collect::<Vec<_>>();
            assert_eq!(
                processed, expected_processed,
                "vector{i}/row{index}/processed order"
            );
            for (tag, name) in [(8, "base_head"), (12, "current_head")] {
                assert_eq!(
                    wire.string(tag).unwrap().map(str::to_owned),
                    expected["session"][name].as_str().map(mapped),
                    "vector{i}/row{index}/{name}"
                );
            }
        }
        if let Some(target) = v["targetIndex"].as_u64() {
            assert!(matches!(target, 5 | 14));
            let target = target as usize;
            let member = p.appended_members().nth(target).unwrap();
            let doc = Automerge::load(
                rows[target + 1]
                    .message(38)
                    .unwrap()
                    .unwrap()
                    .bytes(5)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            let count = match v["cadence"].as_str().unwrap() {
                "correction-before-enter" => 8,
                "erase-before-enter" => 3,
                "revert-before-enter" => 5,
                _ => panic!("unknown targeted reference"),
            };
            assert_eq!(doc.get_changes(&[]).len(), count);
            assert!(rows[target + 1].has(12));
            if count != 8 {
                assert!(member.draft() == &TextDraft::default());
            }
            assert!(target + 1 < p.appended_members().len());
        }
    }
}

#[test]
fn long_sequence_existing_event_budget_preserves_old_maximum_and_bounds_empty_appends() {
    let d = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}");
    assert_eq!(
        split::plain(&d).unwrap(),
        "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"
    );
    assert_eq!(split::plain(&d).unwrap().encode_utf16().count(), 11);
    let updates = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    // Every formerly valid three-Enter +4096-report witness remains admitted.
    assert!(EndWitness::sequence(&d, 11, &[updates.clone(), vec![], vec![]]).is_ok());
    assert!(EndWitness::sequence(&d, 11, &[updates, vec![], vec![], vec![]]).is_err());
    assert!(EndWitness::sequence(&d, 11, &vec![vec![]; 4099]).is_ok());
    assert!(EndWitness::sequence(&d, 11, &vec![vec![]; 4100]).is_err());
    assert!(EndWitness::sequence(&d, 11, &vec![vec![draft("x")]; 2050]).is_err());
    assert!(EndWitness::sequence(&d, 11, &vec![vec![draft("x")]; 2049]).is_ok());
    let s = fixture("Keep this", false);
    assert!(EndIntent::capture_sequence(&s, "item", usize::MAX, 100).is_err());
    let e = evidence();
    for index in [6, 7] {
        let s = source(&e["bindings"][index]);
        let intent = EndIntent::capture_sequence(&s, "paragraph", 4099, 100).unwrap();
        assert_eq!(intent.sequence_members.as_ref().unwrap().len(), 4099);
        assert!(EndIntent::capture_sequence(&s, "paragraph", 4100, 100).is_err());
    }
    // Document capacity counts newly created rows, not an already present reuse.
    assert!(budget::inventory(19_999, 1, false).is_ok());
    assert!(budget::inventory(19_999, 2, false).is_err());
    assert!(budget::inventory(19_999, 2, true).is_ok());
    assert!(budget::inventory(20_000, 2, true).is_err());
    assert!(budget::inventory(usize::MAX, 1, false).is_err());
}

#[test]
fn long_sequence_late_ack_readback_and_neighbor_corruption_stay_unconfirmed() {
    let e = evidence();
    for index in [9, 12, 14, 17] {
        let v = &e["vectors"][index];
        let s = source(&e["bindings"][index]);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = attempt(&s, &id, &reports(v));
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        for k in [0, 1, rows.len() - 2, rows.len() - 1] {
            let mut bad = rows.clone();
            bad.remove(k);
            assert!(p.validate_ack(&ack(&bad)).is_err());
            for field in [
                m(12, &body("altered", false)),
                n(2, 0),
                t(7, "wrong-document"),
            ] {
                let mut bad = rows.clone();
                bad[k] = change(&bad[k], field.clone());
                assert!(p.validate_ack(&ack(&bad)).is_err(), "vector{index}/row{k}");
                assert!(
                    !accepted
                        .confirms_readback(&replace(&fresh, rows[k].id(), field))
                        .unwrap()
                );
            }
        }
        let last = rows.last().unwrap();
        let wire = last
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(8, &[t(8, &"0".repeat(64))])
            .unwrap();
        assert!(
            !accepted
                .confirms_readback(&replace(&fresh, last.id(), m(38, &wire)))
                .unwrap()
        );
        let mut extra = rows.clone();
        extra.push(s.section("title").unwrap().clone());
        assert!(p.validate_ack(&ack(&extra)).is_err());
        let mut dup = rows.clone();
        dup[rows.len() - 1] = dup[1].clone();
        let raw = msg(&[
            n(1, 1),
            m(
                2,
                &msg(&dup.iter().map(|r| m(1, r.raw())).collect::<Vec<_>>()),
            ),
        ]);
        assert!(EditAck::decode(raw.raw()).is_err());
        for neighbor in s
            .sections
            .iter()
            .filter(|r| r.id() != id && r.id() != p.child_section())
        {
            assert!(
                !accepted
                    .confirms_readback(&replace(&fresh, neighbor.id(), n(2, 2000)))
                    .unwrap()
            );
        }
    }
}

#[test]
fn long_sequence_v14_restart_review_rollback_and_archive_retain_whole_operation() {
    let e = evidence();
    super::recovery::check_restart(&e, &[0, 9, 12, 14, 16, 17]);
}

#[test]
fn long_sequence_recovery_preflights_serialized_report_and_event_budgets() {
    use crate::canvas::edit::structure::recovery::{Checkpoint, journal::Journal};
    let e = evidence();
    let s = source(&e["bindings"][6]);
    let p = attempt(&s, "paragraph", &reports(&e["vectors"][6]));
    let original = serde_json::to_value(p.checkpoint().unwrap()).unwrap();
    for kind in 0..3 {
        let mut bad = original.clone();
        let members = &mut bad["action"]["EndSequence"]["members"];
        match kind {
            0 => {
                let member = members[0].clone();
                *members = serde_json::json!(vec![member; 4099]);
            }
            1 => members[0]["updates"] = serde_json::json!(vec!["x"; 4097]),
            _ => members[0]["updates"] = serde_json::json!(["x".repeat(8 * 1024 * 1024 + 1)]),
        }
        let checkpoint: Checkpoint = serde_json::from_value(bad).unwrap();
        let path = std::env::temp_dir().join(format!(
            "canvas-long-budget-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        {
            let mut journal = Journal::open(&path, "00000000-0000-4000-8000-000000000800").unwrap();
            assert!(journal.begin(checkpoint).is_err(), "tamper{kind}");
            assert!(
                journal
                    .get(p.checkpoint().unwrap().scope())
                    .unwrap()
                    .is_none()
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}
