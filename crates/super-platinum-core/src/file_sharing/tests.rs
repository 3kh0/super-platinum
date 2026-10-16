use super::{store::*, *};
use serde_json::json;
use std::path::PathBuf;

const OWNER: &str = "4b02f26e-393b-4fd1-a7da-5d11f76296ae";
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("sp-file-share-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("receipt.sqlite")
    }
    fn journal(&self) -> Journal {
        Journal::open(self.path(), OWNER).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scope(n: usize) -> Scope {
    Scope {
        team: "T12345678".into(),
        user: "U12345678".into(),
        file: format!("FFILE{n}"),
        destination: "C12345678".into(),
    }
}
fn note() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","future":{"a":false},"elements":[{"type":"rich_text_section","section_unknown":7,"elements":[{"type":"text","text":"\u{65e5}\u{672c}\u{8a9e} \u{1f98a} e\u{301}","leaf_unknown":"kept","style":{"bold":true,"future":19}}]}]}),
    ]
}
fn intent(s: &Scope) -> Intent {
    Intent::new(s.file.clone(), s.destination.clone(), Some(note()), None).unwrap()
}
fn begin(j: &mut Journal, s: Scope) -> Receipt {
    match j
        .begin(
            s.clone(),
            intent(&s),
            "File".into(),
            "Destination".into(),
            10,
        )
        .unwrap()
    {
        Begin::Started(r) => r,
        _ => panic!("Expected new admission"),
    }
}
fn finish(j: &mut Journal, r: &Receipt, result: Result<Value, Error>) -> Receipt {
    j.finish_result(&r.scope, &r.attempt_id, result, 20)
        .unwrap()
        .unwrap()
}

#[test]
fn committed_note_reopens_exactly_and_blocked_note_cannot_replace_it() {
    let temp = Temp::new();
    let r = {
        let mut j = temp.journal();
        begin(&mut j, scope(0))
    };
    let mut j = temp.journal();
    let loaded = j.get(&r.scope).unwrap().unwrap();
    assert!(loaded == r);
    assert!(loaded.unresolved());
    assert_eq!(loaded.intent.note(), Some(note().as_slice()));
    let other = Intent::new(
        r.scope.file.clone(),
        r.scope.destination.clone(),
        None,
        None,
    )
    .unwrap();
    match j
        .begin(
            r.scope.clone(),
            other,
            "New name".into(),
            "Other label".into(),
            30,
        )
        .unwrap()
    {
        Begin::Blocked(old) => assert!(old == r),
        _ => panic!("Unresolved note was replaced"),
    }
    assert!(j.unresolved(&r.scope.team, &r.scope.user).unwrap() == vec![r]);
}

#[test]
fn two_connections_admit_exactly_one_same_scope_write() {
    let temp = Temp::new();
    drop(temp.journal());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let joins = (0..2)
        .map(|_| {
            let path = temp.path();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut j = Journal::open(path, OWNER).unwrap();
                barrier.wait();
                let s = scope(0);
                matches!(
                    j.begin(s.clone(), intent(&s), "File".into(), "Dest".into(), 10)
                        .unwrap(),
                    Begin::Started(_)
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        joins
            .into_iter()
            .map(|j| usize::from(j.join().unwrap()))
            .sum::<usize>(),
        1
    );
}

#[test]
fn acceptance_is_server_acceptance_even_without_navigation_timestamp() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"ts":"bad"}),
        json!({"ok":true,"file_msg_ts":"123.000001"}),
    ] {
        let temp = Temp::new();
        let mut j = temp.journal();
        let r = begin(&mut j, scope(0));
        let saved = finish(&mut j, &r, Ok(raw.clone()));
        assert!(!saved.unresolved());
        assert_eq!(saved.raw_response(), Some(&raw));
        assert!(matches!(
            saved.state(),
            State::Server(FileShareDisposition::Accepted { .. })
        ));
        assert!(
            j.unresolved(&r.scope.team, &r.scope.user)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn only_genuine_known_unsent_error_releases_the_scope() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let a = begin(&mut j, scope(0));
    let saved = finish(
        &mut j,
        &a,
        Err(Error::Api("draft_mutation_not_sent_offline".into())),
    );
    assert_eq!(saved.state(), State::NotSent(NotSent::Offline));
    assert!(!saved.unresolved());
    let b = begin(&mut j, scope(1));
    let raw = json!({"ok":false,"error":"draft_mutation_not_sent_offline"});
    let saved = finish(&mut j, &b, Ok(raw.clone()));
    assert!(saved.unresolved());
    assert_eq!(saved.raw_response(), Some(&raw));
}

#[test]
fn warning_processing_policy_and_unknown_keep_the_note_and_no_resend_admission() {
    let examples = [
        json!({"ok":false,"error":"dlp_user_warning","dlp_custom_message":"Review \u{65e5}\u{672c}\u{8a9e}","opaque":true}),
        json!({"ok":false,"error":"dlp_processing"}),
        json!({"ok":false,"error":"posting_to_channel_denied"}),
        json!({"ok":false,"error":"new_server_error"}),
        json!({"ok":true,"error":"contradiction"}),
    ];
    for raw in examples {
        let temp = Temp::new();
        let mut j = temp.journal();
        let r = begin(&mut j, scope(0));
        let saved = finish(&mut j, &r, Ok(raw.clone()));
        assert!(saved.unresolved());
        assert_eq!(saved.raw_response(), Some(&raw));
        assert_eq!(saved.intent.note(), r.intent.note());
        assert!(matches!(
            j.begin(
                r.scope.clone(),
                r.intent.clone(),
                "File".into(),
                "Dest".into(),
                100
            )
            .unwrap(),
            Begin::Blocked(_)
        ));
    }
}

#[test]
fn rate_limit_survives_reopen_and_blocks_other_files_and_destinations() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    finish(
        &mut j,
        &r,
        Err(Error::RateLimited {
            retry_after_secs: Some(90),
        }),
    );
    drop(j);
    let mut j = temp.journal();
    assert_eq!(j.retry_at(&r.scope.team, &r.scope.user).unwrap(), 110);
    let mut s = scope(1);
    s.destination = "G12345678".into();
    assert!(matches!(
        j.begin(s.clone(), intent(&s), "File".into(), "Dest".into(), 109)
            .unwrap(),
        Begin::CoolingDown { retry_at: 110 }
    ));
    s.user = "UOTHER123".into();
    assert!(matches!(
        j.begin(s.clone(), intent(&s), "File".into(), "Dest".into(), 109)
            .unwrap(),
        Begin::Started(_)
    ));
    assert!(j.get(&r.scope).unwrap().unwrap().unresolved());
}

