use super::*;
mod recovery;
mod refusals;
mod typing;

fn original() -> serde_json::Value {
    serde_json::from_str(include_str!("repeat/original.json")).unwrap()
}
fn bound_id(id: &str, mode: &str) -> String {
    if mode == "retained-zero" {
        match id {
            "paragraph" => "temp:C:rep0000000000000000000000003".into(),
            "following" => "temp:C:rep0000000000000000000000004".into(),
            _ => id.into(),
        }
    } else {
        id.into()
    }
}
fn source(index: usize) -> CanvasCollabSnapshot {
    let evidence = original();
    let binding = &evidence["bindings"][index];
    let mode = binding["mode"].as_str().unwrap();
    let mut s = fixture("Keep this", false);
    let template = s.section("item").unwrap().clone();
    s.sections.retain(|r| r.id() == "title");
    for (i, entity) in binding["sourceEntities"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let id = bound_id(entity["id"].as_str().unwrap(), mode);
        let mut row = Section::from_raw(template.raw().remove(38).unwrap()).unwrap();
        for f in [
            t(1, &id),
            t(8, ["aac", "aae", "aag"][i]),
            t(21, ["aac", "aae", "aag"][i]),
            n(2, 1000),
            m(12, &body(entity["rtml"].as_str().unwrap(), true)),
        ] {
            row = change(&row, f);
        }
        if let Some(encoded) = entity["canonicalField38"].as_str() {
            let bytes = crate::slack::api::decode_canvas_base64(encoded).unwrap();
            let wire = RawMessage::decode(&bytes)
                .unwrap()
                .replace_fields(1, &[t(1, &id)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            row = change(&row, m(38, &wire));
        }
        s.sections.push(row);
    }
    s
}
fn repeated(s: &CanvasCollabSnapshot, id: &str) -> PreparedEndEdit {
    let intent = EndIntent::capture_repeat(s, id, 100).unwrap();
    assert!(intent.is_repeat() && !intent.is_empty_enter());
    let before = intent.seed().clone();
    let offset = split::plain(&before).unwrap().encode_utf16().count();
    let w = EndWitness::repeat(
        &before,
        offset,
        &before,
        &TextDraft::default(),
        &TextDraft::default(),
    )
    .unwrap();
    intent.prepare(s, &w, 100).unwrap()
}
fn replace(s: &CanvasCollabSnapshot, id: &str, field: F) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let row = s.sections.iter_mut().find(|r| r.id() == id).unwrap();
    *row = change(row, field);
    s
}

#[test]
fn repeat_original_nineteen_vectors_preserve_three_independent_sessions_and_later_edits() {
    let evidence = original();
    let vectors = evidence["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 19);
    for (index, v) in vectors.iter().enumerate() {
        let mode = v["mode"].as_str().unwrap();
        let id = bound_id("paragraph", mode);
        let label = format!("{index}/{mode}/{}/{}", v["corpus"], v["layout"]);
        let s = source(index);
        let before = s.sections.clone();
        let p = repeated(&s, &id);
        let second = p.second_child_section().unwrap();
        assert!(p.is_repeat() && !p.is_empty_enter() && !p.clears_child());
        assert!(
            p.child_updates().is_empty()
                && p.second_child_draft().unwrap() == &TextDraft::default()
        );
        assert_ne!(second, p.child_section());
        assert_ne!(second, id);
        assert_eq!(p.reuses_child(), v["layout"] == "empty-final-following");
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), 3);
        for (i, expected_id) in [id.as_str(), p.child_section(), second]
            .into_iter()
            .enumerate()
        {
            assert_eq!(rows[i].string(1).unwrap(), Some(expected_id));
            let tags = rows[i].fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            let expected: Vec<u32> = serde_json::from_value(v["tags"][i].clone()).unwrap();
            assert_eq!(tags, expected, "{label}/row{i}");
            let wire = rows[i].message(38).unwrap().unwrap();
            assert!(!wire.has(4) && !wire.has(10));
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            assert_eq!(
                doc.get_changes(&[]).len(),
                v["histories"][i]["changes"].as_u64().unwrap() as usize,
                "{label}/history{i}"
            );
            let expected_draft = if i == 0 {
                draft(v["sourceText"].as_str().unwrap())
            } else {
                TextDraft::default()
            };
            text::crdt::check_document_state(&doc, &expected_draft, false).unwrap();
            if let Some(source) = s.section(expected_id) {
                assert!(!rows[i].has(12) && !rows[i].has(9) && !wire.has(12));
                if let Some(old) = source.crdt().unwrap() {
                    assert_eq!(
                        wire,
                        validation::project_parent_session(&old).unwrap(),
                        "{label}/retained{i}"
                    );
                } else {
                    let changes = doc.get_changes(&[]);
                    assert_eq!(changes.len(), 1);
                    assert_eq!(
                        changes[0].actor_id().to_hex_string(),
                        format!("{}00000000", p.session_id())
                    );
                    assert_eq!(changes[0].seq(), 1);
                    assert!(changes[0].deps().is_empty());
                    assert_eq!(wire.uint(2).unwrap(), Some(1000));
                    assert_eq!(wire.uint(3).unwrap(), Some(1000));
                    assert_eq!(wire.string(6).unwrap(), wire.string(8).unwrap());
                }
            } else {
                let changes = doc.get_changes(&[]);
                assert_eq!(changes.len(), 1);
                assert_eq!(
                    changes[0].actor_id().to_hex_string(),
                    format!("{}00000000", p.session_id())
                );
                assert_eq!(changes[0].seq(), 1);
                assert!(changes[0].deps().is_empty());
                assert_eq!(wire.uint(2).unwrap(), Some(0));
                assert_eq!(wire.uint(3).unwrap(), Some(0));
                assert_eq!(wire.string(6).unwrap(), wire.string(8).unwrap());
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
            }
        }
        if !p.reuses_child() {
            // Equal bootstrap hashes across independent IDs are legitimate.
            assert_eq!(
                p.child_wire.bytes(5).unwrap(),
                p.second_wire.as_ref().unwrap().bytes(5).unwrap()
            );
        }
        let c = p.checkpoint().unwrap();
        assert!(c.is_repeat() && c.is_end() && !c.is_empty_enter());
        assert_eq!(serde_json::to_value(&c).unwrap()["version"], 12);
        let canonical = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        assert_eq!(accepted.second_child_section().unwrap().id(), second);
        let fresh = readback(&s, &canonical);
        assert!(
            accepted.confirms_readback(&fresh).unwrap(),
            "{label}/readback"
        );
        let mut ordered = fresh.sections.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|s| s.raw().string(8).unwrap().unwrap());
        let original_index = ordered.iter().position(|s| s.id() == id).unwrap();
        assert_eq!(ordered[original_index + 1].id(), p.child_section());
        assert_eq!(ordered[original_index + 2].id(), second);
        assert_eq!(
            fresh.sections.len(),
            s.sections.len() + if p.reuses_child() { 1 } else { 2 }
        );
        assert_eq!(s.sections, before);
        let mut edited = fresh;
        for (id, value) in [
            (second, "Third \u{65e5}\u{672c}\u{1f600} &amp; edited"),
            (p.child_section(), "First selected edited"),
            (id.as_str(), "Original edited"),
        ] {
            let next = save_text(&edited, id, value);
            if id == second {
                let actual = text::canonical_draft(next.section(id).unwrap()).unwrap();
                assert_eq!(
                    split::plain(&actual).unwrap(),
                    "Third \u{65e5}\u{672c}\u{1f600} & edited"
                );
            }
            for untouched in edited.sections.iter().filter(|r| r.id() != id) {
                assert_eq!(next.section(untouched.id()), Some(untouched));
            }
            edited = next;
        }
    }
}
