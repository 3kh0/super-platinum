use super::pre_enter::{history, reports};
use super::*;
use serde_json::Value;

mod recovery;
const NOW: u64 = 1_792_000_000_000_000;
fn vectors() -> Value {
    serde_json::from_str(include_str!("parent_updates/original.json")).unwrap()
}
fn draft(text: &str) -> TextDraft {
    TextDraft::from_rtml(text).unwrap()
}
fn attempt(v: &Value) -> (CanvasCollabSnapshot, PreparedSplitEdit, PreparedSplitEdit) {
    let (s, baseline) = super::pre_enter::attempt(v);
    let w = baseline
        .witness
        .clone()
        .with_parent_updates(&reports(&v["postParentReports"]))
        .unwrap();
    let p = PreparedSplitEdit::build(Arc::clone(&baseline.intent), w, NOW).unwrap();
    (s, baseline, p)
}
fn checkpoint(
    p: &PreparedSplitEdit,
    v: &Value,
) -> crate::canvas::edit::structure::recovery::Checkpoint {
    p.checkpoint().unwrap_or_else(|e| {
        panic!(
            "checkpoint {}/{}/{}/{}/{}: {e:?}",
            v["mode"], v["corpus"], v["parentCadence"], v["postParentCadence"], v["schedule"]
        )
    })
}

#[test]
fn original_54_middle_parent_vectors_keep_initial_lineage_and_every_report() {
    let evidence = vectors();
    assert_eq!(evidence["count"], 54);
    assert_eq!(
        evidence["provenance"]["bindings"],
        "527fccdda6af16a74ebea899e80b5d24f8b16bf5b8872c943d5292b9e25a9fee"
    );
    let mut groups = std::collections::BTreeMap::<String, Value>::new();
    let mut equal_schedules = 0;
    for v in evidence["vectors"].as_array().unwrap() {
        let key = format!(
            "{}/{}/{}/{}/{}",
            v["mode"], v["corpus"], v["parentCadence"], v["postParentCadence"], v["childCadence"]
        );
        if let Some(prior) = groups.get(&key) {
            assert_eq!(prior["wireSha256"], v["wireSha256"]);
            assert_eq!(prior["parentHistory"], v["parentHistory"]);
            assert_eq!(prior["childHistory"], v["childHistory"]);
            equal_schedules += 1;
        } else {
            groups.insert(key, v.clone());
        }
        let (s, baseline, p) = attempt(v);
        assert_eq!(p.lineage, baseline.lineage);
        assert_eq!(p.child_wire, baseline.child_wire);
        assert!(p.initial_parent_draft() == &draft(v["liveLeft"].as_str().unwrap()));
        assert!(p.parent_draft() == &draft(v["finalParent"].as_str().unwrap()));
        assert!(p.parent_updates() == reports(&v["postParentReports"]));
        assert_eq!(p.lineage.uint(4).unwrap(), v["serializedOffset"].as_u64());
        let live = Automerge::load(p.lineage.bytes(7).unwrap().unwrap()).unwrap();
        let initial = Automerge::load(baseline.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let parent = Automerge::load(p.parent_wire.bytes(5).unwrap().unwrap()).unwrap();
        let child = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap()).unwrap();
        history(&live, &v["liveHistory"]);
        history(&initial, &v["initialParentHistory"]);
        history(&parent, &v["parentHistory"]);
        history(&child, &v["childHistory"]);
        for c in initial.get_changes(&[]) {
            assert_eq!(
                parent.get_change_by_hash(&c.hash()).unwrap().raw_bytes(),
                c.raw_bytes()
            );
        }
        assert!(
            !validation::processed(&p.parent_wire, &initial.get_heads()[0].to_string()).unwrap()
        );
        assert!(validation::processed(&p.parent_wire, &parent.get_heads()[0].to_string()).unwrap());
        assert_ne!(initial.get_heads(), parent.get_heads());
        let c = checkpoint(&p, v);
        let value = serde_json::to_value(&c).unwrap();
        assert_eq!(value["version"], 17);
        assert_eq!(value["draft"], v["original"]);
        assert_eq!(
            value["action"]["SplitParent"]["parent_draft"],
            v["liveLeft"]
        );
        assert_eq!(
            value["action"]["SplitParent"]["parent_updates"],
            v["postParentReports"]
        );
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let current = readback(&s, &rows);
        assert!(accepted.confirms_readback(&current).unwrap());
        assert!(p.validate_ack(&ack(&rows[..1])).is_err());
        assert!(p.validate_ack(&ack(&rows[1..])).is_err());
        assert!(!accepted.confirms_readback(&s).unwrap_or(false));
        let mut missing = rows.clone();
        missing[0] = Section::from_raw(missing[0].raw().remove(12).unwrap()).unwrap();
        assert!(p.validate_ack(&ack(&missing)).is_err());
        let mut old = rows.clone();
        old[0] = pair(&baseline, &s)[0].clone();
        assert!(p.validate_ack(&ack(&old)).is_err());
        assert!(
            !accepted
                .confirms_readback(&readback(&s, &old))
                .unwrap_or(false)
        );
        assert!(
            crate::canvas::edit::text::TextIntent::capture(&current, p.target_section(), 100)
                .is_ok()
        );
    }
    assert_eq!(equal_schedules, 24);
}

#[test]
fn middle_parent_reports_bind_initial_half_allow_empty_revert_and_share_budget() {
    let w = witness();
    assert!(w.clone().with_parent_updates(&[w.parent.clone()]).is_err());
    assert!(
        w.clone()
            .with_parent_updates(&[draft("x"), draft("x")])
            .is_err()
    );
    let reports = vec![draft(""), w.parent.clone()];
    let changed = w.clone().with_parent_updates(&reports).unwrap();
    assert!(changed.initial_parent_draft() == &w.parent && changed.parent_draft() == &w.parent);
    assert!(changed.parent_updates() == reports);
    for rich in ["<b>bold</b>", "line<br>break", "a\u{feff}b"] {
        assert!(w.clone().with_parent_updates(&[draft(rich)]).is_err());
    }
    let many = (0..4096)
        .map(|i| draft(if i % 2 == 0 { "x" } else { "y" }))
        .collect::<Vec<_>>();
    assert!(
        w.clone()
            .with_parent_updates(&many)
            .unwrap()
            .with_child_updates(&[draft("child")])
            .is_err()
    );
    assert!(
        w.clone()
            .with_child_updates(&[draft("child")])
            .unwrap()
            .with_parent_updates(&many)
            .is_err()
    );
    let s = fixture();
    let p = SplitIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(&s, &changed, 100)
        .unwrap();
    let mut fresh = s.clone();
    let i = fresh
        .sections
        .iter()
        .position(|r| r.id() == "item")
        .unwrap();
    fresh.sections[i] = change(&fresh.sections[i], n(2, 2000));
    assert!(p.validate_snapshot(&fresh, 100).is_err());
    let empty = PreparedSplitEdit::build(
        Arc::clone(&p.intent),
        w.with_parent_updates(&[draft("")]).unwrap(),
        100,
    )
    .unwrap();
    let rows = pair(&empty, &s);
    let accepted = empty.validate_ack(&ack(&rows)).unwrap();
    assert!(accepted.confirms_readback(&readback(&s, &rows)).unwrap());
}