#[test]
fn older_callback_or_duplicate_cannot_downgrade_or_replace_an_attempt() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let old = begin(&mut j, scope(0));
    let accepted = finish(&mut j, &old, Ok(json!({"ok":true})));
    assert!(finish(&mut j, &old, Err(Error::Transport("secret URL".into()))) == accepted);
    let newer = match j
        .begin(
            old.scope.clone(),
            old.intent.clone(),
            "File".into(),
            "Dest".into(),
            30,
        )
        .unwrap()
    {
        Begin::Started(r) => r,
        _ => panic!(),
    };
    assert_ne!(old.attempt_id, newer.attempt_id);
    assert!(
        j.finish_result(&old.scope, &old.attempt_id, Ok(json!({"ok":true})), 40)
            .unwrap()
            .is_none()
    );
    assert!(j.get(&newer.scope).unwrap().unwrap() == newer);
}

#[test]
fn canceled_admission_is_explicitly_not_sent_but_crash_window_stays_uncertain() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let canceled = j
        .cancel_before_dispatch(&r.scope, &r.attempt_id, 11)
        .unwrap()
        .unwrap();
    assert_eq!(canceled.state(), State::NotSent(NotSent::AdmissionCanceled));
    let pending = begin(&mut j, scope(1));
    drop(j);
    assert_eq!(
        temp.journal().get(&pending.scope).unwrap().unwrap().state(),
        State::Unconfirmed
    );
}

#[test]
fn response_limit_and_transport_error_do_not_store_unbounded_or_secret_error_text() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let saved = finish(
        &mut j,
        &r,
        Ok(json!({"ok":true,"large":"x".repeat(FILE_SHARE_RESPONSE_BYTES)})),
    );
    assert!(saved.raw_response().is_none());
    assert!(saved.unresolved());
    assert_eq!(
        saved.state(),
        State::Server(FileShareDisposition::Uncertain(
            FileShareUncertainty::ResponseLimit
        ))
    );
    let r = begin(&mut j, scope(1));
    let saved = finish(
        &mut j,
        &r,
        Err(Error::Transport("secret signed URL".into())),
    );
    assert!(
        !serde_json::to_string(&saved)
            .unwrap()
            .contains("secret signed URL")
    );
    assert_eq!(saved.state(), State::Unconfirmed);
}

#[test]
fn mismatched_database_account_and_mismatched_intent_are_refused() {
    let temp = Temp::new();
    let mut j = temp.journal();
    assert!(Journal::open(temp.path(), "2a261d84-a9b7-4a48-9152-3b9cd03374ca").is_err());
    assert!(
        j.begin(
            scope(0),
            intent(&scope(1)),
            "File".into(),
            "Dest".into(),
            10
        )
        .is_err()
    );
    assert!(j.unresolved("T12345678", "U12345678").unwrap().is_empty());
}

