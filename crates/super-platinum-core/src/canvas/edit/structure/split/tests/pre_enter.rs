use super::*;
use serde_json::Value;

mod recovery;

const TARGET: &str = "temp:C:xyz0000000000000000000000001";
const NOW: u64 = 1_792_000_000_000_000;
fn vectors() -> Value {
    serde_json::from_str(include_str!("pre_enter/original.json")).unwrap()
}
fn draft(value: &str) -> TextDraft {
    TextDraft::from_rtml(value).unwrap()
}
pub(super) fn reports(value: &Value) -> Vec<TextDraft> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| draft(v.as_str().unwrap()))
        .collect()
}
pub(super) fn attempt(v: &Value) -> (CanvasCollabSnapshot, PreparedSplitEdit) {
    let mut s = fixture();
    let row = s.sections.iter_mut().find(|s| s.id() == "item").unwrap();
    *row = change(row, m(12, &body(v["original"].as_str().unwrap(), true)));
    *row = change(row, n(2, 1000));
    *row = change(row, t(1, TARGET));
    if let Some(encoded) = v["sourceSession"].as_str() {
        let raw = crate::slack::api::decode_canvas_base64(encoded).unwrap();
        let wire = RawMessage::decode(&raw)
            .unwrap()
            .replace_fields(1, &[t(1, TARGET)])
            .unwrap()
            .replace_fields(4, &[n(4, NOW + 1_000_000)])
            .unwrap();
        *row = change(row, m(38, &wire));
    }
    let original = draft(v["original"].as_str().unwrap());
    let enter = draft(v["liveDraft"].as_str().unwrap());
    let left = draft(v["liveLeft"].as_str().unwrap());
    let tail = draft(v["tail"].as_str().unwrap());
    let pre = reports(&v["preReports"]);
    let w = if pre.is_empty() {
        SplitWitness::new(
            &original,
            v["visibleOffset"].as_u64().unwrap() as usize,
            &left,
            &tail,
        )
    } else {
        SplitWitness::new_with_pre_enter_updates(
            &original,
            &enter,
            v["visibleOffset"].as_u64().unwrap() as usize,
            &left,
            &tail,
            &pre,
        )
    }
    .unwrap()
    .with_child_updates(&reports(&v["childReports"]))
    .unwrap();
    let mut intent = SplitIntent::capture(&s, TARGET, NOW).unwrap();
    intent.session = "0123456789ab".into();
    let p = intent.prepare(&s, &w, NOW).unwrap();
    (s, p)
}
fn checkpoint(
    p: &PreparedSplitEdit,
    v: &Value,
) -> crate::canvas::edit::structure::recovery::Checkpoint {
    p.checkpoint().unwrap_or_else(|error| {
        panic!(
            "checkpoint {}/{}/{}/{}: {error:?}",
            v["mode"], v["corpus"], v["parentCadence"], v["childCadence"]
        )
    })
}
pub(super) fn history(doc: &Automerge, expected: &Value) {
    let changes = doc.get_changes(&[]);
    let expected = expected.as_array().unwrap();
    assert_eq!(changes.len(), expected.len());
    for (change, e) in changes.iter().zip(expected) {
        assert_eq!(
            change.actor_id().to_hex_string(),
            e["actor"].as_str().unwrap()
        );
        assert_eq!(change.seq(), e["seq"].as_u64().unwrap());
        let at = doc.fork_at(&[change.hash()]).unwrap();
        let (_, object) = at.get(automerge::ROOT, "text").unwrap().unwrap();
        assert_eq!(at.text(&object).unwrap(), e["text"].as_str().unwrap());
        assert!(at.marks(&object).unwrap().is_empty());
        // Dependencies retain the original topology even across the original
        // WASM and Rust document serialization representations.
        let indices = change
            .deps()
            .iter()
            .map(|h| changes.iter().position(|c| c.hash() == *h).unwrap())
            .collect::<Vec<_>>();
        let original_indices = e["deps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| expected.iter().position(|c| c["hash"] == *h).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(indices, original_indices);
    }
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

#[test]
fn original_78_middle_vectors_bind_pre_enter_history_and_live_lineage() {
    let evidence = vectors();
    assert_eq!(evidence["count"], 78);
    for v in evidence["vectors"].as_array().unwrap() {
        let (s, p) = attempt(v);
        let live = Automerge::load(p.lineage.bytes(7).unwrap().unwrap()).unwrap();
        let parent = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let child = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        history(&live, &v["liveHistory"]);
        history(&parent, &v["parentHistory"]);
        history(&child, &v["childHistory"]);
        assert_eq!(p.lineage.uint(4).unwrap(), v["serializedOffset"].as_u64());
        text::crdt::check_document_state(&live, p.enter_draft(), false).unwrap();
        assert!(p.pre_enter_updates() == reports(&v["preReports"]));
        let additions = parent.get_changes(&live.get_heads());
        assert_eq!(additions.len(), 1);
        assert_eq!(additions[0].deps(), live.get_heads());
        for c in live.get_changes(&[]) {
            assert_eq!(
                parent.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
                c.raw_bytes()
            );
        }
        if let Some(source) = s.section(TARGET).unwrap().crdt().unwrap() {
            for c in Automerge::load(source.bytes(5).unwrap().unwrap())
                .unwrap()
                .get_changes(&[])
            {
                assert!(live.get_change_by_hash(&c.hash()).is_some());
            }
        }
        let original_processed = s
            .section(TARGET)
            .unwrap()
            .crdt()
            .unwrap()
            .as_ref()
            .map(processed)
            .unwrap_or_else(|| vec![parent.get_changes(&[])[0].hash().to_string()]);
        let mut expected = original_processed.clone();
        expected.push(parent.get_heads()[0].to_string());
        expected.extend(original_processed);
        assert_eq!(processed(&p.parent_wire), expected);
        assert_eq!(
            processed(&p.child_wire),
            vec![child.get_heads()[0].to_string()]
        );
        let checkpoint = checkpoint(&p, v);
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(value["draft"], v["original"]);
        if !p.pre_enter_updates().is_empty() {
            assert_eq!(value["version"], 16);
            let tags = p
                .parent_wire
                .fields()
                .iter()
                .map(|f| f.tag())
                .collect::<Vec<_>>();
            assert!(
                tags.iter().rposition(|t| *t == 6).unwrap()
                    < tags.iter().position(|t| *t == 12).unwrap()
            );
            assert!(
                tags.iter().position(|t| *t == 12).unwrap()
                    < tags.iter().position(|t| *t == 10).unwrap()
            );
            assert_eq!(
                value["action"]["SplitPreEnter"]["pre_enter_updates"],
                v["preReports"]
            );
            assert_eq!(
                value["action"]["SplitPreEnter"]["enter_draft"],
                v["liveDraft"]
            );
        } else {
            assert_eq!(
                value["version"],
                if p.child_updates().is_empty() { 3 } else { 8 }
            );
        }
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
        assert!(p.validate_ack(&ack(&rows[..1])).is_err());
        assert!(p.validate_ack(&ack(&rows[1..])).is_err());
        assert!(!accepted.confirms_readback(&s).unwrap_or(false));
    }
}

#[test]
fn pre_enter_middle_boundary_reports_and_original_source_cannot_substitute_each_other() {
    let e = vectors();
    let v = e["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["parentCadence"] == "correction")
        .unwrap();
    let (s, p) = attempt(v);
    let w = &p.witness;
    for offset in [0, 9, 999] {
        assert!(
            SplitWitness::new_with_pre_enter_updates(
                &w.before,
                w.enter_draft(),
                offset,
                &w.parent,
                &w.child,
                &w.pre_updates
            )
            .is_err()
        );
    }
    assert!(
        SplitWitness::new_with_pre_enter_updates(
            &w.before,
            &w.before,
            w.offset,
            &w.parent,
            &w.child,
            &w.pre_updates
        )
        .is_err()
    );
    assert!(
        SplitWitness::new_with_pre_enter_updates(
            &w.before,
            w.enter_draft(),
            w.offset,
            &w.parent,
            &w.child,
            &[]
        )
        .is_err()
    );
    let mut duplicate = w.pre_updates.clone();
    duplicate.push(w.enter_draft().clone());
    assert!(
        SplitWitness::new_with_pre_enter_updates(
            &w.before,
            w.enter_draft(),
            w.offset,
            &w.parent,
            &w.child,
            &duplicate
        )
        .is_err()
    );
    let mut wrong_source = w.clone();
    wrong_source.before = draft("Another source");
    assert!(
        SplitIntent::capture(&s, TARGET, NOW)
            .unwrap()
            .prepare(&s, &wrong_source, NOW)
            .is_err()
    );
    let mut fresh = s.clone();
    let i = fresh
        .sections
        .iter()
        .position(|s| s.id() == TARGET)
        .unwrap();
    fresh.sections[i] = change(&fresh.sections[i], n(2, 2000));
    assert!(p.validate_snapshot(&fresh, NOW).is_err());
    let child_reports = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(w.clone().with_child_updates(&child_reports).is_err());
    let original = draft("a\u{1f600}b");
    let enter = draft("xa\u{1f600}b");
    assert!(
        SplitWitness::new_with_pre_enter_updates(
            &original,
            &enter,
            3,
            &draft("xa"),
            &draft("\u{1f600}b"),
            &[enter.clone()]
        )
        .is_err()
    );
    assert!(
        SplitWitness::new_with_pre_enter_updates(
            &original,
            &enter,
            2,
            &draft("xa"),
            &draft("\u{1f600}b"),
            &[enter.clone()]
        )
        .is_ok()
    );
}
