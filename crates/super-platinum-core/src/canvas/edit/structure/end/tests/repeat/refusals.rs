use super::*;

#[test]
fn repeat_witness_and_source_admission_preserve_explicit_mode_and_freshness() {
    let d = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}");
    assert_eq!(
        split::plain(&d).unwrap(),
        "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"
    );
    assert_eq!(split::plain(&d).unwrap().encode_utf16().count(), 11);
    let blank = TextDraft::default();
    assert!(EndWitness::repeat(&d, 11, &d, &blank, &blank).is_ok());
    for offset in [0, 10, 15, usize::MAX] {
        assert!(EndWitness::repeat(&d, offset, &d, &blank, &blank).is_err());
    }
    assert!(EndWitness::repeat(&d, 11, &draft("changed"), &blank, &blank).is_err());
    assert!(EndWitness::repeat(&d, 11, &d, &draft("typed"), &blank).is_err());
    assert!(EndWitness::repeat(&d, 11, &d, &blank, &draft("third")).is_err());
    assert!(EndWitness::repeat(&blank, 0, &blank, &blank, &blank).is_err());
    for value in ["<b>text</b>", "a<br>b", "a\u{feff}b", "a\u{2028}b"] {
        let value = draft(value);
        assert!(EndWitness::repeat(&value, 3, &value, &blank, &blank).is_err());
    }
    let w = EndWitness::repeat(&d, 11, &d, &blank, &blank).unwrap();
    assert!(w.clone().with_child_updates(&[]).is_err());
    assert!(w.with_child_updates(&[draft("x")]).is_err());
    // Nonempty rich content in an untouched neighbor is not a rich source or
    // a nonliteral-empty clearing operation. Preserve its full raw record.
    let s = replace(&source(2), "following", m(12, &body("<b>x</b>", true)));
    let neighbor = s.section("following").unwrap().clone();
    let p = repeated(&s, "paragraph");
    assert!(!p.reuses_child());
    assert!(
        p.data
            .messages(1)
            .unwrap()
            .iter()
            .all(|row| row.string(1).unwrap() != Some("following"))
    );
    assert_eq!(
        p.intent.inventory.sections.get("following"),
        Some(&neighbor)
    );
    let rows = pair(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let fresh = readback(&s, &rows);
    assert_eq!(
        fresh.section("following").unwrap().raw().raw(),
        neighbor.raw().raw()
    );
    assert!(accepted.confirms_readback(&fresh).unwrap());
    for index in [2, 8, 14] {
        let s = source(index);
        let mode = original()["vectors"][index]["mode"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = bound_id("paragraph", &mode);
        let child = bound_id("following", &mode);
        let p = repeated(&s, &id);
        // No translation between old single-End and explicit repeat witnesses.
        let single = witness(&p.intent.seed.rtml().unwrap());
        assert!(
            EndIntent::capture_repeat(&s, &id, 100)
                .unwrap()
                .prepare(&s, &single, 100)
                .is_err()
        );
        let mut tampered = p.witness.clone();
        tampered.updates = vec![draft("x")];
        assert!(
            EndIntent::capture_repeat(&s, &id, 100)
                .unwrap()
                .prepare(&s, &tampered, 100)
                .is_err()
        );
        for target in [&id, &child, &"title".to_owned()] {
            assert!(
                p.validate_snapshot(&replace(&s, target, n(2, 2000)), 100)
                    .is_err()
            );
        }
        for target in [&id, &child] {
            for (label, field) in [
                ("readonly", n(35, 1)),
                ("style", n(10, 1)),
                ("parents", m(13, &msg(&[t(1, "missing")]))),
                ("default", m(16, &msg(&[n(13, 1)]))),
                ("position", t(8, "aac:temp")),
            ] {
                assert!(
                    EndIntent::capture_repeat(&replace(&s, target, field), &id, 100).is_err(),
                    "{index}/{target}/{label}"
                );
            }
            if let Some(wire) = s.section(target).unwrap().crdt().unwrap() {
                for (label, bad) in [
                    ("missing lease", wire.remove(4).unwrap()),
                    ("expired", wire.replace_fields(4, &[n(4, 100)]).unwrap()),
                    ("processed", wire.remove(6).unwrap()),
                    (
                        "future saved",
                        wire.replace_fields(3, &[n(3, 1001)]).unwrap(),
                    ),
                    (
                        "split",
                        wire.replace_fields(10, &[m(10, &msg(&[]))]).unwrap(),
                    ),
                ] {
                    assert!(
                        EndIntent::capture_repeat(&replace(&s, target, m(38, &bad)), &id, 100)
                            .is_err(),
                        "{index}/{target}/{label}"
                    );
                }
            }
        }
        for unsupported in [" ", "&nbsp;", "<br>"] {
            assert!(
                EndIntent::capture_repeat(
                    &replace(&s, &child, m(12, &body(unsupported, true))),
                    &id,
                    100
                )
                .is_err(),
                "{index}/nonliteral-empty-neighbor/{unsupported}"
            );
        }
        assert!(
            EndIntent::capture_repeat(&replace(&s, &id, m(12, &body("", true))), &id, 100).is_err()
        );
        if mode != "bootstrap" {
            assert!(p.validate_snapshot(&s, 1_000_000).is_err());
        }
    }
}

#[test]
fn repeat_ack_requires_all_three_exact_members_and_independent_complete_readback() {
    for index in [0, 2, 6, 8, 12, 14, 18] {
        let s = source(index);
        let mode = original()["vectors"][index]["mode"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = bound_id("paragraph", &mode);
        let p = repeated(&s, &id);
        let rows = pair(&p, &s);
        for missing in 0..3 {
            let mut partial = rows.clone();
            partial.remove(missing);
            assert!(p.validate_ack(&ack(&partial)).is_err());
        }
        let duplicate = msg(&[
            n(1, 1),
            m(
                2,
                &msg(&[
                    m(1, rows[0].raw()),
                    m(1, rows[1].raw()),
                    m(1, rows[1].raw()),
                ]),
            ),
        ]);
        assert!(EditAck::decode(duplicate.raw()).is_err());
        for i in 0..3 {
            for (label, field) in [
                ("body", m(12, &body("changed", true))),
                ("trace", t(33, "wrong")),
                ("counter", n(20, 2)),
                ("modified", n(19, 0)),
                ("owner", t(7, "wrong")),
                ("readonly", n(35, 1)),
                ("position", t(8, "zzz")),
            ] {
                let mut bad = rows.clone();
                bad[i] = change(&bad[i], field);
                assert!(
                    p.validate_ack(&ack(&bad)).is_err(),
                    "{index}/row{i}/{label}"
                );
            }
            let body_rtml = if i == 0 {
                p.intent.seed.rtml().unwrap()
            } else {
                String::new()
            };
            let language = rows[i]
                .raw()
                .message(12)
                .unwrap()
                .unwrap()
                .message(1)
                .unwrap()
                .unwrap()
                .has(2);
            let mut bad = rows.clone();
            bad[i] = change(&bad[i], m(12, &body(&body_rtml, !language)));
            assert!(p.validate_ack(&ack(&bad)).is_err());
            let wire = rows[i].crdt().unwrap().unwrap();
            for bad_wire in [
                wire.remove(6).unwrap(),
                wire.replace_fields(2, &[n(2, 7)]).unwrap(),
                wire.replace_fields(8, &[t(8, &"0".repeat(64))]).unwrap(),
                wire.replace_fields(10, &[m(10, &msg(&[]))]).unwrap(),
            ] {
                let mut bad = rows.clone();
                bad[i] = change(&bad[i], m(38, &bad_wire));
                assert!(
                    p.validate_ack(&ack(&bad)).is_err(),
                    "{index}/row{i}/history"
                );
            }
        }
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        for i in 0..3 {
            let mut metadata = rows.clone();
            metadata[i] = Section::from_raw(metadata[i].raw().remove(12).unwrap()).unwrap();
            assert_eq!(
                p.validate_ack(&ack(&metadata)).is_ok(),
                i == 0 || (i == 1 && p.reuses_child())
            );
            assert!(
                !accepted
                    .confirms_readback(&readback(&s, &metadata))
                    .unwrap_or(false)
            );
            let mut missing = fresh.clone();
            missing.sections.retain(|r| r.id() != rows[i].id());
            assert!(!accepted.confirms_readback(&missing).unwrap_or(false));
        }
        for row in &fresh.sections {
            assert!(
                !accepted
                    .confirms_readback(&replace(&fresh, row.id(), n(2, 50000)))
                    .unwrap_or(false)
            );
        }
    }
}