#[test]
fn unresolved_capacity_never_evicts_pending_notes_and_terminal_history_is_bounded() {
    let temp = Temp::new();
    let mut j = temp.journal();
    for n in 0..MAX_UNRESOLVED {
        begin(&mut j, scope(n));
    }
    let s = scope(1000);
    assert!(matches!(
        j.begin(s.clone(), intent(&s), "File".into(), "Dest".into(), 10)
            .unwrap(),
        Begin::Capacity
    ));
    assert_eq!(
        j.unresolved(&s.team, &s.user).unwrap().len(),
        MAX_UNRESOLVED
    );
    for n in 0..MAX_UNRESOLVED {
        let r = j.get(&scope(n)).unwrap().unwrap();
        finish(&mut j, &r, Ok(json!({"ok":true})));
    }
    for n in 1000..1000 + MAX_TERMINAL + 2 {
        let r = begin(&mut j, scope(n));
        finish(&mut j, &r, Ok(json!({"ok":true})));
    }
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM receipts WHERE terminal=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, MAX_TERMINAL as i64);
}

#[test]
fn corrupted_index_or_receipt_cannot_become_new_dispatch_authority() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    db.execute("UPDATE receipts SET terminal=1", []).unwrap();
    assert!(j.get(&r.scope).is_err());
    assert!(
        j.begin(
            r.scope.clone(),
            r.intent.clone(),
            "File".into(),
            "Dest".into(),
            30
        )
        .is_err()
    );
}

#[test]
fn unrelated_settlement_cannot_prune_a_corrupted_unresolved_note() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let s = scope(0);
    let original = match j
        .begin(
            s.clone(),
            intent(&s),
            "Original file".into(),
            "Destination".into(),
            0,
        )
        .unwrap()
    {
        Begin::Started(r) => r,
        _ => panic!("Expected original unresolved admission"),
    };
    // Leave exactly one slot in normal terminal history. The original note is
    // still unresolved and therefore must not count toward that history.
    for n in 1..MAX_TERMINAL {
        let r = begin(&mut j, scope(n));
        finish(&mut j, &r, Ok(json!({"ok":true})));
    }
    let unrelated = begin(&mut j, scope(MAX_TERMINAL));
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    let original_body: String = db
        .query_row(
            "SELECT body FROM receipts WHERE attempt=?1",
            [&original.attempt_id],
            |r| r.get(0),
        )
        .unwrap();
    // Only the index lies; the oldest body still has outcome=None and the exact
    // retained Unicode/opaque note. A blind terminal DELETE loses it here.
    db.execute(
        "UPDATE receipts SET terminal=1 WHERE attempt=?1",
        [&original.attempt_id],
    )
    .unwrap();
    let result = Ok(json!({"ok":true,"opaque":"Retain this result if storage fails"}));
    let retained = result.clone();
    assert!(
        j.finish_result(&unrelated.scope, &unrelated.attempt_id, result, 20)
            .is_err()
    );
    assert_eq!(
        retained.unwrap()["opaque"],
        "Retain this result if storage fails"
    );
    let after: String = db
        .query_row(
            "SELECT body FROM receipts WHERE attempt=?1",
            [&original.attempt_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, original_body);
    let preserved: Receipt = serde_json::from_str(&after).unwrap();
    assert!(preserved == original);
    assert_eq!(preserved.intent.note(), Some(note().as_slice()));
    // Settlement insertion/update also rolls back; an in-memory success has
    // not become a fabricated durable success when unrelated pruning fails.
    assert!(j.get(&unrelated.scope).unwrap().unwrap() == unrelated);
    assert!(j.get(&original.scope).is_err());
    assert!(
        j.begin(
            original.scope.clone(),
            original.intent.clone(),
            "File".into(),
            "Dest".into(),
            30
        )
        .is_err()
    );
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, (MAX_TERMINAL + 1) as i64);
}

#[test]
fn captured_owner_can_finish_after_source_or_ui_disappears_without_a_renderer_guard() {
    let temp = Temp::new();
    let r = {
        let mut original_owner = temp.journal();
        begin(&mut original_owner, scope(0))
    };
    // No shell/source/active account is needed to finish the captured owner.
    let mut original_owner = temp.journal();
    let saved = finish(&mut original_owner, &r, Ok(json!({"ok":true})));
    assert!(!saved.unresolved());
    assert_eq!(saved.intent.note(), r.intent.note());
}

#[test]
fn persistence_failure_after_response_leaves_initial_uncertainty_and_blocks_replay() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_finish BEFORE UPDATE ON receipts BEGIN SELECT RAISE(FAIL,'fixture write failure'); END;").unwrap();
    assert!(
        j.finish_result(&r.scope, &r.attempt_id, Ok(json!({"ok":true})), 20)
            .is_err()
    );
    let still = j.get(&r.scope).unwrap().unwrap();
    assert!(still == r);
    assert_eq!(still.state(), State::Unconfirmed);
    assert!(matches!(
        j.begin(
            r.scope.clone(),
            r.intent.clone(),
            "File".into(),
            "Dest".into(),
            30
        )
        .unwrap(),
        Begin::Blocked(_)
    ));
}

