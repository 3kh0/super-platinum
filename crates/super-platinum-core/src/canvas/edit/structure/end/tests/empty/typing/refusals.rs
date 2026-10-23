use super::*;

#[test]
fn empty_typing_bounds_and_report_history_cannot_collapse_to_final_text() {
    for values in [
        vec![],
        vec![draft("")],
        vec![draft("x"), draft("x")],
        vec![draft("<b>x</b>")],
        vec![draft("a<br>b")],
        vec![draft("a\u{feff}b")],
    ] {
        assert!(EndWitness::empty().with_child_updates(&values).is_err());
    }
    let too_many = (0..4097)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "" }))
        .collect::<Vec<_>>();
    assert!(EndWitness::empty().with_child_updates(&too_many).is_err());
    let large = [draft(&"x".repeat(32768)), draft(&"y".repeat(32768))];
    let reports = (0..257).map(|i| large[i % 2].clone()).collect::<Vec<_>>();
    assert!(EndWitness::empty().with_child_updates(&reports).is_err());
    let exact = draft("e\u{301} 👩🏽\u{200d}💻 &amp; 日本");
    assert!(
        EndWitness::empty()
            .with_child_updates(&[exact.clone(), draft("")])
            .is_ok()
    );
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            let s = fixture_from_reference(mode, layout);
            let reports = [draft("x"), draft("x draftx"), draft("x"), draft("")];
            let p = typed(&s, source_id(mode), &reports);
            let seed = initial_seed(&p, &s);
            for false_reports in [
                vec![draft("")],
                vec![draft("x"), draft("")],
                vec![draft("x"), draft("x draftx"), draft("")],
            ] {
                assert!(
                    proof::validate_reports(
                        &seed,
                        &p.child_wire,
                        p.reuses_child(),
                        &false_reports,
                        p.session_id()
                    )
                    .is_err(),
                    "{mode}/{layout}/reports"
                );
            }
            for (name, bad) in [
                ("base", p.child_wire.replace_fields(2, &[n(2, 7)]).unwrap()),
                ("saved", p.child_wire.replace_fields(3, &[n(3, 7)]).unwrap()),
                ("processed", p.child_wire.remove(6).unwrap()),
                (
                    "base-head",
                    p.child_wire
                        .replace_fields(8, &[t(8, &"0".repeat(64))])
                        .unwrap(),
                ),
                (
                    "lineage",
                    p.child_wire
                        .replace_fields(10, &[m(10, &msg(&[]))])
                        .unwrap(),
                ),
            ] {
                assert!(
                    proof::validate_reports(
                        &seed,
                        &bad,
                        p.reuses_child(),
                        &reports,
                        p.session_id()
                    )
                    .is_err(),
                    "{mode}/{layout}/{name}"
                );
            }
            let mut other_actor = p.session_id().to_owned();
            other_actor.replace_range(..1, if &other_actor[..1] == "0" { "1" } else { "0" });
            assert!(
                proof::validate_reports(
                    &seed,
                    &p.child_wire,
                    p.reuses_child(),
                    &reports,
                    &other_actor
                )
                .is_err()
            );
            let ordinary = EndIntent::capture(&s, source_id(mode), 100);
            assert!(ordinary.is_err());
            for id in [source_id(mode), "title"] {
                assert!(
                    p.validate_snapshot(&replace(&s, id, n(2, 2000)), 100)
                        .is_err()
                );
            }
            if p.reuses_child() {
                assert!(
                    p.validate_snapshot(&replace(&s, p.child_section(), n(2, 2000)), 100)
                        .is_err()
                );
            }
            if mode != "bootstrap" {
                assert!(p.validate_snapshot(&s, 1_000_000).is_err());
            }
        }
    }
}

#[test]
fn empty_typing_complete_ack_and_readback_require_changed_body_even_after_erase() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            for reports in [
                vec![draft("New 日本😀 &amp; text")],
                vec![draft("x"), draft("")],
            ] {
                let s = fixture_from_reference(mode, layout);
                let p = typed(&s, source_id(mode), &reports);
                let rows = pair(&p, &s);
                assert!(p.validate_ack(&ack(&rows[..1])).is_err());
                assert!(p.validate_ack(&ack(&rows[1..])).is_err());
                let duplicate = msg(&[
                    n(1, 1),
                    m(2, &msg(&[m(1, rows[0].raw()), m(1, rows[0].raw())])),
                ]);
                assert!(EditAck::decode(duplicate.raw()).is_err());
                let mut missing_body = rows.clone();
                missing_body[1] = Section::from_raw(rows[1].raw().remove(12).unwrap()).unwrap();
                assert!(
                    p.validate_ack(&ack(&missing_body)).is_err(),
                    "{mode}/{layout}/explicit child body"
                );
                for i in 0..2 {
                    for (label, field) in [
                        ("wrong body", m(12, &body("wrong", true))),
                        ("session", t(33, "wrong")),
                        ("local sequence", n(20, 2)),
                        ("modified", n(19, 0)),
                        ("owner", t(7, "wrong")),
                    ] {
                        let mut bad = rows.clone();
                        bad[i] = change(&bad[i], field);
                        assert!(
                            p.validate_ack(&ack(&bad)).is_err(),
                            "{mode}/{layout}/{i}/{label}"
                        );
                    }
                    let text = rows[i]
                        .raw()
                        .message(12)
                        .unwrap()
                        .unwrap()
                        .message(1)
                        .unwrap()
                        .unwrap();
                    let mut bad = rows.clone();
                    bad[i] = change(
                        &bad[i],
                        m(12, &body(text.string(1).unwrap().unwrap(), !text.has(2))),
                    );
                    assert!(p.validate_ack(&ack(&bad)).is_err());
                    let w = rows[i].crdt().unwrap().unwrap();
                    for corrupted in [
                        w.remove(6).unwrap(),
                        w.replace_fields(2, &[n(2, 7)]).unwrap(),
                        w.replace_fields(10, &[m(10, &msg(&[]))]).unwrap(),
                    ] {
                        let mut bad = rows.clone();
                        bad[i] = change(&bad[i], m(38, &corrupted));
                        assert!(p.validate_ack(&ack(&bad)).is_err());
                    }
                }
                // Metadata-only parent ACK is allowed, never independent body evidence.
                let mut partial_parent = rows.clone();
                partial_parent[0] = Section::from_raw(rows[0].raw().remove(12).unwrap()).unwrap();
                assert!(p.validate_ack(&ack(&partial_parent)).is_ok());
                let accepted = p.validate_ack(&ack(&rows)).unwrap();
                let fresh = readback(&s, &rows);
                assert!(accepted.confirms_readback(&fresh).unwrap());
                for i in 0..2 {
                    let mut absent = fresh.clone();
                    absent.sections.retain(|r| r.id() != rows[i].id());
                    assert!(!accepted.confirms_readback(&absent).unwrap_or(false));
                    let mut bad = rows.clone();
                    bad[i] = Section::from_raw(rows[i].raw().remove(12).unwrap()).unwrap();
                    assert!(
                        !accepted
                            .confirms_readback(&readback(&s, &bad))
                            .unwrap_or(false)
                    );
                }
                assert!(
                    !accepted
                        .confirms_readback(&replace(&fresh, "title", n(2, 2000)))
                        .unwrap_or(false)
                );
            }
        }
    }
}
