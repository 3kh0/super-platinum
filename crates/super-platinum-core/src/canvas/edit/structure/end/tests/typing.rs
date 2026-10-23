use super::reuse::reused_fixture;
use super::*;
mod recovery;

#[test]
fn live_report_patch_order_is_not_a_reconstructed_history_diff() {
    use automerge::transaction::Transactable;
    use automerge::{PatchAction, PatchLog};
    for prefix in ["New ", "New 日本😀 "] {
        let seed = crdt::insert(
            "probe",
            &draft(&format!("{prefix}draft")),
            "0123456789ab",
            100,
        )
        .unwrap();
        let mut provisional = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
        let before = provisional.get_heads();
        let (_, object) = provisional.get(automerge::ROOT, "text").unwrap().unwrap();
        let mut tx = provisional.transaction_log_patches(PatchLog::active());
        tx.update_text(&object, format!("{prefix}paragraph\u{feff}"))
            .unwrap();
        let (_, mut log) = tx.commit();
        let live = provisional.make_patches(&mut log);
        let reloaded = Automerge::load(&provisional.save()).unwrap();
        let rebuilt = reloaded.diff(&before, &reloaded.get_heads());
        let summarize = |patches: &[automerge::Patch]| {
            patches
                .iter()
                .map(|p| match &p.action {
                    PatchAction::SpliceText { index, value, .. } => {
                        (*index, 0, value.make_string())
                    }
                    PatchAction::DeleteSeq { index, length } => (*index, *length, String::new()),
                    _ => panic!("plain report unexpectedly contains another patch"),
                })
                .collect::<Vec<_>>()
        };
        let offset = prefix.encode_utf16().count();
        // Pinned original cached/live and save/load comparison. The first
        // insertion/deletion pair stays in its original order: no generic sort.
        assert_eq!(
            summarize(&live),
            vec![
                (offset, 0, "parag".into()),
                (offset + 5, 1, String::new()),
                (offset + 7, 2, String::new()),
                (offset + 7, 0, "ph".into()),
            ]
        );
        assert_eq!(
            summarize(&rebuilt),
            vec![
                (offset, 0, "parag".into()),
                (offset + 5, 1, String::new()),
                (offset + 7, 0, "ph".into()),
                (offset + 9, 2, String::new()),
            ]
        );
    }
}

fn typed(reused: bool, updates: &[&str]) -> (CanvasCollabSnapshot, PreparedEndEdit) {
    let s = if reused {
        reused_fixture(true)
    } else {
        fixture("Café &amp; 日本😀", true)
    };
    let intent = EndIntent::capture(&s, "item", 100).unwrap();
    let w = witness(&intent.seed().rtml().unwrap())
        .with_child_updates(&updates.iter().map(|s| draft(s)).collect::<Vec<_>>())
        .unwrap();
    let p = intent.prepare(&s, &w, 100).unwrap();
    (s, p)
}

