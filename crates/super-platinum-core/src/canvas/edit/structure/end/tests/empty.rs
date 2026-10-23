use super::*;
mod recovery;
mod typing;

const ZERO_SOURCE: &str = "temp:C:Dte0000000000000000000000001";
const ZERO_SELECTED: &str = "temp:C:Dte0000000000000000000000000";

fn original() -> serde_json::Value {
    serde_json::from_str(include_str!("empty/original.json")).unwrap()
}
fn source_id(mode: &str) -> &str {
    if mode == "retained-zero" {
        ZERO_SOURCE
    } else {
        "item"
    }
}
fn selected_id(mode: &str) -> &str {
    if mode == "retained-zero" {
        ZERO_SELECTED
    } else {
        "other"
    }
}
fn existing(template: &Section, id: &str, pos: &str, mode: &str) -> Section {
    let mut row = Section::from_raw(template.raw().remove(38).unwrap()).unwrap();
    for f in [
        t(1, id),
        t(8, pos),
        t(21, pos),
        n(2, 1000),
        m(12, &body("", true)),
    ] {
        row = change(&row, f);
    }
    let captured = original();
    let seed = captured["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["mode"] == mode)
        .unwrap();
    if let Some(encoded) = seed["canonicalSession"].as_str() {
        let raw = crate::slack::api::decode_canvas_base64(encoded).unwrap();
        let wire = RawMessage::decode(&raw)
            .unwrap()
            .replace_fields(1, &[t(1, id)])
            .unwrap()
            .replace_fields(4, &[n(4, 1_000_000)])
            .unwrap();
        row = change(&row, m(38, &wire));
    }
    row
}
fn source(mode: &str, layout: &str) -> CanvasCollabSnapshot {
    let mut s = fixture("", false);
    let template = s.section("item").unwrap().clone();
    s.sections.retain(|r| r.id() == "title");
    s.sections
        .push(existing(&template, source_id(mode), "aac", mode));
    if layout != "no-following" {
        let mut neighbor = existing(&template, selected_id(mode), "aae", "bootstrap");
        match layout {
            "empty-final-following" => {
                neighbor = existing(&template, selected_id(mode), "aae", mode)
            }
            "empty-nonfinal-following" => {
                let last = change(
                    &existing(&template, "last-text", "aag", "bootstrap"),
                    m(12, &body("Last paragraph", true)),
                );
                s.sections.push(last);
            }
            "text-following-ascii" => {
                neighbor = change(&neighbor, m(12, &body("Another paragraph", true)))
            }
            "text-following-unicode" => {
                neighbor = change(
                    &neighbor,
                    m(12, &body("Café &amp; 日本😀Second paragraph", true)),
                )
            }
            _ => panic!("unknown layout"),
        }
        s.sections.push(neighbor);
    }
    s
}
fn attempt(s: &CanvasCollabSnapshot, id: &str) -> PreparedEndEdit {
    let intent = EndIntent::capture_empty(s, id, 100).unwrap();
    assert!(intent.is_empty_enter());
    assert!(intent.seed() == &TextDraft::default());
    intent.prepare(s, &EndWitness::empty(), 100).unwrap()
}

#[test]
fn original_fifteen_empty_vectors_preserve_noop_sessions_and_exact_pair_projection() {
    let captured = original();
    for v in captured["vectors"].as_array().unwrap() {
        let mode = v["mode"].as_str().unwrap();
        let layout = v["layout"].as_str().unwrap();
        let s = source(mode, layout);
        let before = s.sections.clone();
        let p = attempt(&s, source_id(mode));
        assert!(p.is_empty_enter() && !p.clears_child() && p.child_updates().is_empty());
        assert_eq!(
            p.reuses_child(),
            v["reused"].as_bool().unwrap(),
            "{mode}/{layout}"
        );
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), 2);
        for (i, wire) in [&p.parent_wire, &p.child_wire].into_iter().enumerate() {
            let tags = rows[i].fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            let expected: Vec<u32> = serde_json::from_value(v["tags"][i].clone()).unwrap();
            assert_eq!(tags, expected, "{mode}/{layout}/row{i}");
            assert!(!wire.has(4) && !wire.has(10));
            let id = if i == 0 {
                source_id(mode)
            } else {
                p.child_section()
            };
            assert_eq!(rows[i].string(1).unwrap(), Some(id));
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            let expected_count = v[if i == 0 {
                "sourceHistoryCount"
            } else {
                "selectedHistoryCount"
            }]
            .as_u64()
            .unwrap() as usize;
            assert_eq!(
                doc.get_changes(&[]).len(),
                expected_count,
                "{mode}/{layout}/row{i}"
            );
            text::crdt::check_document_state(&doc, &TextDraft::default(), false).unwrap();
            if let Some(old) = s.section(id) {
                assert!(!wire.has(12) && !rows[i].has(12) && !rows[i].has(9));
                if let Some(seed) = old.crdt().unwrap() {
                    assert_eq!(*wire, validation::project_parent_session(&seed).unwrap());
                } else {
                    let changes = doc.get_changes(&[]);
                    assert_eq!(changes.len(), 1);
                    assert_eq!(
                        changes[0].actor_id().to_hex_string(),
                        format!("{}00000000", p.session_id())
                    );
                    assert_eq!(changes[0].seq(), 1);
                    assert!(changes[0].deps().is_empty());
                    assert_eq!(wire.uint(2).unwrap(), old.sequence().unwrap());
                }
            } else {
                assert_eq!(wire.uint(2).unwrap(), Some(0));
                assert_eq!(wire.uint(3).unwrap(), Some(0));
                assert_eq!(wire.string(8).unwrap(), wire.string(12).unwrap());
                assert_eq!(wire.string(6).unwrap(), wire.string(12).unwrap());
                assert_eq!(
                    rows[i]
                        .message(12)
                        .unwrap()
                        .unwrap()
                        .message(1)
                        .unwrap()
                        .unwrap()
                        .string(1)
                        .unwrap(),
                    Some("")
                );
                let pos = rows[i].string(8).unwrap().unwrap();
                assert_eq!(pos, "aad:temp");
            }
        }
        if p.reuses_child() {
            assert_eq!(p.child_section(), selected_id(mode));
        }
        let c = p.checkpoint().unwrap();
        assert!(c.is_empty_enter() && c.is_end());
        assert_eq!(serde_json::to_value(&c).unwrap()["version"], 10);
        let canonical = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        let fresh = readback(&s, &canonical);
        assert!(
            accepted.confirms_readback(&fresh).unwrap(),
            "{mode}/{layout}"
        );
        assert_eq!(
            fresh.sections.len(),
            s.sections.len() + usize::from(!p.reuses_child())
        );
        assert_eq!(s.sections, before);
        // Both original and selected remain ordinary independently editable rows.
        let edited = save_text(&fresh, p.child_section(), "Selected 日本😀 &amp; text");
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

fn replace(s: &CanvasCollabSnapshot, id: &str, field: F) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let row = s.sections.iter_mut().find(|r| r.id() == id).unwrap();
    *row = change(row, field);
    s
}

