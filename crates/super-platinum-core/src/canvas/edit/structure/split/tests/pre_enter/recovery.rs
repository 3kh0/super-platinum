use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
use crate::canvas::edit::{EditSession, text::TextIntent};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("middle-pre-enter-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000816").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn v16_restart_review_and_cleanup_preserve_original_live_split_and_whole_record() {
    let e = vectors();
    for v in e["vectors"].as_array().unwrap().iter().filter(|v| {
        v["corpus"] == "ascii"
            && v["parentCadence"] != "untouched"
            && v["childCadence"] == "correction"
    }) {
        let (s, p) = attempt(v);
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
        assert_eq!(split.enter_text(), plain(p.enter_draft()).unwrap());
        assert_eq!(split.pre_enter_report_count(), p.pre_enter_updates().len());
        assert_eq!(split.parent_text(), plain(&p.witness.parent).unwrap());
        assert_eq!(
            split.initial_child_text(),
            plain(p.initial_child_draft()).unwrap()
        );
        assert!(split.has_child_updates());
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
            .position(|s| s.id() == TARGET)
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
        let archived: String = conn
            .query_row("SELECT body FROM local_reviews", [], |r| r.get(0))
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&archived).unwrap(), value);
        assert!(store.open().get(c.scope()).unwrap().is_none());
    }
}

fn rewrite(value: &mut Value, rows: &[RawMessage]) {
    let original = RawMessage::decode(
        &value["request"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut fields = rows.iter().map(|r| m(1, r)).collect::<Vec<_>>();
    fields.push(m(14, &original.message(14).unwrap().unwrap()));
    value["request"] = serde_json::to_value(msg(&fields).raw()).unwrap();
}

#[test]
fn v16_refuses_forged_prefix_lineage_legacy_versions_and_collapsed_revert_history() {
    let e = vectors();
    for cadence in ["correction", "exact-revert", "erase-retype"] {
        let v = e["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["parentCadence"] == cadence)
            .unwrap();
        let (s, p) = attempt(v);
        let original = serde_json::to_value(checkpoint(&p, v)).unwrap();
        for kind in 0..10 {
            let mut bad = original.clone();
            match kind {
                0 => bad["version"] = 8.into(),
                1 => bad["action"]["SplitPreEnter"]["pre_enter_updates"] = serde_json::json!([]),
                2 => bad["action"]["SplitPreEnter"]["enter_draft"] = "forged".into(),
                3 => bad["action"]["SplitPreEnter"]["offset_utf16"] = 0.into(),
                4 => {
                    bad["action"]["SplitPreEnter"]["pre_enter_updates"] =
                        serde_json::json!(["forged", p.enter_draft().rtml().unwrap()])
                }
                5 => bad["session"] = "111111111111".into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    let mut wire = rows[0].message(38).unwrap().unwrap();
                    match kind {
                        6 => wire = wire.remove(10).unwrap(),
                        7 => {
                            let lineage = wire
                                .message(10)
                                .unwrap()
                                .unwrap()
                                .replace_fields(4, &[n(4, 1)])
                                .unwrap();
                            wire = wire.replace_fields(10, &[m(10, &lineage)]).unwrap();
                        }
                        8 => {
                            // A same-visible-text original document cannot replace
                            // the live exact-revert/retype lineage prefix.
                            let seed =
                                s.section(TARGET)
                                    .unwrap()
                                    .crdt()
                                    .unwrap()
                                    .unwrap_or_else(|| {
                                        text::crdt::bootstrap(
                                            s.section(TARGET).unwrap(),
                                            &p.witness.before,
                                            p.session_id(),
                                            NOW,
                                        )
                                        .unwrap()
                                    });
                            let doc = Automerge::load(seed.bytes(5).unwrap().unwrap()).unwrap();
                            let lineage = wire
                                .message(10)
                                .unwrap()
                                .unwrap()
                                .replace_fields(7, &[F::bytes(7, &doc.save()).unwrap()])
                                .unwrap()
                                .replace_fields(
                                    5,
                                    &[t(
                                        5,
                                        &serde_json::to_string(
                                            &doc.get_heads()
                                                .iter()
                                                .map(ToString::to_string)
                                                .collect::<Vec<_>>(),
                                        )
                                        .unwrap(),
                                    )],
                                )
                                .unwrap();
                            wire = wire.replace_fields(10, &[m(10, &lineage)]).unwrap();
                        }
                        _ => {
                            wire = wire
                                .replace_fields(6, &[t(6, wire.string(12).unwrap().unwrap())])
                                .unwrap()
                        }
                    }
                    rows[0] = rows[0].replace_fields(38, &[m(38, &wire)]).unwrap();
                    rewrite(&mut bad, &rows);
                }
            }
            let checkpoint: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(checkpoint).is_err(),
                "{cadence} tamper {kind}"
            );
        }
        let mut rows = pair(&p, &s);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let changed_lineage = p.lineage.replace_fields(4, &[n(4, 1)]).unwrap();
        let changed_wire = rows[0]
            .crdt()
            .unwrap()
            .unwrap()
            .replace_fields(10, &[m(10, &changed_lineage)])
            .unwrap();
        let mut altered = rows.clone();
        altered[0] = change(&altered[0], m(38, &changed_wire));
        assert!(p.validate_ack(&ack(&altered)).is_err());
        assert!(
            !accepted
                .confirms_readback(&readback(&s, &altered))
                .unwrap_or(false)
        );
        let forged = crdt::insert(p.child_section(), p.child_draft(), p.session_id(), NOW).unwrap();
        rows[1] = change(&rows[1], m(38, &forged));
        assert!(p.validate_ack(&ack(&rows)).is_err());
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
fn saved_pre_enter_split_allows_independent_child_then_parent_edits_with_live_lineage() {
    let e = vectors();
    for v in e["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["parentCadence"] != "untouched" && v["childCadence"] == "correction")
    {
        let (s, p) = attempt(v);
        let mut current = readback(&s, &pair(&p, &s));
        let lineage = current
            .section(TARGET)
            .unwrap()
            .crdt()
            .unwrap()
            .unwrap()
            .message(10)
            .unwrap();
        save_text(&mut current, p.child_section(), "Child continued");
        save_text(&mut current, TARGET, "Parent continued");
        assert_eq!(
            current
                .section(TARGET)
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
                .message(10)
                .unwrap(),
            lineage
        );
        assert!(TextIntent::capture(&current, p.child_section(), 100).is_ok());
    }
}
