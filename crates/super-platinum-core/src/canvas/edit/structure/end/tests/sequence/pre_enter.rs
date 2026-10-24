use super::*;
use crate::canvas::edit::structure::recovery::Checkpoint;

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("pre_enter/original.json")).unwrap()
}

#[test]
fn pre_enter_original_48_vectors_bind_live_boundary_and_preserve_v15_history() {
    let e = evidence();
    assert_eq!(
        e["bindingsSha256"],
        "f89976efd04e3c35a21507f659b98a3dabfb8ff535fa8427d2ff0befc462e6ca"
    );
    assert_eq!(
        e["contractSha256"],
        "931011d16ffbe82d12b4d91aaed1d351bc117026521f2c0d030e03c3412abbac"
    );
    let vectors = e["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 48);
    for pair in vectors.chunks_exact(2) {
        assert_eq!(pair[0]["wireSha256"], pair[1]["wireSha256"]);
        assert_eq!(pair[0]["histories"], pair[1]["histories"]);
        assert_eq!(pair[0]["sessions"], pair[1]["sessions"]);
        assert_eq!(pair[0]["parentDirtyFields"], serde_json::json!([12, 15]));
        assert_eq!(pair[1]["parentDirtyFields"], serde_json::json!([15, 12]));
    }
    for v in vectors {
        if v["schedule"] == "parent-preenter" {
            assert_eq!(v["parentPreservedAtEnter"], true);
        }
        let source = super::source(v);
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let intent = EndIntent::capture_sequence(&source, &id, 2, 100).unwrap();
        let original = intent.seed().clone();
        let enter = draft(v["enterDraft"].as_str().unwrap());
        let parent = v["parentReports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| draft(s.as_str().unwrap()))
            .collect::<Vec<_>>();
        let appends = v["appendReports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                r.as_array()
                    .unwrap()
                    .iter()
                    .map(|s| draft(s.as_str().unwrap()))
                    .collect()
            })
            .collect::<Vec<Vec<_>>>();
        let original_offset = split::plain(&original).unwrap().encode_utf16().count();
        let legacy =
            EndWitness::sequence_with_parent_updates(&original, original_offset, &parent, &appends)
                .unwrap();
        let w = if v["schedule"] == "parent-preenter" {
            EndWitness::sequence_with_pre_enter_updates(
                &original,
                &enter,
                v["offset"].as_u64().unwrap() as usize,
                &parent,
                &[],
                &appends,
            )
            .unwrap()
        } else {
            legacy.clone()
        };
        let p = intent.prepare(&source, &w, 100).unwrap();
        // Same captured plan/session/time: first-Enter timing does not change the
        // source-bound ordered-history packet. Native chronology stays separate.
        let post = PreparedEndEdit::build(Arc::clone(&p.intent), legacy, 100).unwrap();
        assert_eq!(p.data, post.data);
        assert!(p.parent_draft() == parent.last().unwrap());
        for (index, row) in p.data.messages(1).unwrap().iter().enumerate() {
            let wire = row.message(38).unwrap().unwrap();
            let mut tags = row.fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            tags.sort();
            assert_eq!(
                tags,
                serde_json::from_value::<Vec<u32>>(v["tags"][index].clone()).unwrap()
            );
            assert_eq!(
                wire.uint(2).unwrap(),
                v["sessions"][index]["baseSequence"].as_u64()
            );
            assert_eq!(
                wire.uint(3).unwrap(),
                v["sessions"][index]["lastSavedSequence"].as_u64()
            );
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            let changes = doc.get_changes(&[]);
            let history = v["histories"][index].as_array().unwrap();
            assert_eq!(changes.len(), history.len());
            for (i, (change, expected)) in changes.iter().zip(history).enumerate() {
                assert_eq!(change.seq(), expected["seq"].as_u64().unwrap());
                assert_eq!(
                    change.deps(),
                    if i == 0 {
                        vec![]
                    } else {
                        vec![changes[i - 1].hash()]
                    }
                );
                let at = doc.fork_at(&[change.hash()]).unwrap();
                let (_, text) = at.get(automerge::ROOT, "text").unwrap().unwrap();
                assert_eq!(at.text(&text).unwrap(), expected["text"].as_str().unwrap());
            }
            if let Some(seed) = source
                .section(row.string(1).unwrap().unwrap())
                .and_then(|s| s.crdt().unwrap())
            {
                for change in Automerge::load(seed.bytes(5).unwrap().unwrap())
                    .unwrap()
                    .get_changes(&[])
                {
                    assert!(doc.get_change_by_hash(&change.hash()).is_some());
                }
            }
        }
        let checkpoint = p.checkpoint().unwrap();
        let json = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(json["version"], 15);
        assert_eq!(json["draft"], original.rtml().unwrap());
        assert_eq!(
            json["action"]["EndSequenceParent"]["parent_updates"],
            v["parentReports"]
        );
        assert!(serde_json::from_value::<Checkpoint>(json).unwrap() == checkpoint);
        let canonical = pair(&p, &source);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        assert!(
            accepted
                .confirms_readback(&readback(&source, &canonical))
                .unwrap()
        );
        let mut no_content = canonical.clone();
        no_content[0] = Section::from_raw(no_content[0].raw().remove(12).unwrap()).unwrap();
        assert!(p.validate_ack(&ack(&no_content)).is_err());
        assert!(p.validate_ack(&ack(&canonical[..1])).is_err());
        assert!(!accepted.confirms_readback(&source).unwrap_or(false));
    }
}

#[test]
fn pre_enter_source_live_offset_prefix_and_later_reports_remain_distinct() {
    let source = fixture("Keep this", false);
    let original = draft("Keep this");
    let enter = draft("Keep this revised");
    let pre = vec![
        draft("Keep this draftx"),
        draft("Keep this draft"),
        enter.clone(),
    ];
    let later = vec![draft("later"), original.clone()];
    let appends = vec![vec![draft("child")]];
    let w =
        EndWitness::sequence_with_pre_enter_updates(&original, &enter, 17, &pre, &later, &appends)
            .unwrap();
    assert!(w.before == original && w.offset == 17);
    assert!(w.parent_updates() == pre.iter().chain(&later).cloned().collect::<Vec<_>>());
    let p = EndIntent::capture_sequence(&source, "item", 1, 100)
        .unwrap()
        .prepare(&source, &w, 100)
        .unwrap();
    assert!(p.parent_draft() == &original);
    assert!(p.data.messages(1).unwrap()[0].has(12));
    // The private boundary is checked again at preparation, not merely accepted
    // once at construction. Later final text cannot stand in for first Enter.
    let mut altered = w.clone();
    altered.parent_updates[pre.len() - 1] = original.clone();
    assert!(
        EndIntent::capture_sequence(&source, "item", 1, 100)
            .unwrap()
            .prepare(&source, &altered, 100)
            .is_err()
    );
    let mut truncated = w.clone();
    truncated.parent_updates.truncate(pre.len() - 1);
    assert!(
        EndIntent::capture_sequence(&source, "item", 1, 100)
            .unwrap()
            .prepare(&source, &truncated, 100)
            .is_err()
    );
    for (live, offset, prefix, suffix) in [
        (enter.clone(), 9, pre.clone(), later.clone()),
        (draft("wrong"), 5, pre.clone(), vec![]),
        (enter.clone(), 17, vec![], vec![]),
        (enter.clone(), 17, pre.clone(), vec![enter.clone()]),
        (draft(""), 1, vec![draft("")], vec![]),
    ] {
        assert!(
            EndWitness::sequence_with_pre_enter_updates(
                &original, &live, offset, &prefix, &suffix, &appends
            )
            .is_err()
        );
    }
    let changed_source = replace(&source, "item", n(2, 2000));
    assert!(
        EndIntent::capture_sequence(&source, "item", 1, 100)
            .unwrap()
            .prepare(&changed_source, &w, 100)
            .is_err()
    );
    let wrong = EndWitness::sequence_with_pre_enter_updates(
        &draft("another seed"),
        &enter,
        17,
        &pre,
        &later,
        &appends,
    )
    .unwrap();
    assert!(
        EndIntent::capture_sequence(&source, "item", 1, 100)
            .unwrap()
            .prepare(&source, &wrong, 100)
            .is_err()
    );
    // The old API still requires original end position, even with parent reports.
    assert!(EndWitness::sequence_with_parent_updates(&original, 17, &pre, &appends).is_err());
    let unicode = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600}");
    let live = draft("Caf\u{e9} &amp; \u{65e5}\u{672c}\u{1f600} revised");
    assert!(
        EndWitness::sequence_with_pre_enter_updates(
            &unicode,
            &live,
            19,
            &[live.clone()],
            &[],
            &appends
        )
        .is_ok()
    );
    assert!(
        EndWitness::sequence_with_pre_enter_updates(
            &unicode,
            &live,
            18,
            &[live.clone()],
            &[],
            &appends
        )
        .is_err()
    );
}

