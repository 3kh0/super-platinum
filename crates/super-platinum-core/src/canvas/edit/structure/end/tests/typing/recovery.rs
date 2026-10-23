use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
use automerge::transaction::Transactable;

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(
            std::env::temp_dir().join(format!("canvas-end-typing-{}.sqlite", uuid::Uuid::new_v4())),
        )
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000796").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn v6_typed_pair_survives_restart_and_review_with_final_child_and_ordered_reports() {
    for reused in [false, true] {
        for states in [
            &["New", "New draftx", "New draft", "New paragraph"][..],
            &["x", ""][..],
        ] {
            let (s, p) = typed(reused, states);
            let c = p.checkpoint().unwrap();
            assert!(c.is_end());
            assert_eq!(c.is_end_reuse(), reused);
            let json = serde_json::to_value(&c).unwrap();
            assert_eq!(json["version"], 6);
            assert_eq!(json["action"]["EndTyping"]["reused"], reused);
            let decoded: Checkpoint = serde_json::from_value(json).unwrap();
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
            // A review can see only the parent; this must not certify remote success.
            let mut observed = readback(&s, &pair(&p, &s));
            observed.sections.retain(|r| r.id() != p.child_section());
            let review = journal.local_review(&restored, &observed).unwrap();
            let end = review.end().unwrap();
            assert_eq!(end.reuses_child(), reused);
            assert_eq!(end.child_text(), *states.last().unwrap());
            assert_eq!(end.parent_text(), "Café & 日本😀");
            let conn = rusqlite::Connection::open(&store.0).unwrap();
            conn.execute_batch("CREATE TRIGGER reject_typing_retirement BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
            assert_eq!(journal.local_review_count(c.scope()).unwrap(), 0);
            assert!(store.open().get(c.scope()).unwrap().as_ref() == Some(&restored));
            conn.execute_batch("DROP TRIGGER reject_typing_retirement;")
                .unwrap();
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
            assert!(serde_json::from_str::<Checkpoint>(&archived).unwrap() == c);
            assert!(store.open().get(c.scope()).unwrap().is_none());
        }
    }
}

#[test]
fn v6_rejects_changed_mode_report_order_duplicates_missing_history_and_actor() {
    for reused in [false, true] {
        let (s, p) = typed(reused, &["New", "New draftx", "New draft", "New paragraph"]);
        let checkpoint = p.checkpoint().unwrap();
        let original = serde_json::to_value(&checkpoint).unwrap();
        let store = Store::new();
        for kind in 0..11 {
            let mut bad = original.clone();
            match kind {
                0 => bad["version"] = 5.into(),
                1 => bad["action"]["EndTyping"]["reused"] = (!reused).into(),
                2 => {
                    bad["action"]["EndTyping"]["child_updates"] =
                        serde_json::json!(["New paragraph"])
                }
                3 => {
                    bad["action"]["EndTyping"]["child_updates"] =
                        serde_json::json!(["New", "New draft", "New draftx", "New paragraph"])
                }
                4 => {
                    bad["action"]["EndTyping"]["child_updates"] = serde_json::json!([
                        "New",
                        "New",
                        "New draftx",
                        "New draft",
                        "New paragraph"
                    ])
                }
                5 => bad["session"] = "000000000000".into(),
                _ => {
                    let mut rows = p.data.messages(1).unwrap();
                    match kind {
                        6 => {
                            rows.pop();
                        }
                        7 => rows[1] = rows[0].clone(),
                        8 => {
                            let old = if reused {
                                s.section(p.child_section())
                                    .unwrap()
                                    .crdt()
                                    .unwrap()
                                    .unwrap()
                            } else {
                                crdt::insert(
                                    p.child_section(),
                                    &draft("New paragraph"),
                                    p.session_id(),
                                    100,
                                )
                                .unwrap()
                            };
                            rows[1] = rows[1].replace_fields(38, &[m(38, &old)]).unwrap();
                        }
                        9 => {
                            let mut doc = Automerge::load(p.child_wire.bytes(5).unwrap().unwrap())
                                .unwrap()
                                .with_actor("00000000000000000000".parse().unwrap());
                            let (_, obj) = doc.get(automerge::ROOT, "text").unwrap().unwrap();
                            let mut tx = doc.transaction();
                            tx.splice_text(&obj, 0, 0, "x").unwrap();
                            tx.commit();
                            let mut tx = doc.transaction();
                            tx.splice_text(&obj, 0, 1, "").unwrap();
                            tx.commit();
                            let head = doc.get_heads()[0].to_string();
                            let wire = p
                                .child_wire
                                .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
                                .unwrap()
                                .replace_fields(6, &[t(6, &head)])
                                .unwrap()
                                .replace_fields(12, &[t(12, &head)])
                                .unwrap();
                            rows[1] = rows[1].replace_fields(38, &[m(38, &wire)]).unwrap();
                        }
                        _ => rows[1] = rows[1].remove(12).unwrap(),
                    }
                    let data = p
                        .data
                        .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
                        .unwrap();
                    bad["request"] = serde_json::to_value(data.raw()).unwrap();
                }
            }
            let bad: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                store.open().begin(bad).is_err(),
                "reuse={reused}, corruption={kind}"
            );
            assert!(store.open().get(checkpoint.scope()).unwrap().is_none());
        }
        // Existing empty-only persisted actions keep their exact versions.
        let old = prepare(&s).checkpoint().unwrap();
        assert_eq!(
            serde_json::to_value(&old).unwrap()["version"],
            if reused { 5 } else { 4 }
        );
        assert!(matches!(
            store.open().begin(old).unwrap(),
            Begin::Started(_)
        ));
    }
}

