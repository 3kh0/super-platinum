use super::*;

#[test]
fn typed_start_ack_requires_complete_pair_exact_body_language_identity_and_history() {
    for v in vectors()
        .into_iter()
        .filter(|v| v["sourceMode"] == "retained")
    {
        let (s, p) = attempt(&v);
        let rows = canonical(&p, &s);
        for partial in [vec![], vec![rows[0].clone()], vec![rows[1].clone()]] {
            assert!(p.validate_ack(&ack(&partial)).is_err());
        }
        for index in [0, 1] {
            for f in [
                t(1, "other-id"),
                t(7, "other-document"),
                n(20, 2),
                n(19, 0),
                t(33, "other-0"),
                t(8, "zzz"),
                n(1002, 1),
            ] {
                let mut bad = rows.clone();
                bad[index] = change(&bad[index], f);
                assert!(p.validate_ack(&ack(&bad)).is_err(), "member {index}");
            }
            for tag in [12, 38] {
                let mut bad = rows.clone();
                bad[index] = Section::from_raw(bad[index].raw().remove(tag).unwrap()).unwrap();
                assert!(p.validate_ack(&ack(&bad)).is_err());
            }
            let mut bad = rows.clone();
            let body = bad[index].raw().message(12).unwrap().unwrap();
            let text = body
                .message(1)
                .unwrap()
                .unwrap()
                .replace_fields(2, &[t(2, "changed-language")])
                .unwrap();
            bad[index] = change(&bad[index], m(12, &msg(&[m(1, &text)])));
            assert!(p.validate_ack(&ack(&bad)).is_err());
            let wire = rows[index].crdt().unwrap().unwrap();
            for f in [
                n(2, 999),
                t(8, &"0".repeat(64)),
                m(10, &msg(&[])),
                t(5, "malformed"),
                n(4, 0),
            ] {
                let mut bad = rows.clone();
                bad[index] = change(
                    &bad[index],
                    m(
                        38,
                        &wire
                            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
                            .unwrap(),
                    ),
                );
                assert!(p.validate_ack(&ack(&bad)).is_err());
            }
        }
        let duplicate = msg(&[
            n(1, 1),
            m(2, &msg(&[m(1, rows[0].raw()), m(1, rows[0].raw())])),
        ]);
        assert!(EditAck::decode(duplicate.raw()).is_err());
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        assert!(!accepted.confirms_readback(&s).unwrap());
        let current = readback(&s, &rows);
        for index in [0, 1] {
            let mut bad = rows.clone();
            bad[index] = change(&bad[index], n(2, 9000));
            assert!(
                !accepted
                    .confirms_readback(&readback(&current, &bad))
                    .unwrap()
            );
        }
        let other = change(s.section("other").unwrap(), n(1002, 1));
        assert!(
            !accepted
                .confirms_readback(&readback(&current, &[other]))
                .unwrap()
        );
        assert!(accepted.confirms_readback(&current).unwrap());
    }
}

#[test]
fn start_reports_preserve_revisit_but_refuse_noop_stale_source_access_and_limits() {
    let before = draft("Original");
    assert!(StartTypingWitness::new(&before, &[]).is_err());
    assert!(StartTypingWitness::new(&before, &[before.clone()]).is_err());
    assert!(StartTypingWitness::new(&before, &[draft("x"), draft("x")]).is_err());
    assert!(StartTypingWitness::new(&before, &[draft("x"), before.clone()]).is_ok());
    for invalid in ["<b>Bold</b>", "a<br>b", "a\u{feff}b", "a\u{2028}b"] {
        assert!(StartTypingWitness::new(&before, &[draft(invalid)]).is_err());
    }
    assert!(
        StartTypingWitness::new(
            &before,
            &(0..4097)
                .map(|i| draft(if i % 2 == 0 { "a" } else { "b" }))
                .collect::<Vec<_>>()
        )
        .is_err()
    );
    let v = vectors().remove(6);
    let (s, p) = attempt(&v);
    for f in [n(2, 2000), t(8, "aad"), n(35, 1), n(1002, 1)] {
        let drift = readback(&s, &[change(s.section("item").unwrap(), f)]);
        assert!(p.validate_snapshot(&drift, 100).is_err());
    }
    assert!(p.validate_snapshot(&s, 1_000_000).is_err());
    let mut no_title = s.clone();
    no_title.sections.retain(|r| r.id() != "title");
    assert!(p.validate_snapshot(&no_title, 100).is_err());
    let updates = v["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| draft(x.as_str().unwrap()))
        .collect::<Vec<_>>();
    let wrong = StartTypingWitness::new(&draft("Other source"), &updates).unwrap();
    assert!(
        StartTypingIntent::capture(&s, "item", 100)
            .unwrap()
            .prepare(&s, &wrong, 100)
            .is_err()
    );
    let readonly = readback(&s, &[change(s.section("item").unwrap(), n(35, 1))]);
    assert!(StartTypingIntent::capture(&readonly, "item", 100).is_err());
}