#[test]
fn pre_enter_erased_nonempty_source_and_exact_revert_support_one_append_with_full_history() {
    let source = fixture("Keep this", false);
    let original = draft("Keep this");
    for pre in [
        vec![draft("")],
        vec![draft("Keep this draftx"), original.clone()],
    ] {
        let live = pre.last().unwrap();
        let offset = live.model().unwrap().text.encode_utf16().count();
        let w = EndWitness::sequence_with_pre_enter_updates(
            &original,
            live,
            offset,
            &pre,
            &[],
            &[vec![draft("child")]],
        )
        .unwrap();
        assert!(!w.is_empty_enter());
        let p = EndIntent::capture_sequence(&source, "item", 1, 100)
            .unwrap()
            .prepare(&source, &w, 100)
            .unwrap();
        assert!(p.parent_draft() == live);
        assert_eq!(p.parent_updates().len(), pre.len());
        let doc = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        assert_eq!(doc.get_changes(&[]).len(), 1 + pre.len());
        let c = p.checkpoint().unwrap();
        let value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], 15);
        assert_eq!(value["draft"], "Keep this");
        assert!(serde_json::from_value::<Checkpoint>(value).unwrap() == c);
        let canonical = pair(&p, &source);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        assert!(
            accepted
                .confirms_readback(&readback(&source, &canonical))
                .unwrap()
        );
        let mut wrong_history = canonical.clone();
        wrong_history[0] = change(&wrong_history[0], m(12, &body("wrong", true)));
        assert!(
            !accepted
                .confirms_readback(&readback(&source, &wrong_history))
                .unwrap_or(false)
        );
    }
    assert!(
        EndWitness::sequence_with_pre_enter_updates(
            &draft(""),
            &original,
            9,
            &[original.clone()],
            &[],
            &[vec![]]
        )
        .is_err()
    );
    let reports = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(
        EndWitness::sequence_with_pre_enter_updates(
            &original,
            reports.last().unwrap(),
            1,
            &reports,
            &[],
            &[vec![]]
        )
        .is_ok()
    );
    assert!(
        EndWitness::sequence_with_pre_enter_updates(
            &original,
            reports.last().unwrap(),
            1,
            &reports,
            &[],
            &[vec![draft("extra")]]
        )
        .is_err()
    );
}
