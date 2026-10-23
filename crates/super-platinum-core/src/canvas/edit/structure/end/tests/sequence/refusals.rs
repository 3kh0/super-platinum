use super::*;

#[test]
fn sequence_bounds_modes_aggregate_budgets_and_stale_source_are_closed() {
    let d = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}");
    assert_eq!(split::plain(&d).unwrap().encode_utf16().count(), 11);
    for n in [0, 4100] {
        assert!(EndWitness::sequence(&d, 11, &vec![vec![]; n]).is_err());
    }
    assert!(EndWitness::sequence(&d, 10, &[vec![]]).is_err());
    assert!(EndWitness::sequence(&TextDraft::default(), 0, &[vec![]]).is_err());
    for stages in [
        vec![vec![draft("")]],
        vec![vec![draft("a"), draft("a")]],
        vec![vec![draft("<b>x</b>")]],
        vec![vec![draft("a\u{feff}b")]],
        vec![vec![draft("x\u{2028}y")]],
    ] {
        assert!(EndWitness::sequence(&d, 11, &stages).is_err());
    }
    let alternating = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(EndWitness::sequence(&d, 11, &[alternating.clone()]).is_ok());
    assert!(EndWitness::sequence(&d, 11, &[alternating, vec![draft("x")]]).is_err());
    let large = (0..900)
        .map(|i| {
            draft(&if i % 2 == 0 {
                "x".repeat(5000)
            } else {
                "y".repeat(5000)
            })
        })
        .collect::<Vec<_>>();
    assert!(EndWitness::sequence(&d, 11, &[large.clone()]).is_ok());
    assert!(EndWitness::sequence(&d, 11, &[large.clone(), large]).is_err());
    let w = EndWitness::sequence(&d, 11, &[vec![draft("x")], vec![]]).unwrap();
    assert!(w.clone().with_child_updates(&[draft("x")]).is_err());
    assert!(w.clone().with_second_child_updates(&[draft("x")]).is_err());
    let e = evidence();
    let s = source(&e["bindings"][35]);
    let id = bound_id("paragraph", "retained-zero");
    let intent = EndIntent::capture_sequence(&s, &id, 3, 100).unwrap();
    let before = intent.seed().clone();
    let w = EndWitness::sequence(&before, 11, &[vec![], vec![], vec![]]).unwrap();
    assert!(intent.prepare(&s, &w, 100).is_ok());
    assert!(
        EndIntent::capture_sequence(&s, &id, 2, 100)
            .unwrap()
            .prepare(&s, &w, 100)
            .is_err()
    );
    assert!(
        EndIntent::capture_repeat(&s, &id, 100)
            .unwrap()
            .prepare(&s, &w, 100)
            .is_err()
    );
    for row in &s.sections {
        let stale = replace(&s, row.id(), n(2, 2000));
        let intent = EndIntent::capture_sequence(&s, &id, 3, 100).unwrap();
        assert!(
            intent.prepare(&stale, &w, 100).is_err(),
            "stale {}",
            row.id()
        );
    }
    let p = attempt(&s, &id, &[vec![draft("x")], vec![], vec![]]);
    assert!(p.validate_snapshot(&s, 1_000_000).is_err());
    for target in [&id, p.child_section()] {
        let row = s.section(target).unwrap();
        let denied = replace(&s, target, n(35, 1));
        assert!(EndIntent::capture_sequence(&denied, &id, 3, 100).is_err());
        let wire = row.crdt().unwrap().unwrap().remove(4).unwrap();
        assert!(
            EndIntent::capture_sequence(&replace(&s, target, m(38, &wire)), &id, 3, 100).is_err()
        );
    }
}