#[test]
fn empty_admission_is_explicit_and_preserves_source_neighbor_and_lease_guards() {
    assert!(EndWitness::empty().is_empty_enter());
    assert!(EndWitness::empty().with_child_updates(&[]).is_err());
    assert!(
        EndWitness::empty()
            .with_child_updates(&[draft("x")])
            .is_ok()
    );
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        let s = source(mode, "empty-final-following");
        let id = source_id(mode);
        assert!(EndIntent::capture(&s, id, 100).is_err());
        assert!(
            EndIntent::capture_empty(&s, id, 100)
                .unwrap()
                .prepare(&s, &witness("x"), 100)
                .is_err()
        );
        for target in [id, selected_id(mode)] {
            for field in [
                m(12, &body(" ", true)),
                m(12, &body("<br>", true)),
                m(16, &msg(&[n(13, 1)])),
                n(35, 1),
                n(10, 1),
                m(13, &msg(&[t(1, "missing")])),
                t(8, "aac:temp"),
            ] {
                assert!(
                    EndIntent::capture_empty(&replace(&s, target, field), id, 100).is_err(),
                    "{mode}/{target}"
                );
            }
            if let Some(w) = s.section(target).unwrap().crdt().unwrap() {
                for (label, bad) in [
                    ("missing lease", w.remove(4).unwrap()),
                    ("expired lease", w.replace_fields(4, &[n(4, 100)]).unwrap()),
                    ("missing processed heads", w.remove(6).unwrap()),
                    (
                        "unsupported split lineage",
                        w.replace_fields(10, &[m(10, &msg(&[n(1, 0)]))]).unwrap(),
                    ),
                    (
                        "future last-saved revision",
                        w.replace_fields(3, &[n(3, 1001)]).unwrap(),
                    ),
                ] {
                    assert!(
                        EndIntent::capture_empty(&replace(&s, target, m(38, &bad)), id, 100)
                            .is_err(),
                        "{mode}/{target}/{label}"
                    );
                }
                // Only acknowledged zero-base histories require last_saved to
                // equal the canonical revision. Positive-base validators allow
                // an earlier saved revision and preserve it without alteration.
                if mode == "retained-zero" {
                    let bad = w.replace_fields(3, &[n(3, 0)]).unwrap();
                    assert!(
                        EndIntent::capture_empty(&replace(&s, target, m(38, &bad)), id, 100)
                            .is_err(),
                        "{mode}/{target}/zero-base unacknowledged revision"
                    );
                }
            }
        }
        let p = attempt(&s, id);
        for target in [id, selected_id(mode), "title"] {
            assert!(
                p.validate_snapshot(&replace(&s, target, n(2, 2000)), 100)
                    .is_err()
            );
        }
        if mode != "bootstrap" {
            assert!(p.validate_snapshot(&s, 1_000_000).is_err());
        }
    }
}

