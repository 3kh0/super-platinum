use super::*;
use crate::{
    canvas::comments::archive::ArchiveIntent,
    slack::models::{File, Message},
};
use serde_json::json;
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "canvas-comment-test-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn record() -> ArchiveRecord {
    let file: File =
        serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"write"})).unwrap();
    let root:Message=serde_json::from_value(json!({"type":"message","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"thread-1"}})).unwrap();
    ArchiveIntent::capture(&file, &root)
        .unwrap()
        .record("T1", "U1")
        .unwrap()
}
#[test]
fn reopen_restores_inert_receipt_and_blocks_replay_and_structural_admission() {
    let temporary = Temp::new();
    let path = temporary.0.clone();
    let account = uuid::Uuid::new_v4().to_string();
    let receipt = {
        let mut journal = Journal::open(&path, &account).unwrap();
        match journal.begin_comment(record()).unwrap() {
            CommentBegin::Started(receipt) => receipt,
            _ => panic!("fresh"),
        }
    };
    let mut reopened = Journal::open(&path, &account).unwrap();
    let found = reopened.comment(&scope(receipt.record())).unwrap().unwrap();
    assert!(found == receipt);
    assert!(matches!(
        reopened.begin_comment(record()).unwrap(),
        CommentBegin::Blocked(_)
    ));
    assert!(reopened.get(&scope(receipt.record())).is_err());
    assert!(reopened.complete_comment(&receipt).unwrap());
    assert!(!reopened.complete_comment(&receipt).unwrap());
    assert!(
        reopened
            .comment(&scope(receipt.record()))
            .unwrap()
            .is_none()
    );
    let newer = match reopened.begin_comment(record()).unwrap() {
        CommentBegin::Started(receipt) => receipt,
        _ => panic!("retired"),
    };
    assert!(!reopened.complete_comment(&receipt).unwrap());
    assert!(reopened.comment(&scope(newer.record())).unwrap().as_ref() == Some(&newer));
}
#[test]
fn older_schema_upgrades_without_reinterpreting_structural_rows() {
    let temporary = Temp::new();
    let path = temporary.0.clone();
    let account = uuid::Uuid::new_v4().to_string();
    let journal = Journal::open(&path, &account).unwrap();
    journal
        .conn
        .execute_batch("DROP TABLE comment_receipts; PRAGMA user_version=2;")
        .unwrap();
    drop(journal);
    let journal = Journal::open(&path, &account).unwrap();
    assert!(journal.comment(&scope(&record())).unwrap().is_none());
    assert!(Journal::open(&path, &uuid::Uuid::new_v4().to_string()).is_err());
}
#[test]
fn corrupt_comment_identity_is_not_an_empty_slot() {
    let temporary = Temp::new();
    let path = temporary.0.clone();
    let account = uuid::Uuid::new_v4().to_string();
    let mut journal = Journal::open(&path, &account).unwrap();
    journal.begin_comment(record()).unwrap();
    journal
        .conn
        .execute("UPDATE comment_receipts SET body='{}'", [])
        .unwrap();
    assert!(journal.comment(&scope(&record())).is_err());
    assert!(journal.begin_comment(record()).is_err());
}

#[test]
fn explicit_unknown_review_retains_exact_evidence_and_cannot_unlock_a_new_attempt() {
    let temporary = Temp::new();
    let account = uuid::Uuid::new_v4().to_string();
    let mut journal = Journal::open(&temporary.0, &account).unwrap();
    let receipt = match journal.begin_comment(record()).unwrap() {
        CommentBegin::Started(value) => value,
        _ => panic!("fresh"),
    };
    let file: File =
        serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"write"})).unwrap();
    let root:Message=serde_json::from_value(json!({"type":"message","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"thread-1","is_archived":true}})).unwrap();
    let review = journal.review_comment(&receipt, &file, &root).unwrap();
    let stale = journal.review_comment(&receipt, &file, &root).unwrap();
    assert!(
        journal
            .resolve_comment_locally(review, CommentDecision::RemoveLocalLockOutcomeUnknown)
            .unwrap()
    );
    assert!(journal.comment(&scope(receipt.record())).unwrap().is_none());
    assert_eq!(
        journal
            .comment_review_count(&scope(receipt.record()))
            .unwrap(),
        1
    );
    let (id, body, observed, decision): (String, String, String, String) = journal
        .conn
        .query_row(
            "SELECT operation,body,observed,decision FROM comment_reviews",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(id, receipt.operation_id());
    assert_eq!(body, receipt.body);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&observed).unwrap(),
        serde_json::to_value(&root).unwrap()
    );
    assert_eq!(decision, "local_unlock_outcome_unknown");
    let next = match journal.begin_comment(record()).unwrap() {
        CommentBegin::Started(value) => value,
        _ => panic!("explicit new attempt"),
    };
    assert!(
        !journal
            .resolve_comment_locally(stale, CommentDecision::RemoveLocalLockOutcomeUnknown)
            .unwrap()
    );
    assert!(!journal.complete_comment(&receipt).unwrap());
    assert!(journal.comment(&scope(next.record())).unwrap().as_ref() == Some(&next));
    drop(journal);
    let reopened = Journal::open(&temporary.0, &account).unwrap();
    assert_eq!(
        reopened
            .comment_review_count(&scope(next.record()))
            .unwrap(),
        1
    );
    assert!(reopened.comment(&scope(next.record())).unwrap().as_ref() == Some(&next));
}
#[test]
fn review_refuses_foreign_thread_and_permission_change_without_erasing_lock() {
    let temporary = Temp::new();
    let account = uuid::Uuid::new_v4().to_string();
    let mut journal = Journal::open(&temporary.0, &account).unwrap();
    let receipt = match journal.begin_comment(record()).unwrap() {
        CommentBegin::Started(value) => value,
        _ => panic!("fresh"),
    };
    let file: File =
        serde_json::from_value(json!({"id":"F123","filetype":"quip","access":"read"})).unwrap();
    let root:Message=serde_json::from_value(json!({"type":"message","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"thread-1"}})).unwrap();
    assert!(journal.review_comment(&receipt, &file, &root).is_err());
    let mut file = file;
    file.extra.insert("access".into(), json!("write"));
    let mut root = root;
    root.extra
        .insert("document_comment".into(), json!({"thread_id":"foreign"}));
    assert!(journal.review_comment(&receipt, &file, &root).is_err());
    assert!(journal.comment(&scope(receipt.record())).unwrap().as_ref() == Some(&receipt));
}