#[test]
fn sequence_ack_readback_require_every_member_exact_body_history_identity_and_neighbors() {
    let e = evidence();
    for index in [24, 35, 38, 41, 43, 46, 47] {
        let v = &e["vectors"][index];
        let s = source(&e["bindings"][index]);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let p = attempt(&s, &id, &reports(v));
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let fresh = readback(&s, &rows);
        assert!(accepted.confirms_readback(&fresh).unwrap());
        for missing in 0..rows.len() {
            let mut partial = rows.clone();
            partial.remove(missing);
            assert!(
                p.validate_ack(&ack(&partial)).is_err(),
                "{index}/missing{missing}"
            );
            let mut read = fresh.clone();
            read.sections.retain(|r| r.id() != rows[missing].id());
            assert!(!accepted.confirms_readback(&read).unwrap());
        }
        let mut duplicated = rows.clone();
        duplicated.push(rows[1].clone());
        let raw = msg(&[
            n(1, 1),
            m(
                2,
                &msg(&duplicated.iter().map(|r| m(1, r.raw())).collect::<Vec<_>>()),
            ),
        ]);
        assert!(EditAck::decode(raw.raw()).is_err());
        let mut extra = rows.clone();
        extra.push(s.section("title").unwrap().clone());
        assert!(p.validate_ack(&ack(&extra)).is_err());
        for k in 0..rows.len() {
            for f in [
                t(1, "wrong-owned-id"),
                n(2, 0),
                t(6, "wrong-thread"),
                t(7, "wrong-document"),
                n(19, 0),
                n(20, 2),
                t(33, "wrong-trace"),
                m(12, &body("altered", false)),
            ] {
                let mut bad = rows.clone();
                bad[k] = change(&bad[k], f);
                assert!(p.validate_ack(&ack(&bad)).is_err(), "{index}/row{k}");
            }
            let wire = rows[k]
                .crdt()
                .unwrap()
                .unwrap()
                .replace_fields(8, &[t(8, &"0".repeat(64))])
                .unwrap();
            let mut bad = rows.clone();
            bad[k] = change(&bad[k], m(38, &wire));
            assert!(p.validate_ack(&ack(&bad)).is_err());
        }
        for (k, member) in p.appended_members().enumerate() {
            if !member.reused() || !member.updates().is_empty() {
                let mut bad = rows.clone();
                bad[k + 1] = Section::from_raw(bad[k + 1].raw().remove(12).unwrap()).unwrap();
                assert!(p.validate_ack(&ack(&bad)).is_err(), "{index}/body omission");
            }
            let mut bad = rows.clone();
            let content = bad[k + 1].raw().message(12).unwrap().unwrap();
            let rich = content
                .message(1)
                .unwrap()
                .unwrap()
                .replace_fields(2, &[t(2, "fr")])
                .unwrap();
            bad[k + 1] = change(
                &bad[k + 1],
                m(12, &content.replace_fields(1, &[m(1, &rich)]).unwrap()),
            );
            assert!(p.validate_ack(&ack(&bad)).is_err());
            assert!(
                !accepted
                    .confirms_readback(&replace(&fresh, member.id(), m(12, &body("stale", false))))
                    .unwrap()
            );
        }
        for neighbor in fresh
            .sections
            .iter()
            .filter(|r| r.id() != id && !p.appended_members().any(|m| m.id() == r.id()))
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
fn sequence_same_final_text_with_different_history_or_report_boundaries_is_not_equivalent() {
    let e = evidence();
    let s = source(&e["bindings"][24]);
    let mut p = attempt(
        &s,
        "paragraph",
        &[vec![draft("x"), draft("")], vec![draft("Next")], vec![]],
    );
    let rows = p.data.messages(1).unwrap();
    let seed = repeat_typing::seed(&rows[1], p.session_id(), &TextDraft::default()).unwrap();
    let alternative = super::super::super::typing::build(
        &seed,
        false,
        &[draft("detour"), draft("")],
        p.session_id(),
        100,
    )
    .unwrap();
    p.sequence_rows[0].wire = alternative;
    p.data = super::super::super::sequence::project(
        &p.intent.original,
        p.intent.reused(),
        p.session_id(),
        &p.parent_wire,
        &p.sequence_rows,
        p.intent.title.rich_text(),
    )
    .unwrap();
    assert!(p.checkpoint().is_err());
    let mut p = attempt(
        &s,
        "paragraph",
        &[
            vec![
                draft("New"),
                draft("New draftx"),
                draft("New draft"),
                draft("New paragraph"),
            ],
            vec![],
        ],
    );
    p.sequence_rows[0].updates = vec![draft("New paragraph")];
    assert!(p.checkpoint().is_err());
}
