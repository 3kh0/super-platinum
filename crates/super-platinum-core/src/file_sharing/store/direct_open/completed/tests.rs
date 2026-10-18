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

fn raw() -> Value {
    json!({"ok":true,"channel":{"id":"D00000001"},"future":{"keep":37}})
}
#[test]
fn delayed_second_handle_joins_exact_completed_history_for_more_than_capacity() {
    let mut f = Fixture::new();
    let mut delayed = Journal::open(
        f.dir.join("private.sqlite"),
        "4b02f26e-393b-4fd1-a7da-5d11f76296ae",
    )
    .unwrap();
    for n in 1..=LIMIT * 3 {
        let (record, draft) = f.start(n);
        let resolved = f.resolve(&record, &draft, false);
        let receipt = f.send(&resolved);
        f.finish(&receipt, json!({"ok":true})).unwrap();
        assert_eq!(f.count(), 0);
        assert!(matches!(
            delayed.settle_direct_returned(&record, raw()).unwrap(),
            Returned::Completed
        ));
        let history = get(&f.journal.conn, record.id()).unwrap().unwrap();
        assert!(history.open.draft == draft);
        assert!(history.open.returned.as_ref() == Some(&raw()));
        assert!(history.receipt == f.journal.get(&receipt.scope).unwrap().unwrap());
        assert!(matches!(
            delayed.settle_direct_returned(&record, raw()).unwrap(),
            Returned::Completed
        ));
    }
    drop(delayed);
}
#[test]
fn completed_response_cannot_rebind_foreign_original_or_replace_new_peer_attempt() {
    let mut f = Fixture::new();
    let (record, draft) = f.start(1);
    let resolved = f.resolve(&record, &draft, false);
    let receipt = f.send(&resolved);
    f.finish(&receipt, json!({"ok":true})).unwrap();
    let (new_record, _) = f.start(1);
    assert!(new_record.id() != record.id());
    let mut wrong = record.clone();
    wrong.id = uuid::Uuid::new_v4().to_string();
    assert!(f.journal.settle_direct_returned(&wrong, raw()).is_err());
    let mut wrong = record.clone();
    wrong.draft.content.source["future"] = json!("changed");
    assert!(f.journal.settle_direct_returned(&wrong, raw()).is_err());
    assert!(
        f.journal
            .settle_direct_returned(&record, json!({"ok":true,"channel":{"id":"D00000002"}}))
            .is_err()
    );
    assert!(
        f.journal
            .settle_direct_returned(
                &record,
                json!({"ok":true,"channel":{"id":"D00000001","user":"U99999999"}})
            )
            .is_err()
    );
    assert!(matches!(
        f.journal.settle_direct_returned(&record, raw()).unwrap(),
        Returned::Completed
    ));
    assert!(
        super::super::get(
            &f.journal.conn,
            &draft.scope.team,
            &draft.scope.user,
            record.peer()
        )
        .unwrap()
        .as_ref()
            == Some(&new_record)
    );
    let mut changed = raw();
    changed["future"] = json!({"changed":true});
    assert!(f.journal.settle_direct_returned(&record, changed).is_err());
}
#[test]
fn actual_completion_write_failure_preserves_exact_retry_and_missing_history_refuses() {
    let mut f = Fixture::new();
    let (record, draft) = f.start(1);
    let resolved = f.resolve(&record, &draft, false);
    let receipt = f.send(&resolved);
    f.finish(&receipt, json!({"ok":true})).unwrap();
    f.journal.conn.execute_batch("CREATE TEMP TRIGGER completed_fault BEFORE UPDATE ON file_share_direct_completed BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END").unwrap();
    assert!(f.journal.settle_direct_returned(&record, raw()).is_err());
    assert!(
        get(&f.journal.conn, record.id())
            .unwrap()
            .unwrap()
            .open
            .returned
            .is_none()
    );
    f.journal
        .conn
        .execute_batch("DROP TRIGGER completed_fault")
        .unwrap();
    assert!(matches!(
        f.journal.settle_direct_returned(&record, raw()).unwrap(),
        Returned::Completed
    ));
    for n in 2..=super::super::super::MAX_TERMINAL + 2 {
        let (r, d) = f.start(n);
        let d = f.resolve(&r, &d, false);
        let receipt = f.send(&d);
        f.journal
            .finish_result(
                &receipt.scope,
                &receipt.attempt_id,
                Ok(json!({"ok":true})),
                n as u64 + 100,
            )
            .unwrap();
    }
    let count: i64 = f
        .journal
        .conn
        .query_row(
            "SELECT COUNT(*) FROM file_share_direct_completed",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(count <= super::super::super::MAX_TERMINAL as i64);
    assert!(f.journal.get(&receipt.scope).unwrap().is_none());
    assert!(get(&f.journal.conn, record.id()).unwrap().is_none());
    assert!(f.journal.settle_direct_returned(&record, raw()).is_err());
}
#[test]
fn missing_completed_schema_at_current_version_is_not_silently_recreated() {
    let f = Fixture::new();
    f.journal
        .conn
        .execute_batch("DROP TABLE file_share_direct_completed")
        .unwrap();
    assert!(
        Journal::open(
            f.dir.join("private.sqlite"),
            "4b02f26e-393b-4fd1-a7da-5d11f76296ae"
        )
        .is_err()
    );
}
