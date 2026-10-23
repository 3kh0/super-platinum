use super::*;
mod recovery;
mod refusals;

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("typing/original.json")).unwrap()
}
fn reports(v: &serde_json::Value) -> Vec<TextDraft> {
    v["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| draft(x.as_str().unwrap()))
        .collect()
}
fn typed(s: &CanvasCollabSnapshot, id: &str, updates: &[TextDraft]) -> PreparedEndEdit {
    let intent = EndIntent::capture_repeat(s, id, 100).unwrap();
    let before = intent.seed().clone();
    let w = EndWitness::repeat(
        &before,
        split::plain(&before).unwrap().encode_utf16().count(),
        &before,
        &TextDraft::default(),
        &TextDraft::default(),
    )
    .unwrap()
    .with_second_child_updates(updates)
    .unwrap();
    intent.prepare(s, &w, 100).unwrap()
}

#[test]
fn repeat_typing_thirty_six_original_vectors_keep_prefixes_and_every_real_patch() {
    let e = evidence();
    assert_eq!(
        e["contractSha256"],
        "3895c0bc02cf6b735017da1b455740fd12c703e03bc29395adfdf05e9d9792af"
    );
    let vectors = e["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 36);
    for (i, v) in vectors.iter().enumerate() {
        let b = &e["bindings"][i];
        let prior = b["priorVector"].as_u64().unwrap() as usize;
        // New reference really uses the previously accepted source sessions.
        assert_eq!(
            b["sourceEntities"],
            original()["bindings"][prior]["sourceEntities"]
        );
        assert_eq!(b["afterEnter2"][0], b["afterTyping"][0]);
        assert_eq!(b["afterEnter2"][1], b["afterTyping"][1]);
        let s = source(prior);
        let untouched = s.sections.clone();
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let updates = reports(v);
        let p = typed(&s, &id, &updates);
        let label = format!("{i}/{}/{}/{}", v["mode"], v["corpus"], v["cadence"]);
        if v["corpus"] == "unicode" {
            let visible = split::plain(&p.intent.seed).unwrap();
            assert_eq!(visible, "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}", "{label}");
            assert_eq!(visible.encode_utf16().count(), 11);
            assert!(
                updates
                    .iter()
                    .any(|d| d.rtml().unwrap().contains("\u{65e5}\u{672c}\u{1f600}"))
            );
        }
        assert!(p.is_repeat() && p.is_repeat_typing());
        assert!(p.child_draft() == &TextDraft::default() && p.child_updates().is_empty());
        assert!(p.second_child_draft() == updates.last());
        assert!(p.second_child_updates() == updates.as_slice());
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), 3);
        for row in 0..3 {
            let tags = rows[row]
                .fields()
                .iter()
                .map(|f| f.tag())
                .collect::<Vec<_>>();
            let expected: Vec<u32> = serde_json::from_value(v["tags"][row].clone()).unwrap();
            assert_eq!(tags, expected, "{label}/tags{row}");
        }
        assert!(!rows[0].has(12) && !rows[0].has(9));
        assert_eq!(rows[2].string(1).unwrap(), p.second_child_section());
        assert_ne!(p.child_section(), p.second_child_section().unwrap());
        assert_eq!(p.reuses_child(), v["layout"] == "empty-final-following");
        for row in 0..2 {
            let wire = rows[row].message(38).unwrap().unwrap();
            let rid = rows[row].string(1).unwrap().unwrap();
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            if let Some(old) = s.section(rid).and_then(|s| s.crdt().unwrap()) {
                assert_eq!(
                    wire,
                    validation::project_parent_session(&old).unwrap(),
                    "{label}/unchanged{row}"
                );
            } else {
                assert_eq!(doc.get_changes(&[]).len(), 1, "{label}/bootstrap{row}");
            }
            if s.section(rid).is_some() {
                assert!(!wire.has(12) && !rows[row].has(12));
            }
        }
        let wire = p.second_wire.as_ref().unwrap();
        let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        let changes = doc.get_changes(&[]);
        let history = v["thirdHistory"].as_array().unwrap();
        assert_eq!(changes.len(), history.len(), "{label}");
        assert_eq!(
            changes.len(),
            match v["cadence"].as_str().unwrap() {
                "correction" => 8,
                "erase" => 3,
                "exact-revert" => 5,
                x => panic!("unknown {x}"),
            }
        );
        for (n, (change, expected)) in changes.iter().zip(history).enumerate() {
            assert_eq!(
                change.actor_id().to_hex_string(),
                format!("{}00000000", p.session_id())
            );
            assert_eq!(change.seq(), n as u64 + 1);
            assert_eq!(
                change.deps(),
                if n == 0 {
                    vec![]
                } else {
                    vec![changes[n - 1].hash()]
                }
            );
            let at = doc.fork_at(&[change.hash()]).unwrap();
            let (_, obj) = at.get(automerge::ROOT, "text").unwrap().unwrap();
            assert_eq!(
                at.text(&obj).unwrap(),
                expected["text"].as_str().unwrap(),
                "{label}/patch{n}"
            );
        }
        assert_eq!(wire.uint(2).unwrap(), Some(0));
        assert_eq!(wire.uint(3).unwrap(), Some(0));
        assert_eq!(
            wire.string(8).unwrap(),
            Some(changes[0].hash().to_string().as_str())
        );
        assert_ne!(wire.string(8).unwrap(), wire.string(12).unwrap());
        assert_eq!(wire.fields().iter().filter(|f| f.tag() == 6).count(), 1);
        assert_eq!(
            wire.string(6).unwrap(),
            Some(doc.get_heads()[0].to_string().as_str())
        );
        assert_eq!(wire.string(6).unwrap(), wire.string(12).unwrap());
        let child = Section::from_raw(rows[2].clone()).unwrap();
        assert!(text::canonical_draft(&child).unwrap() == *updates.last().unwrap());
        assert!(!wire.has(10));
        let checkpoint = p.checkpoint().unwrap();
        assert!(checkpoint.is_repeat_typing());
        assert_eq!(serde_json::to_value(&checkpoint).unwrap()["version"], 13);
        let canonical = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        assert!(accepted.is_repeat_typing());
        let fresh = readback(&s, &canonical);
        assert!(
            accepted.confirms_readback(&fresh).unwrap(),
            "{label}/readback"
        );
        let mut ordered = fresh.sections.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|s| s.raw().string(8).unwrap().unwrap());
        let at = ordered.iter().position(|r| r.id() == id).unwrap();
        assert_eq!(ordered[at + 1].id(), p.child_section());
        assert_eq!(ordered[at + 2].id(), p.second_child_section().unwrap());
        assert_eq!(s.sections, untouched);
        let mut edited = fresh;
        for (rid, value) in [
            (
                p.second_child_section().unwrap(),
                "Third \u{65e5}\u{672c}\u{1f600} &amp; edited",
            ),
            (p.child_section(), "First selected edited"),
            (id.as_str(), "Original edited"),
        ] {
            let next = save_text(&edited, rid, value);
            for old in edited.sections.iter().filter(|r| r.id() != rid) {
                assert_eq!(next.section(old.id()), Some(old));
            }
            edited = next;
        }
    }
}
