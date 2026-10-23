use super::*;

fn repeated_witness() -> EndWitness {
    let before = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}");
    assert_eq!(
        split::plain(&before).unwrap(),
        "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}"
    );
    EndWitness::repeat(
        &before,
        11,
        &before,
        &TextDraft::default(),
        &TextDraft::default(),
    )
    .unwrap()
}
#[test]
fn repeat_third_reports_refuse_other_modes_noops_rich_controls_and_stale_sources() {
    assert!(repeated_witness().with_second_child_updates(&[]).is_err());
    assert!(
        repeated_witness()
            .with_child_updates(&[draft("x")])
            .is_err()
    );
    assert!(
        EndWitness::empty()
            .with_second_child_updates(&[draft("x")])
            .is_err()
    );
    assert!(
        witness("Keep this")
            .with_second_child_updates(&[draft("x")])
            .is_err()
    );
    for updates in [
        vec![draft("")],
        vec![draft("x"), draft("x")],
        vec![draft("<b>x</b>")],
        vec![draft("x<br>y")],
        vec![draft("a\u{feff}b")],
        vec![draft("a\u{2028}b")],
        vec![draft("x"); 4097],
    ] {
        assert!(
            repeated_witness()
                .with_second_child_updates(&updates)
                .is_err()
        );
    }
    let w = repeated_witness()
        .with_second_child_updates(&[draft("x"), draft("")])
        .unwrap();
    assert!(w.is_repeat_typing() && w.is_repeat() && w.updates.is_empty());
    assert!(w.with_child_updates(&[]).is_err());
    for index in [0, 3, 12, 15, 24, 27] {
        let e = evidence();
        let v = &e["vectors"][index];
        let b = &e["bindings"][index];
        let s = source(b["priorVector"].as_u64().unwrap() as usize);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = typed(&s, &id, &reports(v));
        assert!(
            p.validate_snapshot(&replace(&s, &id, n(2, 2000)), 100)
                .is_err()
        );
        assert!(
            p.validate_snapshot(&replace(&s, "title", m(12, &body("Different", true))), 100)
                .is_err()
        );
        if p.reuses_child() {
            assert!(
                p.validate_snapshot(
                    &replace(&s, p.child_section(), m(12, &body("changed", true))),
                    100
                )
                .is_err()
            );
            if let Some(wire) = s.section(p.child_section()).unwrap().crdt().unwrap() {
                assert!(
                    p.validate_snapshot(
                        &replace(
                            &s,
                            p.child_section(),
                            m(38, &wire.replace_fields(4, &[n(4, 99)]).unwrap())
                        ),
                        100
                    )
                    .is_err()
                );
            }
        }
        assert!(EndIntent::capture_repeat(&replace(&s, &id, n(35, 1)), &id, 100).is_err());
    }
}

#[test]
fn repeat_typed_ack_readback_require_all_three_and_exact_third_empty_or_text_history() {
    let e = evidence();
    for index in [0, 1, 2, 3, 4, 5, 24, 25, 26, 27, 28, 29] {
        let v = &e["vectors"][index];
        let b = &e["bindings"][index];
        let s = source(b["priorVector"].as_u64().unwrap() as usize);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = typed(&s, &id, &reports(v));
        let good = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&good)).unwrap();
        for missing in 0..3 {
            let mut partial = good.clone();
            partial.remove(missing);
            assert!(
                p.validate_ack(&ack(&partial)).is_err(),
                "{index}/missing{missing}"
            );
            let mut canonical = readback(&s, &good);
            canonical.sections.retain(|r| r.id() != good[missing].id());
            assert!(!accepted.confirms_readback(&canonical).unwrap());
        }
        let duplicate = msg(&[
            n(1, 1),
            m(
                2,
                &msg(&[
                    m(1, good[0].raw()),
                    m(1, good[1].raw()),
                    m(1, good[1].raw()),
                ]),
            ),
        ]);
        assert!(EditAck::decode(duplicate.raw()).is_err());
        for kind in 0..8 {
            let mut bad = good.clone();
            let third = &good[2];
            let wire = third.crdt().unwrap().unwrap();
            bad[2] = match kind {
                0 => Section::from_raw(third.raw().remove(12).unwrap()).unwrap(),
                1 => change(third, m(12, &body("wrong", false))),
                2 => change(
                    third,
                    m(
                        12,
                        &body(&p.second_child_draft().unwrap().rtml().unwrap(), true),
                    ),
                ),
                3 => change(third, m(38, &wire.replace_fields(2, &[n(2, 1)]).unwrap())),
                4 => change(
                    third,
                    m(
                        38,
                        &wire
                            .replace_fields(
                                5,
                                &[F::bytes(5, p.child_wire.bytes(5).unwrap().unwrap()).unwrap()],
                            )
                            .unwrap(),
                    ),
                ),
                5 => change(third, t(8, "zzz")),
                6 => change(third, n(2, 0)),
                _ => change(third, n(19, 0)),
            };
            assert!(p.validate_ack(&ack(&bad)).is_err(), "{index}/ack{kind}");
            assert!(
                !accepted
                    .confirms_readback(&readback(&s, &bad))
                    .unwrap_or(false),
                "{index}/read{kind}"
            );
        }
        // Existing STATUS members may omit body in ACK only; independent full
        // canonical readback still requires all exact bodies and histories.
        let mut metadata = good.clone();
        metadata[0] = Section::from_raw(metadata[0].raw().remove(12).unwrap()).unwrap();
        if p.reuses_child() {
            metadata[1] = Section::from_raw(metadata[1].raw().remove(12).unwrap()).unwrap();
        }
        let accepted = p.validate_ack(&ack(&metadata)).unwrap();
        assert!(accepted.confirms_readback(&readback(&s, &good)).unwrap());
        assert!(
            !accepted
                .confirms_readback(&readback(&s, &metadata))
                .unwrap_or(false)
        );
    }
}
