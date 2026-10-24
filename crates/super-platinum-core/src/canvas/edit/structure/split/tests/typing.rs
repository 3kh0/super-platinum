use super::*;
use crate::canvas::edit::{EditSession, text::TextIntent};
use serde_json::Value;

mod negatives;
mod recovery;

fn vectors() -> Value {
    serde_json::from_str(include_str!("typing/original.json")).unwrap()
}
fn draft(value: &str) -> TextDraft {
    TextDraft::from_rtml(value).unwrap()
}
fn source(v: &Value) -> CanvasCollabSnapshot {
    let mut s = fixture();
    let row = s.sections.iter_mut().find(|r| r.id() == "item").unwrap();
    *row = change(row, m(12, &body(v["full"].as_str().unwrap(), true)));
    *row = change(row, n(2, 1000));
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
fn attempt(v: &Value) -> (CanvasCollabSnapshot, PreparedSplitEdit) {
    assert_eq!(
        plain(&draft(v["left"].as_str().unwrap()))
            .unwrap()
            .encode_utf16()
            .count(),
        v["visibleOffset"].as_u64().unwrap() as usize,
        "reference fixture UTF-16 offset"
    );
    assert_eq!(
        format!(
            "{}{}",
            v["left"].as_str().unwrap(),
            v["tail"].as_str().unwrap()
        ),
        v["full"].as_str().unwrap(),
        "reference fixture exact halves"
    );
    let s = source(v);
    let updates = v["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| draft(v.as_str().unwrap()))
        .collect::<Vec<_>>();
    let w = SplitWitness::new(
        &draft(v["full"].as_str().unwrap()),
        v["visibleOffset"].as_u64().unwrap() as usize,
        &draft(v["left"].as_str().unwrap()),
        &draft(v["tail"].as_str().unwrap()),
    )
    .unwrap()
    .with_child_updates(&updates)
    .unwrap();
    let p = SplitIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(&s, &w, 100)
        .unwrap();
    (s, p)
}
fn processed(wire: &RawMessage) -> Vec<String> {
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
        .collect()
}
fn apply_patch(text: &mut Vec<u16>, patch: &Value) {
    assert_eq!(patch["path"][0], "text");
    let at = patch["path"][1].as_u64().unwrap() as usize;
    match patch["action"].as_str().unwrap() {
        "del" => {
            text.drain(at..at + patch["length"].as_u64().unwrap_or(1) as usize);
        }
        "splice" => {
            text.splice(at..at, patch["value"].as_str().unwrap().encode_utf16());
        }
        other => panic!("unknown original patch {other}"),
    }
}

#[test]
fn middle_typing_preserves_original_split_and_every_live_report_patch() {
    for v in vectors()["vectors"].as_array().unwrap() {
        let label = format!("{}/{}/{}", v["parentMode"], v["corpus"], v["cadence"]);
        let (s, p) = attempt(v);
        assert_eq!(
            p.initial_child_draft().rtml().unwrap(),
            v["tail"].as_str().unwrap()
        );
        assert_eq!(
            p.child_draft().rtml().unwrap(),
            v["states"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_str()
                .unwrap()
        );
        assert_eq!(p.lineage.uint(4).unwrap(), v["serializedOffset"].as_u64());
        let before = Automerge::load(p.lineage.bytes(7).unwrap().unwrap()).unwrap();
        let parent = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let old = s.section("item").unwrap().crdt().unwrap();
        let old_heads = old
            .as_ref()
            .map(processed)
            .unwrap_or_else(|| vec![before.get_heads()[0].to_string()]);
        let mut expected = old_heads.clone();
        expected.push(parent.get_heads()[0].to_string());
        expected.extend(old_heads);
        assert_eq!(processed(&p.parent_wire), expected, "{label}");
        for change in before.get_changes(&[]) {
            assert_eq!(
                parent
                    .get_change_by_hash(&change.hash())
                    .unwrap()
                    .raw_bytes(),
                change.raw_bytes(),
                "{label}"
            );
        }
        let doc = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        let changes = doc.get_changes(&[]);
        let patches = v["patchReports"]["child"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r.as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(changes.len(), 1 + patches.len(), "{label}");
        assert_eq!(processed(&p.child_wire), [doc.get_heads()[0].to_string()]);
        assert!(!validation::processed(&p.child_wire, &changes[1].hash().to_string()).unwrap());
        let mut expected_text = vec![0xfeff];
        for (i, c) in changes.iter().enumerate() {
            assert_eq!(
                c.actor_id().to_hex_string(),
                format!("{}00000000", p.session_id())
            );
            assert_eq!(c.seq(), (i + 1) as u64);
            assert_eq!(
                c.deps(),
                if i == 0 {
                    vec![]
                } else {
                    vec![changes[i - 1].hash()]
                }
            );
            if i > 0 {
                apply_patch(&mut expected_text, patches[i - 1]);
            }
            let at = doc.fork_at(&[c.hash()]).unwrap();
            let (_, object) = at.get(automerge::ROOT, "text").unwrap().unwrap();
            assert_eq!(
                at.text(&object).unwrap(),
                String::from_utf16(&expected_text).unwrap(),
                "{label} change {i}"
            );
            assert!(at.marks(&object).unwrap().is_empty());
        }
        assert!(p.checkpoint().unwrap().is_split_typing(), "{label}");
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
    }
}

fn save_text(s: &mut CanvasCollabSnapshot, id: &str, value: &str) {
    let mut session = EditSession::new();
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(s, &mut session, &draft(value), 100)
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let original = s.section(id).unwrap();
    let seq = original.sequence().unwrap().unwrap() + 1000;
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
    let row = change(
        &change(&original.merge_partial(&update).unwrap(), n(2, seq)),
        m(38, &wire),
    );
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    *s = readback(s, &[row]);
    assert!(accepted.confirms_readback(s).unwrap());
    session.remember_text_accepted(&accepted).unwrap();
}

#[test]
fn typed_split_child_then_parent_remain_editable_without_fabricated_processed_tail() {
    for v in vectors()["vectors"].as_array().unwrap() {
        let (s, p) = attempt(v);
        let mut current = readback(&s, &pair(&p, &s));
        let lineage = current
            .section("item")
            .unwrap()
            .crdt()
            .unwrap()
            .unwrap()
            .message(10)
            .unwrap();
        assert!(TextIntent::capture(&current, "item", 100).is_ok());
        save_text(&mut current, p.child_section(), "Continued child 日本😀");
        save_text(&mut current, "item", "Continued parent 日本😀");
        assert_eq!(
            current
                .section("item")
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .message(10)
                .unwrap(),
            lineage
        );
        assert!(
            super::super::super::paragraph_actions(&current, &["item"], 100)[0]
                .as_ref()
                .unwrap()
                .delete
        );
        let child_before = current.section(p.child_section()).unwrap().clone();
        let parent_before = current.section("item").unwrap().clone();
        let deletion =
            crate::canvas::edit::structure::StructureIntent::delete(&current, "item", 100)
                .unwrap()
                .prepare(&current, 100)
                .unwrap();
        let update =
            Section::from_raw(deletion.document_data().message(1).unwrap().unwrap()).unwrap();
        let seq = parent_before.sequence().unwrap().unwrap() + 1000;
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
        assert_eq!(wire.message(10).unwrap(), lineage);
        let original_wire = parent_before.crdt().unwrap().unwrap();
        let original_doc = Automerge::load(original_wire.bytes(5).unwrap().unwrap()).unwrap();
        let deleted_doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
        assert_eq!(
            deleted_doc.get_changes(&[]).len(),
            original_doc.get_changes(&[]).len() + 1
        );
        assert!(
            original_doc
                .get_changes(&[])
                .iter()
                .all(|c| deleted_doc.get_change_by_hash(&c.hash()).is_some())
        );
        let tombstone = change(
            &change(&parent_before.merge_partial(&update).unwrap(), n(2, seq)),
            m(38, &wire),
        );
        let accepted = deletion.validate_ack(&ack(&[tombstone.clone()])).unwrap();
        current = readback(&current, &[tombstone]);
        assert!(accepted.confirms_readback(&current).unwrap());
        assert_eq!(current.section(p.child_section()), Some(&child_before));
        assert!(TextIntent::capture(&current, p.child_section(), 100).is_ok());
    }
}

#[test]
fn typed_witness_binds_initial_tail_and_report_limits_and_allows_restore() {
    let w = witness();
    assert!(w.clone().with_child_updates(&[w.child.clone()]).is_err());
    assert!(
        w.clone()
            .with_child_updates(&[draft("x"), draft("x")])
            .is_err()
    );
    assert!(
        w.clone()
            .with_child_updates(&[draft(""), w.child.clone()])
            .is_ok()
    );
    assert!(
        w.clone()
            .with_child_updates(&[])
            .unwrap()
            .updates
            .is_empty()
    );
    for value in ["<b>rich</b>", "line<br>break", "a\u{feff}b", "a\u{2028}b"] {
        assert!(
            w.clone().with_child_updates(&[draft(value)]).is_err(),
            "{value:?}"
        );
    }
    let reports = (0..4097)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(w.clone().with_child_updates(&reports).is_err());
    let reports = (0..130)
        .map(|i| {
            draft(&if i % 2 == 0 {
                "x".repeat(65536)
            } else {
                "y".repeat(65536)
            })
        })
        .collect::<Vec<_>>();
    assert!(w.with_child_updates(&reports).is_err());
}