#[test]
fn empty_ack_and_full_readback_require_both_exact_members_and_unchanged_history() {
    for mode in ["bootstrap", "retained-positive", "retained-zero"] {
        for layout in ["no-following", "empty-final-following"] {
            let s = source(mode, layout);
            let p = attempt(&s, source_id(mode));
            let rows = pair(&p, &s);
            assert!(p.validate_ack(&ack(&rows[..1])).is_err());
            assert!(p.validate_ack(&ack(&rows[1..])).is_err());
            let duplicate = msg(&[
                n(1, 1),
                m(2, &msg(&[m(1, rows[0].raw()), m(1, rows[0].raw())])),
            ]);
            assert!(EditAck::decode(duplicate.raw()).is_err());
            for i in 0..2 {
                for field in [
                    m(12, &body("changed", true)),
                    t(33, "wrong"),
                    n(20, 2),
                    n(19, 0),
                    n(35, 1),
                    t(7, "wrong"),
                ] {
                    let mut bad = rows.clone();
                    bad[i] = change(&bad[i], field);
                    assert!(
                        p.validate_ack(&ack(&bad)).is_err(),
                        "{mode}/{layout}/row{i}"
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
                bad[i] = change(&bad[i], m(12, &body("", !text.has(2))));
                assert!(p.validate_ack(&ack(&bad)).is_err());
                let w = rows[i].crdt().unwrap().unwrap();
                for bad_wire in [
                    w.replace_fields(2, &[n(2, 7)]).unwrap(),
                    w.replace_fields(8, &[t(8, &"0".repeat(64))]).unwrap(),
                    w.remove(6).unwrap(),
                    w.replace_fields(10, &[m(10, &msg(&[]))]).unwrap(),
                ] {
                    let mut bad = rows.clone();
                    bad[i] = change(&bad[i], m(38, &bad_wire));
                    assert!(p.validate_ack(&ack(&bad)).is_err());
                }
            }
            let accepted = p.validate_ack(&ack(&rows)).unwrap();
            let fresh = readback(&s, &rows);
            assert!(accepted.confirms_readback(&fresh).unwrap());
            for i in 0..2 {
                let mut bad = fresh.clone();
                bad.sections.retain(|r| r.id() != rows[i].id());
                assert!(!accepted.confirms_readback(&bad).unwrap_or(false));
                let mut metadata = rows.clone();
                metadata[i] = Section::from_raw(metadata[i].raw().remove(12).unwrap()).unwrap();
                if i == 0 || p.reuses_child() {
                    assert!(p.validate_ack(&ack(&metadata)).is_ok());
                } else {
                    assert!(p.validate_ack(&ack(&metadata)).is_err());
                }
                assert!(
                    !accepted
                        .confirms_readback(&readback(&s, &metadata))
                        .unwrap_or(false)
                );
            }
        }
    }
}
