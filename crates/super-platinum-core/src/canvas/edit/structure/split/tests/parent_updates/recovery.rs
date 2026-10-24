use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
use crate::canvas::edit::{EditSession, text::TextIntent};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("middle-parent-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000817").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn v17_restart_review_keeps_initial_parent_final_empty_and_atomic_whole_record() {
    let e = vectors();
    for v in e["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["corpus"] == "ascii" && v["schedule"] == "parent-only")
    {
        let (s, _, p) = attempt(v);
        let c = checkpoint(&p, v);
        let value = serde_json::to_value(&c).unwrap();
        let decoded: Checkpoint = serde_json::from_value(value.clone()).unwrap();
        assert!(decoded == c);
        let store = Store::new();
        let receipt = match store.open().begin(decoded).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let mut journal = store.open();
        let restored = journal.get(c.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        assert!(matches!(
            journal.begin(c.clone()).unwrap(),
            Begin::Blocked(_)
        ));
        let observed = readback(&s, &pair(&p, &s)[..1]);
        let review = journal.local_review(&restored, &observed).unwrap();
        let split = review.split().unwrap();
        assert_eq!(split.original_text(), plain(&p.witness.before).unwrap());
        assert_eq!(
            split.initial_parent_text(),
            plain(p.initial_parent_draft()).unwrap()
        );
        assert_eq!(
            split.parent_text(),
            p.parent_draft()
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
        );
        assert_eq!(split.parent_report_count(), p.parent_updates().len());
        assert_eq!(split.pre_enter_report_count(), p.pre_enter_updates().len());
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &observed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .is_err()
        );
        assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
        assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
        conn.execute_batch("DROP TRIGGER reject_retirement;")
            .unwrap();
        let mut changed = observed.clone();
        let i = changed
            .sections
            .iter()
            .position(|r| r.id() == p.target_section())
            .unwrap();
        changed.sections[i] = change(&changed.sections[i], n(2, 3000));
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &changed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .is_err()
        );
        assert!(
            journal
                .remove_local_lock(
                    &review,
                    &observed,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    123
                )
                .unwrap()
        );
        let archive: String = conn
            .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&archive).unwrap(), value);
        assert!(store.open().get(c.scope()).unwrap().is_none());
    }
}

fn rewrite(value: &mut Value, rows: &[RawMessage]) {
    let bytes = value["request"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let old = RawMessage::decode(&bytes).unwrap();
    let mut fields = rows.iter().map(|r| m(1, r)).collect::<Vec<_>>();
    fields.push(m(14, &old.message(14).unwrap().unwrap()));
    value["request"] = serde_json::to_value(msg(&fields).raw()).unwrap();
}

#[test]
fn v17_forged_reports_initial_lineage_final_history_and_legacy_version_refuse() {
    let e = vectors();
    for cadence in ["correction", "erase", "exact-revert"] {
        let v = e["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["postParentCadence"] == cadence)
            .unwrap();
        let (s, baseline, p) = attempt(v);
        let c = checkpoint(&p, v);
        let original = serde_json::to_value(c).unwrap();
        for kind in 0..10 {
            let mut bad = original.clone();
            match kind {
                0 => bad["version"] = 16.into(),
                1 => bad["action"]["SplitParent"]["parent_updates"] = serde_json::json!([]),
                2 => {
                    bad["action"]["SplitParent"]["parent_updates"] =
                        serde_json::json!([p.initial_parent_draft().rtml().unwrap()])
                }
                3 => {
                    bad["action"]["SplitParent"]["parent_draft"] =
                        p.parent_draft().rtml().unwrap().into()
                }
                4 => {
                    bad["action"]["SplitParent"]["parent_updates"] =
                        serde_json::json!(["unobserved", p.parent_draft().rtml().unwrap()])
                }
                5 => bad["session"] = "111111111111".into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    let mut wire = rows[0].message(38).unwrap().unwrap();
                    match kind {
                        6 => wire = baseline.parent_wire.clone(),
                        7 => {
                            let lineage = wire
                                .message(10)
                                .unwrap()
                                .unwrap()
                                .replace_fields(4, &[n(4, 0)])
                                .unwrap();
                            wire = wire.replace_fields(10, &[m(10, &lineage)]).unwrap();
                        }
                        8 => {
                            let doc = Automerge::load(wire.bytes(5).unwrap().unwrap()).unwrap();
                            let lineage = wire
                                .message(10)
                                .unwrap()
                                .unwrap()
                                .replace_fields(7, &[F::bytes(7, &doc.save()).unwrap()])
                                .unwrap();
                            wire = wire.replace_fields(10, &[m(10, &lineage)]).unwrap();
                        }
                        _ => {
                            wire = wire
                                .replace_fields(
                                    6,
                                    &[t(6, baseline.parent_wire.string(12).unwrap().unwrap())],
                                )
                                .unwrap()
                        }
                    }
                    rows[0] = rows[0].replace_fields(38, &[m(38, &wire)]).unwrap();
                    rewrite(&mut bad, &rows);
                }
            }
            // Exact revert has initial==final, so replacing the initial draft is
            // byte-identical. All genuinely altered evidence must refuse.
            if bad == original {
                continue;
            }
            let decoded: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(decoded).is_err(),
                "{cadence}/{kind}"
            );
        }
        let rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let wire = rows[0].crdt().unwrap().unwrap();
        let live = Automerge::load(p.lineage.bytes(7).unwrap().unwrap()).unwrap();
        let unprocessed = wire
            .replace_fields(6, &[t(6, &live.get_heads()[0].to_string())])
            .unwrap();
        let mut forged = rows.clone();
        forged[0] = change(&forged[0], m(38, &unprocessed));
        let current = readback(&s, &forged);
        assert!(p.validate_ack(&ack(&forged)).is_err());
        assert!(!accepted.confirms_readback(&current).unwrap_or(false));
        assert!(TextIntent::capture(&current, p.target_section(), 100).is_err());
    }
}

fn save_text(s: &mut CanvasCollabSnapshot, id: &str, value: &str) {
    let mut session = EditSession::new();
    let p = TextIntent::capture(s, id, 100)
        .unwrap()
        .prepare(s, &mut session, &draft(value), 100)
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let source = s.section(id).unwrap();
    let seq = source.sequence().unwrap().unwrap() + 1000;
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
        &change(&source.merge_partial(&update).unwrap(), n(2, seq)),
        m(38, &wire),
    );
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    *s = readback(s, &[row]);
    assert!(accepted.confirms_readback(s).unwrap());
    session.remember_text_accepted(&accepted).unwrap();
}

#[test]
fn coalesced_parent_split_history_remains_editable_in_both_later_row_orders() {
    let e = vectors();
    for v in e["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["schedule"] == "parent-only")
    {
        let (s, _, p) = attempt(v);
        for parent_first in [false, true] {
            let mut current = readback(&s, &pair(&p, &s));
            let lineage = p.lineage.clone();
            let order = if parent_first {
                [
                    (p.target_section(), "Parent continued"),
                    (p.child_section(), "Child continued"),
                ]
            } else {
                [
                    (p.child_section(), "Child continued"),
                    (p.target_section(), "Parent continued"),
                ]
            };
            for (id, text) in order {
                save_text(&mut current, id, text);
            }
            assert_eq!(
                current
                    .section(p.target_section())
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .message(10)
                    .unwrap(),
                Some(lineage)
            );
            assert!(TextIntent::capture(&current, p.target_section(), 100).is_ok());
            assert!(TextIntent::capture(&current, p.child_section(), 100).is_ok());
        }
    }
}
