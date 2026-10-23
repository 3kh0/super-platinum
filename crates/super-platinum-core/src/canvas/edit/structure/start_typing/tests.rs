use super::*;
use crate::canvas::wire::EditAck;
use automerge::{Automerge, ReadDoc};
use serde_json::Value;
mod negative;
mod recovery;
fn t(tag: u32, s: &str) -> F {
    F::bytes(tag, s.as_bytes()).unwrap()
}
fn n(tag: u32, v: u64) -> F {
    F::uint(tag, v).unwrap()
}
fn m(tag: u32, v: &RawMessage) -> F {
    F::message(tag, v).unwrap()
}
fn msg(f: &[F]) -> RawMessage {
    RawMessage::from_fields(f).unwrap()
}
fn change(s: &Section, f: F) -> Section {
    Section::from_raw(
        s.raw()
            .merge_partial(&RawMessage::decode(f.raw()).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn draft(s: &str) -> TextDraft {
    TextDraft::from_rtml(s).unwrap()
}
fn vectors() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("tests/original.json")).unwrap()["vectors"]
        .as_array()
        .unwrap()
        .clone()
}
fn source(v: &Value) -> CanvasCollabSnapshot {
    let mut s = super::super::tests::fixture();
    let row = s.sections.iter_mut().find(|r| r.id() == "item").unwrap();
    *row = change(
        &change(row, n(2, 1000)),
        m(
            12,
            &split::validation::child_content(&draft(v["full"].as_str().unwrap())).unwrap(),
        ),
    );
    if let Some(encoded) = v["sourceSession"].as_str() {
        let bytes = crate::slack::api::decode_canvas_base64(encoded).unwrap();
        let wire = RawMessage::decode(&bytes)
            .unwrap()
            .replace_fields(1, &[t(1, "item")])
            .unwrap()
            .replace_fields(4, &[n(4, 1_000_000)])
            .unwrap();
        *row = change(row, m(38, &wire));
    }
    s
}
fn attempt(v: &Value) -> (CanvasCollabSnapshot, PreparedStartTypingEdit) {
    let s = source(v);
    let updates = v["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| draft(x.as_str().unwrap()))
        .collect::<Vec<_>>();
    let w = StartTypingWitness::new(&draft(v["full"].as_str().unwrap()), &updates).unwrap();
    let p = StartTypingIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(&s, &w, 100)
        .unwrap();
    (s, p)
}
fn canonical(p: &PreparedStartTypingEdit, s: &CanvasCollabSnapshot) -> Vec<Section> {
    p.document_data()
        .messages(1)
        .unwrap()
        .into_iter()
        .map(|raw| {
            let update = Section::from_raw(raw).unwrap();
            let old = s.section(update.id());
            let mut row = old.map_or_else(|| update.clone(), |o| o.merge_partial(&update).unwrap());
            let seq = old.map_or(0, |o| o.sequence().unwrap().unwrap()) + 1000;
            let wire = update
                .crdt()
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, seq)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            for f in [
                n(2, seq),
                t(6, s.thread_id()),
                t(7, s.document_id()),
                m(38, &wire),
            ] {
                row = change(&row, f);
            }
            if old.is_none() {
                let pos = position::base(row.raw().string(8).unwrap().unwrap())
                    .unwrap()
                    .to_owned();
                row = change(&change(&row, t(8, &pos)), t(21, &pos));
            }
            row
        })
        .collect()
}
fn ack(rows: &[Section]) -> EditAck {
    EditAck::decode(
        msg(&[
            n(1, 1),
            m(
                2,
                &msg(&rows.iter().map(|s| m(1, s.raw())).collect::<Vec<_>>()),
            ),
        ])
        .raw(),
    )
    .unwrap()
}
fn readback(s: &CanvasCollabSnapshot, rows: &[Section]) -> CanvasCollabSnapshot {
    let mut fresh = s.clone();
    for row in rows {
        if let Some(i) = fresh.sections.iter().position(|r| r.id() == row.id()) {
            fresh.sections[i] = row.clone();
        } else {
            fresh.sections.push(row.clone());
        }
    }
    fresh
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
fn history(w: &RawMessage) -> Automerge {
    Automerge::load(w.bytes(5).unwrap().unwrap()).unwrap()
}

#[test]
fn twelve_original_start_typing_vectors_keep_blank_bootstrap_and_exact_ordered_patch_history() {
    for v in vectors() {
        let (s, p) = attempt(&v);
        p.validate_snapshot(&s, 100).unwrap();
        p.checkpoint().unwrap();
        let rows = p.document_data().messages(1).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].string(1).unwrap(), Some("item"));
        assert_eq!(rows[1].string(1).unwrap(), Some(p.child_section()));
        assert_eq!(
            rows[0].fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
            [1, 2, 9, 12, 13, 19, 20, 33, 38]
        );
        assert_eq!(
            rows[1].fields().iter().map(|f| f.tag()).collect::<Vec<_>>(),
            [
                1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38
            ]
        );
        assert_eq!(rows[1].string(8).unwrap(), Some("aab:temp"));
        let blank = history(&p.child_wire);
        assert_eq!(blank.get_changes(&[]).len(), 1);
        text::crdt::check_document_state(&blank, &TextDraft::default(), false).unwrap();
        assert_eq!(
            processed(&p.child_wire),
            vec![p.child_wire.string(8).unwrap().unwrap()]
        );
        assert_eq!(
            p.child_wire.string(8).unwrap(),
            p.child_wire.string(12).unwrap()
        );
        assert!(!p.child_wire.has(10) && !p.parent_wire.has(10));
        let doc = history(&p.parent_wire);
        let initial = if let Some(w) = s.section("item").unwrap().crdt().unwrap() {
            history(&w)
        } else {
            doc.fork_at(&[p.parent_wire.string(8).unwrap().unwrap().parse().unwrap()])
                .unwrap()
        };
        assert_eq!(
            initial.get_changes(&[]).len(),
            v["sourceChanges"].as_u64().unwrap() as usize
        );
        let old_heads = if let Some(w) = s.section("item").unwrap().crdt().unwrap() {
            processed(&w)
        } else {
            vec![initial.get_heads()[0].to_string()]
        };
        let expected_processed = old_heads
            .iter()
            .cloned()
            .chain([doc.get_heads()[0].to_string()])
            .chain(old_heads.iter().cloned())
            .collect::<Vec<_>>();
        assert_eq!(processed(&p.parent_wire), expected_processed);
        for c in initial.get_changes(&[]) {
            assert!(doc.get_change_by_hash(&c.hash()).is_some());
        }
        let changes = doc.get_changes(&initial.get_heads());
        let mut i = 0;
        let mut previous = initial.get_heads();
        let mut text = v["full"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>();
        text.push(0xfeff);
        let mut seq = initial
            .get_changes(&[])
            .iter()
            .filter(|c| c.actor_id().to_hex_string() == format!("{}00000000", p.session_id()))
            .map(|c| c.seq())
            .max()
            .unwrap_or(0);
        for report in v["patches"].as_array().unwrap() {
            for patch in report.as_array().unwrap() {
                let index = patch["path"][1].as_u64().unwrap() as usize;
                if patch["action"] == "del" {
                    let len = patch["length"].as_u64().unwrap_or(1) as usize;
                    text.drain(index..index + len);
                } else {
                    text.splice(
                        index..index,
                        patch["value"].as_str().unwrap().encode_utf16(),
                    );
                }
                let c = &changes[i];
                seq += 1;
                assert_eq!(c.seq(), seq);
                assert_eq!(
                    c.actor_id().to_hex_string(),
                    format!("{}00000000", p.session_id())
                );
                assert_eq!(c.deps(), previous);
                let at = doc.fork_at(&[c.hash()]).unwrap();
                let (_, object) = at.get(automerge::ROOT, "text").unwrap().unwrap();
                assert_eq!(
                    at.text(&object).unwrap(),
                    String::from_utf16(&text).unwrap(),
                    "{} {} patch {i}",
                    v["corpus"],
                    v["cadence"]
                );
                previous = vec![c.hash()];
                i += 1;
            }
        }
        assert_eq!(i, changes.len());
        assert_eq!(previous, doc.get_heads());
        let canonical = canonical(&p, &s);
        let accepted = p.validate_ack(&ack(&canonical)).unwrap();
        let current = readback(&s, &canonical);
        assert!(accepted.confirms_readback(&current).unwrap());
        assert_eq!(
            text::canonical_draft(&canonical[0])
                .unwrap()
                .rtml()
                .unwrap(),
            v["states"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_str()
                .unwrap()
        );
        for row in s.sections.iter().filter(|r| r.id() != "item") {
            assert_eq!(current.section(row.id()), Some(row));
        }
    }
}

#[test]
fn both_start_typing_rows_accept_later_ordinary_edits_without_split_lineage() {
    use crate::canvas::edit::{EditSession, text::TextIntent};
    for v in vectors().into_iter().filter(|v| v["cadence"] == "erase") {
        let (s, p) = attempt(&v);
        let rows = canonical(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let mut current = readback(&s, &rows);
        assert!(accepted.confirms_readback(&current).unwrap());
        for (id, value) in [
            (p.child_section(), "Inserted row edited"),
            ("item", "Original 日本😀 edited"),
        ] {
            let old = current.section(id).unwrap().clone();
            let edit = TextIntent::capture(&current, id, 100)
                .unwrap()
                .prepare(&current, &mut EditSession::new(), &draft(value), 100)
                .unwrap();
            let update =
                Section::from_raw(edit.document_data().message(1).unwrap().unwrap()).unwrap();
            let seq = old.sequence().unwrap().unwrap() + 1000;
            let wire = update
                .crdt()
                .unwrap()
                .unwrap()
                .remove(12)
                .unwrap()
                .replace_fields(3, &[n(3, seq)])
                .unwrap()
                .replace_fields(4, &[n(4, 1_000_000)])
                .unwrap();
            let new = change(
                &change(&old.merge_partial(&update).unwrap(), n(2, seq)),
                m(38, &wire),
            );
            let done = edit.validate_ack(&ack(&[new.clone()])).unwrap();
            let fresh = readback(&current, &[new.clone()]);
            assert!(done.confirms_readback(&fresh).unwrap());
            let before = history(&old.crdt().unwrap().unwrap());
            let after = history(&wire);
            for c in before.get_changes(&[]) {
                assert!(after.get_change_by_hash(&c.hash()).is_some());
            }
            assert!(!wire.has(10));
            for r in current.sections.iter().filter(|r| r.id() != id) {
                assert_eq!(fresh.section(r.id()), Some(r));
            }
            current = fresh;
        }
    }
}
