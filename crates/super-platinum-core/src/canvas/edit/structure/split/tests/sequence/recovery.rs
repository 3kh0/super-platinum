use super::*;
use crate::canvas::edit::structure::recovery::{
    Checkpoint,
    journal::{Begin, Journal, LocalDecision},
};
use crate::canvas::edit::{EditSession, text::TextIntent};
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("second-middle-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, "00000000-0000-4000-8000-000000000818").unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[test]
fn v18_restart_review_custody_and_tampered_two_boundary_records() {
    let e = vectors();
    for index in [1, 3] {
        let (s, p) = attempt(&e["vectors"][index]);
        let c = p.checkpoint().unwrap();
        let original = serde_json::to_value(&c).unwrap();
        let store = Store::new();
        let receipt = match store
            .open()
            .begin(serde_json::from_value(original.clone()).unwrap())
            .unwrap()
        {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let mut j = store.open();
        let restored = j.get(c.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        let observed = readback(&s, &pair(&p, &s)[..1]);
        let review = j.local_review(&restored, &observed).unwrap();
        let split = review.split().unwrap();
        assert_eq!(split.member_ids(), p.member_ids());
        assert_eq!(split.visual_order(), p.visual_order());
        assert_eq!(split.member_texts().len(), 3);
        assert_eq!(split.parent_text(), "");
        assert!(split.second_enter_text().is_some());
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        conn.execute_batch("CREATE TRIGGER deny_retire BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(
            j.remove_local_lock(
                &review,
                &observed,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
        );
        assert!(j.get(c.scope()).unwrap().is_some());
        conn.execute_batch("DROP TRIGGER deny_retire;").unwrap();
        assert!(
            j.remove_local_lock(
                &review,
                &observed,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                124
            )
            .unwrap()
        );
        assert!(j.has_local_review(&restored).unwrap());
        assert!(j.get(c.scope()).unwrap().is_none());
        for kind in 0..8 {
            let mut bad = original.clone();
            match kind {
                0 => bad["version"] = 17.into(),
                1 => bad["action"]["SplitSequence"]["second_offset_utf16"] = 0.into(),
                2 => {
                    bad["action"]["SplitSequence"]["second_enter_draft"] =
                        "same-looking forgery".into()
                }
                3 => {
                    bad["action"]["SplitSequence"]["second_target"] =
                        if index == 1 { "FirstChild" } else { "Original" }.into()
                }
                4 => {
                    bad["action"]["SplitSequence"]["second_child"] =
                        bad["action"]["SplitSequence"]["child"].clone()
                }
                5 => bad["action"]["SplitSequence"]["final_updates"][0] = serde_json::json!([]),
                6 => bad["action"]["SplitSequence"]["parent_updates"] = serde_json::json!([]),
                _ => bad["action"]["SplitSequence"]["second_position"] = "zzzz:temp".into(),
            }
            if bad == original {
                continue;
            }
            let decoded: Checkpoint = serde_json::from_value(bad).unwrap();
            assert!(
                Store::new().open().begin(decoded).is_err(),
                "{index}/{kind}"
            );
        }
    }
}
fn save_text(s: &mut CanvasCollabSnapshot, id: &str, value: &str) {
    let mut session = EditSession::new();
    let intent = TextIntent::capture(s, id, 100).unwrap();
    let p = intent.prepare(s, &mut session, &draft(value), 100).unwrap();
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
fn both_second_targets_allow_later_edits_of_all_three_with_exact_related_row_guards() {
    let e = vectors();
    for v in e["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["cadence"] == "edited")
    {
        let (s, p) = attempt(v);
        let initial = readback(&s, &pair(&p, &s));
        let ids = p.member_ids();
        let infos = ids
            .iter()
            .map(|id| {
                initial
                    .section(id)
                    .unwrap()
                    .crdt()
                    .unwrap()
                    .unwrap()
                    .messages(10)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for order in [[0, 1, 2], [2, 1, 0]] {
            let mut current = initial.clone();
            for i in order {
                save_text(&mut current, ids[i], &format!("Row {i} continued"));
            }
            for (i, id) in ids.iter().enumerate() {
                assert_eq!(
                    current
                        .section(id)
                        .unwrap()
                        .crdt()
                        .unwrap()
                        .unwrap()
                        .messages(10)
                        .unwrap(),
                    infos[i]
                );
                assert!(TextIntent::capture(&current, id, 100).is_ok());
            }
        }
        let intent = TextIntent::capture(&initial, ids[2], 100).unwrap();
        let mut stale = initial.clone();
        let index = stale
            .sections
            .iter()
            .position(|s| s.id() == ids[0])
            .unwrap();
        stale.sections[index] = change(&stale.sections[index], n(2, 9999));
        assert!(
            intent
                .prepare(&stale, &mut EditSession::new(), &draft("change"), 100)
                .is_err()
        );
    }
}
