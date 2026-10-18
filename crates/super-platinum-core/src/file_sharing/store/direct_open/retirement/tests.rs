use super::*;
use serde_json::json;

struct Fixture {
    dir: std::path::PathBuf,
    journal: Journal,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("sp-direct-retire-{}", uuid::Uuid::new_v4()));
        let journal = Journal::open(
            dir.join("private.sqlite"),
            "4b02f26e-393b-4fd1-a7da-5d11f76296ae",
        )
        .unwrap();
        Self { dir, journal }
    }
    fn start(&mut self, n: usize) -> (Record, drafts::Draft) {
        let peer = format!("U{n:08}");
        let scope = drafts::DraftScope {
            team: "T00000001".into(),
            user: "U99999999".into(),
            file: format!("F{n:08}"),
        };
        let content = drafts::Content {
            source: json!({"id":scope.file,"mode":"hosted","is_external":false,"future":{"keep":n}}),
            title: "Original".into(),
            note: vec![],
            destination: None,
        };
        let drafts::Claim::Claimed(draft) = self.journal.claim_draft(scope, None, content).unwrap()
        else {
            panic!("claim")
        };
        let mut content = draft.content.clone();
        content.destination = Some(drafts::Destination {
            id: format!("person:{peer}"),
            name: "Person".into(),
            private: true,
        });
        content.note = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original 日本語 e\u{301}"}]}]}),
        ];
        let drafts::Mutation::Applied(draft) = self.journal.save_draft(&draft, content).unwrap()
        else {
            panic!("save")
        };
        let Begin::Created(record) = self.journal.begin_direct_open(&draft, json!({"ok":true,"results":[{"id":peer,"deleted":false,"is_bot":false}],"failed_ids":[]}), 1).unwrap() else { panic!("begin") };
        (record, draft)
    }
    fn resolve(&mut self, record: &Record, draft: &drafts::Draft, returned: bool) -> drafts::Draft {
        if returned {
            self.journal
                .direct_open_returned(record, json!({"ok":true,"channel":{"id":"D00000001"}}))
                .unwrap();
        } else {
            self.journal.resolve_direct_open(record,json!({"ok":true,"channel":{"id":"D00000001","user":record.peer,"is_im":true,"is_open":true,"is_ext_shared":false}})).unwrap();
        }
        let mut content = draft.content.clone();
        content.destination = Some(drafts::Destination {
            id: "D00000001".into(),
            name: "Person".into(),
            private: true,
        });
        let drafts::Mutation::Applied(draft) = self.journal.save_draft(draft, content).unwrap()
        else {
            panic!("resolved draft")
        };
        draft
    }
    fn send(&mut self, draft: &drafts::Draft) -> Receipt {
        let super::super::super::Begin::Started(receipt) =
            self.journal.begin_from_draft(draft, 2).unwrap()
        else {
            panic!("share begin")
        };
        receipt
    }
    fn finish(&mut self, receipt: &Receipt, raw: Value) -> Result<Option<Receipt>, AppError> {
        self.journal
            .finish_result(&receipt.scope, &receipt.attempt_id, Ok(raw), 3)
    }
    fn count(&self) -> i64 {
        self.journal
            .conn
            .query_row("SELECT COUNT(*) FROM file_share_direct_opens", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(std::mem::replace(
            &mut self.journal.conn,
            Connection::open_in_memory().unwrap(),
        ));
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn more_than_lifetime_capacity_complete_with_exact_returned_or_readonly_recovery() {
    let mut f = Fixture::new();
    for n in 1..=LIMIT * 3 {
        let (record, draft) = f.start(n);
        let resolved = f.resolve(&record, &draft, n % 2 == 0);
        let receipt = f.send(&resolved);
        f.finish(&receipt, json!({"ok":true})).unwrap();
        assert_eq!(f.count(), 0);
        assert!(f.journal.draft(&draft.scope).unwrap().is_none());
        assert!(matches!(
            f.journal.get(&receipt.scope).unwrap().unwrap().state(),
            State::Server(FileShareDisposition::Accepted { .. })
        ));
    }
}

#[test]
fn unresolved_share_and_changed_original_note_never_retire_open_custody() {
    let mut f = Fixture::new();
    let (record, draft) = f.start(1);
    let resolved = f.resolve(&record, &draft, true);
    let receipt = f.send(&resolved);
    f.journal
        .finish_result(
            &receipt.scope,
            &receipt.attempt_id,
            Err(crate::slack::Error::Transport("fixture".into())),
            3,
        )
        .unwrap();
    assert_eq!(f.count(), 1);
    assert!(
        f.journal
            .direct_opens(&draft.scope.team, &draft.scope.user)
            .unwrap()[0]
            .draft()
            == &draft
    );
    let (record2, draft2) = f.start(2);
    let resolved = f.resolve(&record2, &draft2, false);
    let mut content = resolved.content.clone();
    content.note.clear();
    let drafts::Mutation::Applied(changed) = f.journal.save_draft(&resolved, content).unwrap()
    else {
        panic!("change")
    };
    let receipt = f.send(&changed);
    f.finish(&receipt, json!({"ok":true})).unwrap();
    assert_eq!(f.count(), 2);
}

#[test]
fn failed_retirement_rolls_back_accepted_receipt_and_draft_then_exact_retry_succeeds() {
    let mut f = Fixture::new();
    let (record, draft) = f.start(1);
    let resolved = f.resolve(&record, &draft, true);
    let receipt = f.send(&resolved);
    f.journal.conn.execute_batch("CREATE TEMP TRIGGER retire_fault BEFORE DELETE ON file_share_direct_opens BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END").unwrap();
    assert!(f.finish(&receipt, json!({"ok":true})).is_err());
    assert_eq!(f.count(), 1);
    assert!(f.journal.get(&receipt.scope).unwrap().as_ref() == Some(&receipt));
    assert!(f.journal.draft(&resolved.scope).unwrap().as_ref() == Some(&resolved));
    f.journal
        .conn
        .execute_batch("DROP TRIGGER retire_fault")
        .unwrap();
    f.journal
        .retry_result(&receipt, Ok(json!({"ok":true})), 3)
        .unwrap();
    assert_eq!(f.count(), 0);
    assert!(f.journal.draft(&resolved.scope).unwrap().is_none());
}

#[test]
fn malformed_other_record_aborts_entire_retirement_transaction() {
    let mut f = Fixture::new();
    let (record, draft) = f.start(1);
    let resolved = f.resolve(&record, &draft, true);
    let receipt = f.send(&resolved);
    let (_other, _) = f.start(2);
    f.journal
        .conn
        .execute(
            "UPDATE file_share_direct_opens SET body='{}' WHERE peer='U00000002'",
            [],
        )
        .unwrap();
    assert!(f.finish(&receipt, json!({"ok":true})).is_err());
    assert_eq!(f.count(), 2);
    assert!(f.journal.get(&receipt.scope).unwrap().as_ref() == Some(&receipt));
}
