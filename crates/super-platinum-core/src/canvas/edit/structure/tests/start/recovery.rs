use super::*;
use crate::canvas::edit::structure::recovery::journal::{Begin, Journal, LocalDecision};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000975";
struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("canvas-start-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn start_v1_empty_insert_survives_restart_and_local_review_stays_unknown() {
    for s in sources() {
        let store = Store::new();
        let p = start(&s);
        let checkpoint = p.checkpoint().unwrap();
        let original_json = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(original_json["version"], 1);
        assert_eq!(original_json["draft"], "");
        let receipt = match store.open().begin(checkpoint.clone()).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh journal"),
        };
        let mut j = store.open();
        let restored = j.get(checkpoint.scope()).unwrap().unwrap();
        assert!(restored == receipt);
        assert_eq!(
            serde_json::to_value(restored.checkpoint()).unwrap(),
            original_json
        );
        assert!(matches!(
            j.begin(start(&s).checkpoint().unwrap()).unwrap(),
            Begin::Blocked(_)
        ));
        let review = j.local_review(&restored, &s).unwrap();
        assert_eq!(review.draft_text(), "");
        assert!(review.insertion_anchor() == Some(("item", Side::Before)));
        assert!(!review.is_split() && !review.is_end() && !review.is_delete());
        assert_eq!(
            review.observed_sections().find(|r| r.id() == "item"),
            s.section("item")
        );
        assert_eq!(j.local_review_count(checkpoint.scope()).unwrap(), 0);
        assert!(j.get(checkpoint.scope()).unwrap().is_some());
        let drift = readback(&s, &changed(s.section("item").unwrap(), n(1002, 1)));
        assert!(
            j.remove_local_lock(
                &review,
                &drift,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .is_err()
        );
        assert!(j.get(checkpoint.scope()).unwrap().is_some());
        assert!(
            j.remove_local_lock(
                &review,
                &s,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                124
            )
            .unwrap()
        );
        drop(j);
        let reopened = store.open();
        assert!(reopened.get(checkpoint.scope()).unwrap().is_none());
        assert!(reopened.has_local_review(&receipt).unwrap());
        assert_eq!(reopened.local_review_count(checkpoint.scope()).unwrap(), 1);
        let conn = rusqlite::Connection::open(&store.0).unwrap();
        let (body, decision): (String, String) = conn
            .query_row("SELECT body,decision FROM local_reviews", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            original_json
        );
        assert_eq!(decision, "local_unlock_outcome_unknown");
    }
}

#[test]
fn insertion_review_placement_is_inert_and_current_anchor_is_not_original_text() {
    let s = sources().remove(0);
    let split = super::super::super::SplitIntent::capture(&s, "item", 100)
        .unwrap()
        .prepare(
            &s,
            &super::super::super::SplitWitness::new(
                &TextDraft::from_rtml("Keep thisSecond paragraph").unwrap(),
                9,
                &TextDraft::from_rtml("Keep this").unwrap(),
                &TextDraft::from_rtml("Second paragraph").unwrap(),
            )
            .unwrap(),
            100,
        )
        .unwrap();
    let after = StructureIntent::insert(&s, "item", Side::After, &TextDraft::default(), 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap();
    let delete = StructureIntent::delete(&s, "item", 100)
        .unwrap()
        .prepare(&s, 100)
        .unwrap();
    for (checkpoint, expected) in [
        (after.checkpoint().unwrap(), Some(("item", Side::After))),
        (delete.checkpoint().unwrap(), None),
        (split.checkpoint().unwrap(), None),
    ] {
        let store = Store::new();
        let mut j = store.open();
        let receipt = match j.begin(checkpoint.clone()).unwrap() {
            Begin::Started(r) => r,
            _ => panic!("fresh"),
        };
        let review = j.local_review(&receipt, &s).unwrap();
        assert!(review.insertion_anchor() == expected);
        let drift = readback(
            &s,
            &changed(
                s.section("item").unwrap(),
                m(
                    12,
                    &new_content(&TextDraft::from_rtml("Changed current text").unwrap()).unwrap(),
                ),
            ),
        );
        assert!(review.revalidate(&drift).is_err());
        let current = j.local_review(&receipt, &drift).unwrap();
        assert!(current.insertion_anchor() == expected);
        assert_eq!(
            text::canonical_draft(
                current
                    .observed_sections()
                    .find(|r| r.id() == "item")
                    .unwrap()
            )
            .unwrap()
            .rtml()
            .unwrap(),
            "Changed current text"
        );
        assert_eq!(
            serde_json::to_value(receipt.checkpoint()).unwrap(),
            serde_json::to_value(&checkpoint).unwrap()
        );
    }
}