#[test]
fn typed_new_and_reused_child_preserve_each_original_replacement_patch() {
    for reused in [false, true] {
        for prefix in ["New ", "New 日本😀 "] {
            let states = [
                "New".to_owned(),
                format!("{prefix}draftx"),
                format!("{prefix}draft"),
                format!("{prefix}paragraph"),
            ];
            let (s, p) = typed(
                reused,
                &states.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            assert_eq!(p.reuses_child(), reused);
            assert!(p.child_draft() == &draft(states.last().unwrap()));
            assert_eq!(p.child_updates().len(), 4);
            let rows = p.data.messages(1).unwrap();
            assert_eq!(rows.len(), 2);
            let parent = s.section("item").unwrap();
            assert!(
                rows[0].message(38).unwrap().unwrap()
                    == validation::project_parent_session(&parent.crdt().unwrap().unwrap())
                        .unwrap()
            );
            assert!(!rows[0].has(12));
            assert_eq!(rows[1].uint(9).unwrap(), Some(0));
            if reused {
                assert_eq!(
                    rows[1].fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
                    vec![1, 2, 13, 19, 20, 33, 38, 9, 12]
                );
            }
            let doc = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
            let start = if reused {
                Automerge::load(
                    s.section(p.child_section())
                        .unwrap()
                        .crdt()
                        .unwrap()
                        .unwrap()
                        .bytes(5)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap()
                .get_heads()
            } else {
                vec![p.child_wire.string(8).unwrap().unwrap().parse().unwrap()]
            };
            let changes = doc.get_changes(&start);
            assert_eq!(
                changes.len(),
                7,
                "four reports include four final replacement patches"
            );
            let expected = [
                "New".to_owned(),
                format!("{prefix}draftx"),
                format!("{prefix}draft"),
                format!("{prefix}paragdraft"),
                format!("{prefix}paragraft"),
                format!("{prefix}paragra"),
                format!("{prefix}paragraph"),
            ];
            let mut previous = start.clone();
            for (step, (c, expected)) in changes.iter().zip(expected).enumerate() {
                assert_eq!(
                    c.actor_id().to_hex_string(),
                    format!("{}00000000", p.session_id())
                );
                assert_eq!(c.deps(), previous);
                let at = doc.fork_at(&[c.hash()]).unwrap();
                let (_, object) = at.get(automerge::ROOT, "text").unwrap().unwrap();
                assert!(
                    text::crdt::check_document_state(&at, &draft(&expected), false).is_ok(),
                    "source patch state differs: reused={reused}, prefix={prefix:?}, step={step}, actual={:?}, expected={:?}, patches={:?}, marks={:?}",
                    at.text(&object).unwrap(),
                    format!("{expected}\u{feff}"),
                    doc.diff(&previous, &[c.hash()]),
                    at.marks(&object).unwrap()
                );
                previous = vec![c.hash()];
            }
            let processed = p
                .child_wire
                .fields()
                .iter()
                .filter(|f| f.tag() == 6)
                .map(|f| {
                    RawMessage::decode(p.child_wire.encoded_field(f).unwrap())
                        .unwrap()
                        .string(6)
                        .unwrap()
                        .unwrap()
                        .to_owned()
                })
                .collect::<Vec<_>>();
            if reused {
                let old = s
                    .section(p.child_section())
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap();
                let prior = old
                    .fields()
                    .iter()
                    .filter(|f| f.tag() == 6)
                    .map(|f| {
                        RawMessage::decode(old.encoded_field(f).unwrap())
                            .unwrap()
                            .string(6)
                            .unwrap()
                            .unwrap()
                            .to_owned()
                    })
                    .collect::<Vec<_>>();
                let mut expected = prior.clone();
                expected.push(doc.get_heads()[0].to_string());
                expected.extend(prior);
                assert_eq!(processed, expected);
            } else {
                assert_eq!(processed, vec![doc.get_heads()[0].to_string()]);
            }
            let canonical = pair(&p, &s);
            let accepted = p.validate_ack(&ack(&canonical)).unwrap();
            let fresh = readback(&s, &canonical);
            assert!(accepted.confirms_readback(&fresh).unwrap());
            save_text(&fresh, p.child_section(), "Later &amp; 日本😀");
        }
    }
}

#[test]
fn typed_corrections_and_net_empty_retain_history_and_content_projection() {
    for reused in [false, true] {
        for states in [
            &["a", "ab", "a", "ac"][..],
            &["x", ""][..],
            &["e\u{301}", "e\u{301} 👩🏽\u{200d}💻 &amp;"][..],
        ] {
            let (s, p) = typed(reused, states);
            let updates = p.data.messages(1).unwrap();
            assert!(updates[1].has(9) && updates[1].has(12));
            assert!(p.child_wire.has(12));
            assert!(p.child_draft() == &draft(states.last().unwrap()));
            let c = p.checkpoint().unwrap();
            let json = serde_json::to_value(&c).unwrap();
            assert_eq!(json["version"], 6);
            assert_eq!(
                json["action"]["EndTyping"]["child_updates"],
                serde_json::json!(states)
            );
            let rows = pair(&p, &s);
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
        }
    }
}

#[test]
fn typed_ack_and_independent_readback_refuse_partial_stale_body_and_history() {
    for reused in [false, true] {
        let (s, p) = typed(reused, &["New", "New draftx", "New draft", "New paragraph"]);
        let rows = pair(&p, &s);
        assert!(p.validate_ack(&ack(&rows[..1])).is_err());
        assert!(p.validate_ack(&ack(&rows[1..])).is_err());
        for field in [
            m(12, &body("Wrong", true)),
            m(12, &body("New paragraph", !reused)),
            n(20, 2),
            n(19, 0),
        ] {
            let mut bad = rows.clone();
            bad[1] = change(&bad[1], field);
            assert!(p.validate_ack(&ack(&bad)).is_err());
        }
        let mut missing = rows.clone();
        missing[1] = Section::from_raw(missing[1].raw().remove(12).unwrap()).unwrap();
        assert!(p.validate_ack(&ack(&missing)).is_err());
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let mut changed = readback(&s, &rows);
        let i = changed
            .sections
            .iter()
            .position(|r| r.id() == p.child_section())
            .unwrap();
        changed.sections[i] = change(&changed.sections[i], m(12, &body("stale", true)));
        assert!(!accepted.confirms_readback(&changed).unwrap());
        let mut stale = s.clone();
        let i = stale
            .sections
            .iter()
            .position(|r| r.id() == if reused { p.child_section() } else { "item" })
            .unwrap();
        stale.sections[i] = change(&stale.sections[i], n(2, 9999));
        assert!(p.validate_snapshot(&stale, 100).is_err());
    }
}

#[test]
fn typed_witness_bounds_plain_states_and_adjacent_noop_without_normalization() {
    for values in [
        vec![""],
        vec!["a", "a"],
        vec!["<b>a</b>"],
        vec!["a<br>b"],
        vec!["a\u{feff}"],
        vec!["a\u{2028}"],
    ] {
        assert!(
            witness("Parent")
                .with_child_updates(&values.into_iter().map(draft).collect::<Vec<_>>())
                .is_err()
        );
    }
    assert!(
        witness("Parent")
            .with_child_updates(&[draft("é"), draft("e\u{301}"), draft("")])
            .is_ok()
    );
    // The RTML parser normalizes C0/C1 before this logical boundary. Construct
    // raw renderer drafts here so the test actually supplies each excluded scalar.
    for scalar in [
        '\u{1}', '\u{85}', '\u{9f}', '\u{2028}', '\u{2029}', '\u{feff}',
    ] {
        let raw = TextDraft {
            runs: vec![text::TextRun {
                text: format!("a{scalar}"),
                style: text::TextStyle::default(),
            }],
        };
        assert!(
            witness("Parent").with_child_updates(&[raw]).is_err(),
            "scalar={scalar:?}"
        );
    }
    let many = (0..4097)
        .map(|i| draft(if i % 2 == 0 { "a" } else { "b" }))
        .collect::<Vec<_>>();
    assert!(witness("Parent").with_child_updates(&many[..4096]).is_ok());
    assert!(witness("Parent").with_child_updates(&many).is_err());
    let a = draft(&"a".repeat(65535));
    let b = draft(&"b".repeat(65535));
    let big = (0..129)
        .map(|i| if i % 2 == 0 { a.clone() } else { b.clone() })
        .collect::<Vec<_>>();
    assert!(witness("Parent").with_child_updates(&big).is_err());
}
