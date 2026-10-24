use super::*;
mod recovery;

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("parent/original.json")).unwrap()
}
fn attempt(v: &serde_json::Value) -> (CanvasCollabSnapshot, PreparedEndEdit) {
    let source = super::source(v);
    let id = bound_id("paragraph", v["mode"].as_str().unwrap());
    let intent = EndIntent::capture_sequence(&source, &id, 3, 100).unwrap();
    let before = intent.seed().clone();
    let parent = v["parentReports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| draft(r.as_str().unwrap()))
        .collect::<Vec<_>>();
    let members = v["appendReports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reports| {
            reports
                .as_array()
                .unwrap()
                .iter()
                .map(|r| draft(r.as_str().unwrap()))
                .collect()
        })
        .collect::<Vec<Vec<_>>>();
    let witness = EndWitness::sequence_with_parent_updates(
        &before,
        split::plain(&before).unwrap().encode_utf16().count(),
        &parent,
        &members,
    )
    .unwrap();
    let prepared = intent.prepare(&source, &witness, 100).unwrap();
    (source, prepared)
}

#[test]
fn parent_sequence_original_48_vectors_preserve_reports_history_session_and_whole_ack_readback() {
    let evidence = evidence();
    assert_eq!(
        evidence["contractSha256"],
        "b2d7ce1bff29caa198f46992e9270883718e6d0fe68407608efce2adb793bcf4"
    );
    assert_eq!(
        evidence["bindingsSha256"],
        "5761e69cda84e4c1baf1c9c2d92666107a8fb85d93c172791ed3f0a6cb9a9b43"
    );
    let vectors = evidence["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 48);
    for pair in vectors.chunks_exact(2) {
        assert_eq!(pair[0]["wireSha256"], pair[1]["wireSha256"]);
        assert_eq!(pair[0]["parentReports"], pair[1]["parentReports"]);
    }
    for v in vectors {
        let (source, p) = attempt(v);
        let rows = p.data.messages(1).unwrap();
        for (i, row) in rows.iter().enumerate() {
            let mut tags = row.fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            tags.sort();
            let expected: Vec<u32> = serde_json::from_value(v["tags"][i].clone()).unwrap();
            assert_eq!(tags, expected);
        }
        assert!(p.parent_draft() == p.parent_updates().last().unwrap());
        assert_eq!(
            p.parent_updates()
                .iter()
                .map(|d| d.rtml().unwrap())
                .collect::<Vec<_>>(),
            serde_json::from_value::<Vec<String>>(v["parentReports"].clone()).unwrap()
        );
        let original = source.section(p.target_section()).unwrap();
        let seed = original.crdt().unwrap().unwrap();
        let baseline = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
        let doc = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let prefix = baseline.get_changes(&[]);
        for change in &prefix {
            assert!(doc.get_change_by_hash(&change.hash()).is_some());
        }
        let changes = doc.get_changes(&[]);
        let expected = v["parentHistory"].as_array().unwrap();
        assert_eq!(changes.len(), expected.len());
        for (i, (change, expected)) in changes.iter().zip(expected).enumerate() {
            assert_eq!(change.seq(), expected["seq"].as_u64().unwrap());
            assert_eq!(
                change.actor_id().to_hex_string(),
                if i < prefix.len() {
                    expected["actor"].as_str().unwrap().to_owned()
                } else {
                    format!("{}00000000", p.session_id())
                }
            );
            assert_eq!(
                change.deps(),
                if i == 0 {
                    vec![]
                } else {
                    vec![changes[i - 1].hash()]
                }
            );
            let at = doc.fork_at(&[change.hash()]).unwrap();
            let (_, object) = at.get(automerge::ROOT, "text").unwrap().unwrap();
            assert_eq!(
                at.text(&object).unwrap(),
                expected["text"].as_str().unwrap()
            );
        }
        assert_eq!(
            p.parent_wire.uint(2).unwrap(),
            v["session"]["baseSequence"].as_u64()
        );
        assert_eq!(
            p.parent_wire.uint(3).unwrap(),
            v["session"]["lastSavedSequence"].as_u64()
        );
        assert!(!p.parent_wire.has(4) && !p.parent_wire.has(9) && !p.parent_wire.has(10));
        let processed = |wire: &RawMessage| {
            wire.fields()
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
                .collect::<Vec<_>>()
        };
        let prior = processed(&seed);
        let head = doc.get_heads()[0].to_string();
        assert_eq!(
            processed(&p.parent_wire),
            prior
                .iter()
                .cloned()
                .chain([head.clone()])
                .chain(prior.iter().cloned())
                .collect::<Vec<_>>()
        );
        assert_eq!(p.parent_wire.string(12).unwrap(), Some(head.as_str()));
        if v["revisit"] == "exact-revert" {
            assert!(p.parent_draft() == &p.intent.seed);
            assert!(changes.len() > prefix.len());
            assert!(rows[0].has(12));
        }
        let checkpoint = p.checkpoint().unwrap();
        assert_eq!(serde_json::to_value(checkpoint).unwrap()["version"], 15);
        let canonical = pair(&p, &source);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        assert!(accepted.parent_draft() == p.parent_draft());
        assert!(
            accepted
                .confirms_readback(&readback(&source, &canonical))
                .unwrap()
        );
        assert!(accepted.confirms_readback(&source).is_ok_and(|v| !v));
    }
}

#[test]
fn parent_sequence_bootstrap_bounds_plain_reports_and_stale_source_remain_closed() {
    let source = fixture("Keep this", false);
    let initial = draft("Keep this");
    let appends = vec![vec![draft("one")], vec![draft("two")]];
    for reports in [vec![draft("")], vec![draft("Keep this x"), initial.clone()]] {
        let intent = EndIntent::capture_sequence(&source, "item", 2, 100).unwrap();
        let w = EndWitness::sequence_with_parent_updates(&initial, 9, &reports, &appends).unwrap();
        let p = intent.prepare(&source, &w, 100).unwrap();
        assert!(p.checkpoint().is_ok());
        let canonical = pair(&p, &source);
        assert!(
            p.validate_ack(&ack(&canonical))
                .unwrap()
                .confirms_readback(&readback(&source, &canonical))
                .unwrap()
        );
        let stale = replace(&source, "item", n(2, 2000));
        assert!(
            EndIntent::capture_sequence(&source, "item", 2, 100)
                .unwrap()
                .prepare(&stale, &w, 100)
                .is_err()
        );
    }
    for reports in [
        vec![initial.clone()],
        vec![draft("x"), draft("x")],
        vec![draft("<b>x</b>")],
        vec![draft("a\u{feff}b")],
    ] {
        assert!(EndWitness::sequence_with_parent_updates(&initial, 9, &reports, &appends).is_err());
    }
    let reports = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(EndWitness::sequence_with_parent_updates(&initial, 9, &reports, &[vec![]]).is_ok());
    assert!(
        EndWitness::sequence_with_parent_updates(&initial, 9, &reports, &[vec![draft("a")]])
            .is_err()
    );
    assert!(
        EndWitness::sequence_with_parent_updates(
            &initial,
            9,
            &reports,
            &[vec![], vec![], vec![], vec![]]
        )
        .is_err()
    );
    let large = (0..900)
        .map(|i| {
            draft(&if i % 2 == 0 {
                "x".repeat(5000)
            } else {
                "y".repeat(5000)
            })
        })
        .collect::<Vec<_>>();
    assert!(
        EndWitness::sequence_with_parent_updates(&initial, 9, &large, &[large.clone()]).is_err()
    );
    let old = EndWitness::sequence(&initial, 9, &appends).unwrap();
    assert!(old.parent_updates().is_empty());
}

#[test]
fn parent_sequence_ack_and_readback_require_explicit_content_and_exact_whole_history() {
    for v in evidence()["vectors"].as_array().unwrap().iter().step_by(4) {
        let (source, p) = attempt(v);
        let canonical = pair(&p, &source);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        for kind in 0..6 {
            let mut bad = canonical.clone();
            bad[0] = match kind {
                0 => Section::from_raw(bad[0].raw().remove(12).unwrap()).unwrap(),
                1 => change(&bad[0], m(12, &body("wrong", true))),
                2 => change(&bad[0], n(2, 1000)),
                3 => change(
                    &bad[0],
                    m(
                        38,
                        &source
                            .section(p.target_section())
                            .unwrap()
                            .crdt()
                            .unwrap()
                            .unwrap(),
                    ),
                ),
                4 => change(&bad[0], t(6, "other-thread")),
                _ => change(&bad[0], t(8, "zzz")),
            };
            assert!(p.validate_ack(&ack(&bad)).is_err(), "tamper {kind}");
            assert!(
                !accepted
                    .confirms_readback(&readback(&source, &bad))
                    .unwrap_or(false)
            );
        }
        assert!(p.validate_ack(&ack(&canonical[1..])).is_err());
        assert!(p.validate_ack(&ack(&canonical[..1])).is_err());
        let mut dropped = readback(&source, &canonical);
        dropped.sections.retain(|s| s.id() != canonical[2].id());
        assert!(!accepted.confirms_readback(&dropped).unwrap_or(false));
    }
}