#[test]
fn v6_refuses_reconstructed_patch_order_even_with_exact_final_text_and_actor() {
    use automerge::{PatchAction, transaction::CommitOptions};
    for reused in [false, true] {
        let (s, p) = typed(reused, &["New", "New draftx", "New draft", "New paragraph"]);
        let seed = if reused {
            s.section(p.child_section())
                .unwrap()
                .crdt()
                .unwrap()
                .unwrap()
        } else {
            crdt::insert(
                p.child_section(),
                &TextDraft::default(),
                p.session_id(),
                100,
            )
            .unwrap()
        };
        let mut doc = Automerge::load(seed.bytes(5).unwrap().unwrap())
            .unwrap()
            .with_actor(format!("{}00000000", p.session_id()).parse().unwrap());
        let (_, object) = doc.get(automerge::ROOT, "text").unwrap().unwrap();
        for draft in p.child_updates() {
            let before = doc.get_heads();
            let mut scratch = doc.clone();
            let mut tx = scratch.transaction();
            tx.update_text(&object, format!("{}\u{feff}", draft.model().unwrap().text))
                .unwrap();
            tx.commit_with(CommitOptions::default().with_time(0));
            // Reproduce the executed defect: history diff instead of the live
            // transaction log. Each real change still has the correct actor.
            for patch in scratch.diff(&before, &scratch.get_heads()) {
                let (index, count, value) = match patch.action {
                    PatchAction::SpliceText { index, value, .. } => (index, 0, value.make_string()),
                    PatchAction::DeleteSeq { index, length } => {
                        (index, length as isize, String::new())
                    }
                    _ => panic!("plain"),
                };
                let mut tx = doc.transaction();
                tx.splice_text(&object, index, count, &value).unwrap();
                tx.commit_with(CommitOptions::default().with_time(0));
            }
        }
        text::crdt::check_document_state(&doc, p.child_draft(), false).unwrap();
        let old_head = p.child_wire.string(12).unwrap().unwrap();
        let head = doc.get_heads()[0].to_string();
        let processed = p
            .child_wire
            .fields()
            .iter()
            .filter(|f| f.tag() == 6)
            .map(|f| {
                let raw = RawMessage::decode(p.child_wire.encoded_field(f).unwrap()).unwrap();
                let old = raw.string(6).unwrap().unwrap();
                t(6, if old == old_head { &head } else { old })
            })
            .collect::<Vec<_>>();
        let wire = p
            .child_wire
            .replace_fields(5, &[F::bytes(5, &doc.save()).unwrap()])
            .unwrap()
            .replace_fields(6, &processed)
            .unwrap()
            .replace_fields(12, &[t(12, &head)])
            .unwrap();
        let mut rows = p.data.messages(1).unwrap();
        rows[1] = rows[1].replace_fields(38, &[m(38, &wire)]).unwrap();
        let request = p
            .data
            .replace_fields(1, &rows.iter().map(|r| m(1, r)).collect::<Vec<_>>())
            .unwrap();
        let checkpoint = p.checkpoint().unwrap();
        let mut json = serde_json::to_value(&checkpoint).unwrap();
        json["request"] = serde_json::to_value(request.raw()).unwrap();
        let bad: Checkpoint = serde_json::from_value(json).unwrap();
        let store = Store::new();
        assert!(store.open().begin(bad).is_err());
        assert!(store.open().get(checkpoint.scope()).unwrap().is_none());
    }
}