#[test]
fn storage_retry_after_actual_update_failure_is_exact_and_idempotent() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let raw = json!({"ok":true,"file_msg_ts":"1719800400.000001","future":{"note":"\u{65e5}\u{672c}\u{8a9e}"}});
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_retry BEFORE UPDATE ON receipts BEGIN SELECT RAISE(FAIL,'fixture update failure'); END;").unwrap();
    assert!(
        j.finish_result(&r.scope, &r.attempt_id, Ok(raw.clone()), 20)
            .is_err()
    );
    assert!(j.retry_result(&r, Ok(raw.clone()), 20).is_err());
    assert!(j.get(&r.scope).unwrap().unwrap() == r);
    db.execute_batch("DROP TRIGGER reject_retry;").unwrap();
    let saved = j.retry_result(&r, Ok(raw.clone()), 20).unwrap().unwrap();
    assert_eq!(saved.attempt_id, r.attempt_id);
    assert_eq!(saved.intent.note(), r.intent.note());
    assert_eq!(saved.raw_response(), Some(&raw));
    assert!(j.retry_result(&r, Ok(raw), 2000).unwrap().unwrap() == saved);
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM receipts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn checked_retry_refuses_changed_immutable_body_before_any_outcome_write() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    let mut body = serde_json::to_value(&r).unwrap();
    body["intent"]["note"] =
        json!([{"type":"section","text":{"type":"plain_text","text":"different admitted note"}}]);
    let encoded = serde_json::to_string(&body).unwrap();
    db.execute(
        "UPDATE receipts SET body=?1 WHERE attempt=?2",
        rusqlite::params![encoded, r.attempt_id],
    )
    .unwrap();
    assert!(j.retry_result(&r, Ok(json!({"ok":true})), 20).is_err());
    assert!(j.retry_response_limit(&r, 20).is_err());
    let after: String = db
        .query_row(
            "SELECT body FROM receipts WHERE attempt=?1",
            [&r.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, encoded);
    assert_eq!(
        j.get(&r.scope).unwrap().unwrap().state(),
        State::Unconfirmed
    );
}

#[test]
fn checked_retry_preserves_rate_observation_and_bounded_response_limit() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let r = begin(&mut j, scope(0));
    let saved = j
        .retry_result(
            &r,
            Err(Error::RateLimited {
                retry_after_secs: Some(30),
            }),
            20,
        )
        .unwrap()
        .unwrap();
    assert_eq!(j.retry_at(&r.scope.team, &r.scope.user).unwrap(), 50);
    assert!(
        j.retry_result(
            &r,
            Err(Error::RateLimited {
                retry_after_secs: Some(30)
            }),
            5000
        )
        .unwrap()
        .unwrap()
            == saved
    );
    assert_eq!(j.retry_at(&r.scope.team, &r.scope.user).unwrap(), 50);
    let next = Scope {
        team: "T87654321".into(),
        ..scope(1)
    };
    let limited = begin(&mut j, next);
    let saved = j.retry_response_limit(&limited, 20).unwrap().unwrap();
    assert_eq!(
        saved.state(),
        State::Server(FileShareDisposition::Uncertain(
            FileShareUncertainty::ResponseLimit
        ))
    );
    assert!(saved.raw_response().is_none() && saved.unresolved());
    assert!(j.retry_response_limit(&limited, 5000).unwrap().unwrap() == saved);
}

#[test]
fn checked_retry_cannot_create_missing_or_replace_a_later_attempt() {
    let temp = Temp::new();
    let mut j = temp.journal();
    let old = begin(&mut j, scope(0));
    finish(&mut j, &old, Ok(json!({"ok":true})));
    let newer = begin(&mut j, old.scope.clone());
    assert_ne!(newer.attempt_id, old.attempt_id);
    assert!(
        j.retry_result(&old, Ok(json!({"ok":true})), 30)
            .unwrap()
            .is_none()
    );
    assert!(j.retry_response_limit(&old, 30).unwrap().is_none());
    assert!(j.get(&newer.scope).unwrap().unwrap() == newer);
    let db = rusqlite::Connection::open(temp.path()).unwrap();
    db.execute("DELETE FROM receipts WHERE attempt=?1", [&newer.attempt_id])
        .unwrap();
    assert!(
        j.retry_result(&newer, Ok(json!({"ok":true})), 40)
            .unwrap()
            .is_none()
    );
    assert!(j.get(&newer.scope).unwrap().is_none());
}
