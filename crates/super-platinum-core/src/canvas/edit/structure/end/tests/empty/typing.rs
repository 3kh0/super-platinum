//! Original 42 report vectors, with host-only identity/lease bindings.
use super::*;
use crate::canvas::edit::structure::end::typing as proof;
mod recovery;
mod refusals;

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("typing/original.json")).unwrap()
}

fn fixture_from_reference(mode: &str, layout: &str) -> CanvasCollabSnapshot {
    let mut s = source(mode, layout);
    let captured = evidence();
    let seed_layout = if layout == "empty-final-following" {
        layout
    } else {
        "no-following"
    };
    let seed = captured["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["mode"] == mode && v["layout"] == seed_layout)
        .unwrap();
    for (id, key) in [
        (source_id(mode), "originalCanonicalSession"),
        (selected_id(mode), "selectedCanonicalSession"),
    ] {
        if key == "selectedCanonicalSession" && layout != "empty-final-following" {
            continue;
        }
        let row = s.sections.iter_mut().find(|r| r.id() == id).unwrap();
        *row = Section::from_raw(row.raw().remove(38).unwrap()).unwrap();
        if let Some(encoded) = seed[key].as_str() {
            let bytes = crate::slack::api::decode_canvas_base64(encoded).unwrap();
            let wire = RawMessage::decode(&bytes)
                .unwrap()
                .replace_fields(1, &[t(1, id)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            *row = change(row, m(38, &wire));
        }
    }
    s
}

fn typed(s: &CanvasCollabSnapshot, id: &str, updates: &[TextDraft]) -> PreparedEndEdit {
    let witness = EndWitness::empty().with_child_updates(updates).unwrap();
    assert!(witness.is_empty_enter() && witness.is_empty_typing());
    EndIntent::capture_empty(s, id, 100)
        .unwrap()
        .prepare(s, &witness, 100)
        .unwrap()
}

fn processed(w: &RawMessage) -> Vec<String> {
    w.fields()
        .iter()
        .filter(|f| f.tag() == 6)
        .map(|f| {
            RawMessage::decode(w.encoded_field(f).unwrap())
                .unwrap()
                .string(6)
                .unwrap()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn initial_seed(p: &PreparedEndEdit, s: &CanvasCollabSnapshot) -> RawMessage {
    if let Some(old) = s.section(p.child_section()) {
        return crate::canvas::edit::structure::end::empty::typed_seed(
            old,
            &p.child_wire,
            p.session_id(),
        )
        .unwrap();
    }
    let doc = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
    let head = p.child_wire.string(8).unwrap().unwrap();
    let base = doc.fork_at(&[head.parse().unwrap()]).unwrap();
    msg(&[
        t(1, p.child_section()),
        n(2, 0),
        n(3, 0),
        F::bytes(5, &base.save()).unwrap(),
        t(6, head),
        t(8, head),
        t(12, head),
    ])
}

#[test]
fn empty_typing_original_42_vectors_preserve_live_patch_history_and_pair() {
    let captured = evidence();
    assert_eq!(captured["vectors"].as_array().unwrap().len(), 42);
    for v in captured["vectors"].as_array().unwrap() {
        let mode = v["mode"].as_str().unwrap();
        let layout = v["layout"].as_str().unwrap();
        let label = format!("{mode}/{layout}/{}/{}", v["corpus"], v["cadence"]);
        let s = fixture_from_reference(mode, layout);
        let original_sections = s.sections.clone();
        let reports = v["reports"].as_array().unwrap();
        let updates = reports
            .iter()
            .map(|r| draft(r["rtml"].as_str().unwrap()))
            .collect::<Vec<_>>();
        let p = typed(&s, source_id(mode), &updates);
        assert!(p.is_empty_enter() && p.is_empty_typing() && !p.clears_child());
        assert_eq!(
            p.reuses_child(),
            layout == "empty-final-following",
            "{label}"
        );
        assert!(p.child_draft() == updates.last().unwrap());
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].string(1).unwrap(), Some(source_id(mode)));
        assert_eq!(rows[1].string(1).unwrap(), Some(p.child_section()));
        for i in 0..2 {
            // Singular protobuf field order is not a semantic difference. Keep
            // multiplicity and repeated session-head order checked separately.
            let mut actual = rows[i].fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            actual.sort_unstable();
            let mut expected: Vec<u32> = serde_json::from_value(v["tags"][i].clone()).unwrap();
            expected.sort_unstable();
            assert_eq!(actual, expected, "{label}/row{i}");
        }
        assert!(!rows[0].has(12) && !rows[0].has(9));
        assert!(!p.parent_wire.has(12) && !p.parent_wire.has(10) && !p.parent_wire.has(4));
        let parent_doc = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let seed_layout = if p.reuses_child() {
            "empty-final-following"
        } else {
            "no-following"
        };
        let source_seed = captured["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|seed| seed["mode"] == mode && seed["layout"] == seed_layout)
            .unwrap();
        // The vector summary's sourceHistoryCount names the selected child's
        // initial history. The unchanged original has its own retained prefix.
        assert_eq!(
            parent_doc.get_changes(&[]).len(),
            source_seed["sourceOriginal"]["history"]
                .as_array()
                .unwrap()
                .len(),
            "{label}/original source history"
        );
        text::crdt::check_document_state(&parent_doc, &TextDraft::default(), false).unwrap();
        if let Some(old) = s.section(source_id(mode)).unwrap().crdt().unwrap() {
            assert_eq!(
                p.parent_wire,
                validation::project_parent_session(&old).unwrap()
            );
        }
        let seed = initial_seed(&p, &s);
        let baseline = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
        assert_eq!(
            baseline.get_changes(&[]).len(),
            v["sourceHistoryCount"].as_u64().unwrap() as usize,
            "{label}/selected source history"
        );
        let doc = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        let changes = doc.get_changes(&baseline.get_heads());
        assert_eq!(
            changes.len(),
            v["addedChanges"].as_u64().unwrap() as usize,
            "{label}"
        );
        assert_eq!(
            doc.get_changes(&[]).len(),
            v["finalHistoryCount"].as_u64().unwrap() as usize,
            "{label}"
        );
        for c in baseline.get_changes(&[]) {
            assert_eq!(
                doc.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
                c.raw_bytes(),
                "{label}/prefix"
            );
        }
        let expected_texts = reports
            .iter()
            .flat_map(|r| r["texts"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(expected_texts.len(), changes.len());
        let expected_patches = reports
            .iter()
            .flat_map(|r| r["patches"].as_array().unwrap())
            .flat_map(|group| group.as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(expected_patches.len(), changes.len());
        let mut previous = baseline.get_heads();
        let actor = format!("{}00000000", p.session_id());
        let mut sequence = baseline
            .get_changes(&[])
            .iter()
            .filter(|c| c.actor_id().to_hex_string() == actor)
            .map(|c| c.seq())
            .max()
            .unwrap_or(0);
        for (index, (c, expected)) in changes.iter().zip(expected_texts).enumerate() {
            sequence += 1;
            assert_eq!(c.actor_id().to_hex_string(), actor, "{label}/{index}/actor");
            assert_eq!(c.seq(), sequence);
            assert_eq!(c.deps(), previous);
            let actual_patches = doc.diff(&previous, &[c.hash()]);
            assert_eq!(actual_patches.len(), 1, "{label}/{index}/patch count");
            let expected_patch = expected_patches[index];
            let expected_index = expected_patch["path"][1].as_u64().unwrap() as usize;
            match &actual_patches[0].action {
                automerge::PatchAction::SpliceText {
                    index,
                    value,
                    marks,
                } => {
                    assert_eq!(expected_patch["action"], "splice");
                    assert_eq!(*index, expected_index);
                    assert_eq!(
                        value.make_string(),
                        expected_patch["value"].as_str().unwrap()
                    );
                    assert!(marks.is_none());
                }
                automerge::PatchAction::DeleteSeq { index, length } => {
                    assert_eq!(expected_patch["action"], "del");
                    assert_eq!(*index, expected_index);
                    assert_eq!(
                        *length,
                        expected_patch["length"].as_u64().unwrap_or(1) as usize
                    );
                }
                _ => panic!("{label}/{index}/unexpected history patch"),
            }
            let at = doc.fork_at(&[c.hash()]).unwrap();
            let (_, text) = at.get(automerge::ROOT, "text").unwrap().unwrap();
            assert_eq!(
                at.text(&text).unwrap(),
                expected.as_str().unwrap(),
                "{label}/{index}/text"
            );
            assert!(at.marks(&text).unwrap().is_empty());
            previous = vec![c.hash()];
        }
        assert_eq!(previous, doc.get_heads());
        let mut heads = if p.reuses_child() {
            processed(&seed)
        } else {
            Vec::new()
        };
        heads.push(doc.get_heads()[0].to_string());
        if p.reuses_child() {
            heads.extend(processed(&seed));
        }
        assert_eq!(processed(&p.child_wire), heads, "{label}/processed");
        assert_eq!(p.child_wire.uint(2).unwrap(), seed.uint(2).unwrap());
        assert_eq!(p.child_wire.uint(3).unwrap(), seed.uint(3).unwrap());
        assert_eq!(p.child_wire.string(8).unwrap(), seed.string(8).unwrap());
        assert_eq!(p.child_wire.message(11).unwrap(), seed.message(11).unwrap());
        proof::validate_reports(
            &seed,
            &p.child_wire,
            p.reuses_child(),
            &updates,
            p.session_id(),
        )
        .unwrap();
        let checkpoint = p.checkpoint().unwrap();
        assert!(checkpoint.is_empty_typing() && checkpoint.is_empty_enter());
        assert_eq!(serde_json::to_value(&checkpoint).unwrap()["version"], 11);
        let canonical = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        let fresh = readback(&s, &canonical);
        assert!(
            accepted.confirms_readback(&fresh).unwrap(),
            "{label}/readback"
        );
        assert_eq!(s.sections, original_sections);
        let edited = save_text(&fresh, p.child_section(), "Selected 日本😀 &amp; again");
        assert_eq!(
            edited.section(source_id(mode)),
            fresh.section(source_id(mode))
        );
        let final_state = save_text(&edited, source_id(mode), "Original again");
        assert_eq!(
            edited.section(p.child_section()),
            final_state.section(p.child_section())
        );
    }
}

#[test]
fn empty_typing_independent_source_modes_do_not_require_matching_pair_histories() {
    // Product composition proof, not a claim that the reference executed the
    // nine mixed-mode pair cross product. Each session is separately admitted.
    for original_mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for selected_mode in ["bootstrap", "retained-positive", "retained-zero"] {
            let mut s = fixture_from_reference(original_mode, "empty-final-following");
            let child = existing(
                s.section(selected_id(original_mode)).unwrap(),
                selected_id(selected_mode),
                "aae",
                selected_mode,
            );
            s.sections.retain(|r| r.id() != selected_id(original_mode));
            s.sections.push(child);
            let p = typed(&s, source_id(original_mode), &[draft("x"), draft("")]);
            assert!(p.reuses_child());
            assert_eq!(p.child_section(), selected_id(selected_mode));
            let c = p.checkpoint().unwrap();
            assert!(c.is_empty_typing());
            let canonical = pair(&p, &s);
            assert!(
                p.validate_ack(&ack(&canonical))
                    .unwrap()
                    .confirms_readback(&readback(&s, &canonical))
                    .unwrap()
            );
        }
    }
}
