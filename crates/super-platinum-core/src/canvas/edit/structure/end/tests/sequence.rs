use super::*;
mod long;
mod recovery;
mod refusals;
fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("sequence/original.json")).unwrap()
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
fn source(binding: &serde_json::Value) -> CanvasCollabSnapshot {
    let mode = binding["mode"].as_str().unwrap();
    let mut s = fixture("Keep this", false);
    let template = s.section("item").unwrap().clone();
    s.sections.retain(|r| r.id() == "title");
    for (i, e) in binding["sourceEntities"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let id = bound_id(e["id"].as_str().unwrap(), mode);
        let mut row = Section::from_raw(template.raw().remove(38).unwrap()).unwrap();
        for f in [
            t(1, &id),
            t(8, ["aac", "aae", "aag"][i]),
            t(21, ["aac", "aae", "aag"][i]),
            n(2, 1000),
            m(12, &body(e["rtml"].as_str().unwrap(), true)),
        ] {
            row = change(&row, f);
        }
        if let Some(encoded) = e["canonicalField38"].as_str() {
            let raw = crate::slack::api::decode_canvas_base64(encoded).unwrap();
            let wire = RawMessage::decode(&raw)
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
fn reports(v: &serde_json::Value) -> Vec<Vec<TextDraft>> {
    let mut result = vec![Vec::new(); v["appendCount"].as_u64().unwrap() as usize];
    for t in v["transitions"].as_array().unwrap() {
        if t["kind"] == "report" {
            result[t["appendIndex"].as_u64().unwrap() as usize]
                .push(draft(t["rtml"].as_str().unwrap()));
        }
    }
    result
}
fn attempt(s: &CanvasCollabSnapshot, id: &str, reports: &[Vec<TextDraft>]) -> PreparedEndEdit {
    let intent = EndIntent::capture_sequence(s, id, reports.len(), 100).unwrap();
    let before = intent.seed().clone();
    let w = EndWitness::sequence(
        &before,
        split::plain(&before).unwrap().encode_utf16().count(),
        reports,
    )
    .unwrap();
    intent.prepare(s, &w, 100).unwrap()
}
fn replace(s: &CanvasCollabSnapshot, id: &str, f: F) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let row = s.sections.iter_mut().find(|r| r.id() == id).unwrap();
    *row = change(row, f);
    s
}
#[test]
fn sequence_forty_eight_original_vectors_preserve_ordered_histories_all_rows_and_later_edits() {
    let e = evidence();
    assert_eq!(
        e["contractSha256"],
        "9b8d473319751664592ddeef1d0528d043fcf57bcc69599b2d405b4f1f768788"
    );
    let vectors = e["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 48);
    check_vectors(&e);
}
fn check_vectors(e: &serde_json::Value) {
    let vectors = e["vectors"].as_array().unwrap();
    for (i, v) in vectors.iter().enumerate() {
        let binding = &e["bindings"][i];
        let s = source(binding);
        let old = s.sections.clone();
        let id = bound_id("paragraph", v["mode"].as_str().unwrap());
        let stages = reports(v);
        let p = attempt(&s, &id, &stages);
        assert!(p.is_sequence() && !p.is_repeat() && !p.is_repeat_typing() && !p.is_empty_enter());
        assert_eq!(p.appended_members().len(), stages.len());
        assert!(p.second_child_section().is_none());
        if v["corpus"] == "unicode" {
            let visible = split::plain(&p.intent.seed).unwrap();
            assert_eq!(
                visible, "Caf\u{e9} & \u{65e5}\u{672c}\u{1f600}",
                "vector{i}"
            );
            assert_eq!(visible.encode_utf16().count(), 11);
        }
        for transition in binding["transitions"].as_array().unwrap() {
            if transition["kind"] == "enter" && transition["appendIndex"].as_u64().unwrap() > 0 {
                for before in transition["before"].as_array().unwrap() {
                    let after = transition["after"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|x| x["id"] == before["id"])
                        .unwrap();
                    assert_eq!(before, after, "reference prior row changed at vector{i}");
                }
            }
        }
        let rows = p.data.messages(1).unwrap();
        assert_eq!(rows.len(), stages.len() + 1);
        let mut ids = std::collections::BTreeSet::new();
        for (index, row) in rows.iter().enumerate() {
            let rid = row.string(1).unwrap().unwrap();
            assert!(ids.insert(rid));
            let expected = &v["rows"][index];
            let wire = row.message(38).unwrap().unwrap();
            let mut tags = row.fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
            let mut expected_tags: Vec<u32> =
                serde_json::from_value(v["tags"][index].clone()).unwrap();
            tags.sort();
            expected_tags.sort();
            assert_eq!(tags, expected_tags, "{i}/row{index} tag multiplicities");
            assert_eq!(
                row.has(12),
                expected["contentPresent"].as_bool().unwrap(),
                "{i}/body{index}"
            );
            assert_eq!(
                wire.uint(2).unwrap(),
                expected["session"]["base_sequence"].as_u64()
            );
            assert_eq!(
                wire.uint(3).unwrap(),
                expected["session"]["last_saved_sequence"].as_u64()
            );
            assert!(!wire.has(4) && !wire.has(9) && !wire.has(10));
            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
            let changes = doc.get_changes(&[]);
            let history = expected["history"].as_array().unwrap();
            assert_eq!(changes.len(), history.len(), "{i}/row{index}/history");
            let original = s.section(rid).and_then(|r| r.crdt().unwrap());
            let retained = original
                .as_ref()
                .map(|w| Automerge::load(w.bytes(5).unwrap().unwrap()).unwrap());
            if let Some(retained) = &retained {
                for c in retained.get_changes(&[]) {
                    assert!(doc.get_change_by_hash(&c.hash()).is_some(), "{i}/prefix");
                }
            }
            let prefix = retained.as_ref().map_or(0, |d| d.get_changes(&[]).len());
            for (n, (change, expected)) in changes.iter().zip(history).enumerate() {
                let actor = if n < prefix {
                    expected["actor"].as_str().unwrap().to_owned()
                } else {
                    format!("{}00000000", p.session_id())
                };
                assert_eq!(
                    change.actor_id().to_hex_string(),
                    actor,
                    "{i}/row{index}/actor{n}"
                );
                assert_eq!(
                    change.seq(),
                    expected["seq"].as_u64().unwrap(),
                    "{i}/row{index}/seq{n}"
                );
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
                    "{i}/row{index}/patch{n}"
                );
            }
            let processed = wire
                .fields()
                .iter()
                .filter(|f| f.tag() == 6)
                .collect::<Vec<_>>();
            assert_eq!(
                processed.len(),
                expected["session"]["processed_heads"]
                    .as_array()
                    .unwrap()
                    .len()
            );
            assert_eq!(
                wire.has(12),
                expected["session"].get("current_head").is_some()
            );
            if index == 0 {
                if let Some(source) = original {
                    assert_eq!(wire, validation::project_parent_session(&source).unwrap());
                }
            } else {
                let member = p.appended_members().nth(index - 1).unwrap();
                assert_eq!(member.id(), rid);
                assert!(member.draft() == &draft(expected["rtml"].as_str().unwrap()));
                assert!(member.updates() == stages[index - 1].as_slice());
                assert_eq!(member.reused(), index == 1 && p.reuses_child());
            }
        }
        let c = p.checkpoint().unwrap();
        assert!(c.is_sequence() && !c.is_repeat());
        assert_eq!(serde_json::to_value(&c).unwrap()["version"], 14);
        let canonical = pair(&p, &s);
        let a = p.validate_ack(&ack(&canonical)).unwrap();
        assert!(a.is_sequence() && !a.is_repeat());
        assert_eq!(a.appended_sections().len(), stages.len());
        let fresh = readback(&s, &canonical);
        assert!(a.confirms_readback(&fresh).unwrap(), "{i}/readback");
        let mut sorted = fresh.sections.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|r| r.raw().string(8).unwrap().unwrap());
        let at = sorted.iter().position(|r| r.id() == id).unwrap();
        for (i, m) in p.appended_members().enumerate() {
            assert_eq!(sorted[at + 1 + i].id(), m.id());
        }
        assert_eq!(s.sections, old);
        let mut edited = fresh;
        let mut ids = p
            .appended_members()
            .map(|m| m.id().to_owned())
            .collect::<Vec<_>>();
        ids.push(id.clone());
        for (j, id) in ids.iter().enumerate() {
            let next = save_text(
                &edited,
                id,
                &format!("Edited {j} \u{65e5}\u{672c}\u{1f600} &amp; exact"),
            );
            for untouched in edited.sections.iter().filter(|r| r.id() != id) {
                assert_eq!(next.section(untouched.id()), Some(untouched));
            }
            edited = next;
        }
    }
}
