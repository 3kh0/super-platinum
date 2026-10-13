use super::*;

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";
const OTHER_ACCOUNT: &str = "00000000-0000-4000-8000-000000000002";

struct Database(std::path::PathBuf);
impl Database {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("platinum-forward-{}.sqlite", uuid::Uuid::new_v4())))
    }
    fn open(&self) -> Journal {
        Journal::open(&self.0, ACCOUNT).unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn scope() -> Scope {
    Scope {
        team_id: "T1".into(),
        user_id: "U1".into(),
        source: "C_SOURCE".into(),
        timestamp: "100.000001".into(),
        destination: "C_DEST".into(),
    }
}
fn request(scope: &Scope) -> ForwardMessage {
    ForwardMessage::new(
        scope.source.clone(),
        scope.timestamp.clone(),
        scope.destination.clone(),
        vec![serde_json::json!({
            "type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"日本語 🚀\nA second line","style":{"bold":true}},
                {"type":"unknown_future_element","opaque":{"preserve":[1,2,3]}}
            ]}]
        })],
    )
}
fn start(journal: &mut Journal, scope: &Scope, now: u64) -> Receipt {
    let Begin::Started(receipt) = journal
        .begin(
            scope.clone(),
            "Destination 日本語".into(),
            request(scope),
            now,
        )
        .unwrap()
    else {
        panic!("start blocked")
    };
    receipt
}
fn success(scope: &Scope) -> Result<ForwardedMessage, Error> {
    Ok(ForwardedMessage {
        channel: scope.destination.clone(),
        ts: "101.000002".into(),
    })
}
fn found(receipt: &Receipt) -> Message {
    Message {
        client_msg_id: Some(receipt.delivery.request.client_msg_id.clone()),
        user: Some(receipt.scope.user_id.clone()),
        ts: Some("101.000002".into()),
        ..Default::default()
    }
}

#[test]
fn crash_before_reply_preserves_rich_request_and_blocks_changed_note_on_reopen() {
    let db = Database::new();
    let scope = scope();
    let original = start(&mut db.open(), &scope, 100);
    // No finish: this includes crash-before-dispatch and lost-reply cases.
    let mut journal = db.open();
    assert_eq!(journal.get(&scope).unwrap(), Some(original.clone()));
    assert!(original.unresolved());
    let mut changed = request(&scope);
    changed.blocks.clear();
    assert_eq!(
        journal
            .begin(scope.clone(), "Changed".into(), changed, 200)
            .unwrap(),
        Begin::Blocked(original.clone())
    );
    assert_eq!(
        journal.unresolved("T1", "U1").unwrap(),
        vec![original.clone()]
    );
    assert_eq!(
        journal
            .list_source("T1", "U1", "C_SOURCE", "100.000001")
            .unwrap(),
        vec![original]
    );
}

#[test]
fn simultaneous_connections_authorize_one_dispatch_only() {
    let db = Database::new();
    // Prepare schema before the race so the assertion isolates begin itself.
    drop(db.open());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let path = db.0.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut journal = Journal::open(path, ACCOUNT).unwrap();
                barrier.wait();
                journal
                    .begin(scope(), "Destination".into(), request(&scope()), 100)
                    .unwrap()
            })
        })
        .collect();
    let receipts: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(
        receipts
            .iter()
            .filter(|r| matches!(r, Begin::Started(_)))
            .count(),
        1
    );
    assert_eq!(
        receipts
            .iter()
            .filter(|r| matches!(r, Begin::Blocked(_)))
            .count(),
        1
    );
    let ids: Vec<_> = receipts
        .iter()
        .map(|r| match r {
            Begin::Started(r) | Begin::Blocked(r) => &r.attempt_id,
        })
        .collect();
    assert_eq!(ids[0], ids[1]);
}

#[test]
fn explicit_resend_keeps_payload_but_rotates_attempt_and_retains_earlier_uncertainty() {
    let db = Database::new();
    let mut journal = db.open();
    let scope = scope();
    let original = start(&mut journal, &scope, 100);
    journal
        .finish(
            &scope,
            &original.attempt_id,
            Err(Error::Offline("test lost reply".into())),
            101,
        )
        .unwrap();
    let Some(Begin::Started(resend)) = journal.resend(&scope, &original.attempt_id, 102).unwrap()
    else {
        panic!("resend")
    };
    assert_ne!(original.attempt_id, resend.attempt_id);
    assert_eq!(original.delivery.request, resend.delivery.request);
    assert_eq!(original.delivery.started_at, resend.delivery.started_at);
    assert!(resend.delivery.previously_uncertain());
    assert!(
        !journal
            .finish(&scope, &original.attempt_id, success(&scope), 103)
            .unwrap()
    );
    assert!(!journal.retire(&scope, &original.attempt_id, 103).unwrap());
    assert!(
        !journal
            .defer_check(&scope, &original.attempt_id, 103, 999)
            .unwrap()
    );
    assert!(
        journal
            .resend(&scope, &original.attempt_id, 103)
            .unwrap()
            .is_none()
    );
    assert!(
        journal
            .finish(
                &scope,
                &resend.attempt_id,
                Err(Error::Api("not_in_channel".into())),
                104
            )
            .unwrap()
    );
    let loaded = db.open().get(&scope).unwrap().unwrap();
    assert!(
        loaded.unresolved(),
        "rejection of resend cannot reject earlier request"
    );
    assert_eq!(loaded.delivery.request, original.delivery.request);
}

#[test]
fn positive_history_requires_exact_scope_attempt_request_sender_and_timestamp() {
    let db = Database::new();
    let mut journal = db.open();
    let scope = scope();
    let receipt = start(&mut journal, &scope, 100);
    let confirm = |journal: &mut Journal,
                   scope: &Scope,
                   attempt: &str,
                   id: &str,
                   sender: &str,
                   messages: &[Message]| {
        journal
            .confirm(scope, attempt, id, sender, messages, 101)
            .unwrap()
    };
    let id = &receipt.delivery.request.client_msg_id;
    let message = found(&receipt);
    assert!(!confirm(
        &mut journal,
        &scope,
        "old",
        id,
        "U1",
        &[message.clone()]
    ));
    assert!(!confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        "old",
        "U1",
        &[message.clone()]
    ));
    assert!(!confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "OTHER",
        &[message.clone()]
    ));
    assert!(!confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "U1",
        &[]
    ));
    let mut invalid = message.clone();
    invalid.ts = Some("bad".into());
    assert!(!confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "U1",
        &[invalid]
    ));
    let mut wrong_channel = message.clone();
    wrong_channel.channel = Some("OTHER".into());
    assert!(!confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "U1",
        &[wrong_channel]
    ));
    let mut other = scope.clone();
    other.team_id = "OTHER".into();
    assert!(!confirm(
        &mut journal,
        &other,
        &receipt.attempt_id,
        id,
        "U1",
        &[message.clone()]
    ));
    assert!(confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "U1",
        &[message.clone()]
    ));
    assert!(confirm(
        &mut journal,
        &scope,
        &receipt.attempt_id,
        id,
        "U1",
        &[message]
    ));
    assert!(
        journal
            .finish(&scope, &receipt.attempt_id, success(&scope), 102)
            .unwrap(),
        "same write receipt after history is idempotent"
    );
    assert!(
        !journal
            .finish(
                &scope,
                &receipt.attempt_id,
                Err(Error::Offline("late error".into())),
                103
            )
            .unwrap()
    );
    assert!(journal.unresolved("T1", "U1").unwrap().is_empty());
}

#[test]
fn check_cooldown_survives_reopen_retirement_and_new_intent() {
    let db = Database::new();
    let scope = scope();
    let receipt = start(&mut db.open(), &scope, 100);
    let mut journal = db.open();
    assert!(
        journal
            .defer_check(&scope, &receipt.attempt_id, 101, 160)
            .unwrap()
    );
    assert!(
        journal
            .defer_check(&scope, &receipt.attempt_id, 102, 130)
            .unwrap()
    );
    assert_eq!(
        journal.get(&scope).unwrap().unwrap().delivery.retry_at(),
        160
    );
    assert!(matches!(
        journal.resend(&scope, &receipt.attempt_id, 159).unwrap(),
        Some(Begin::Blocked(_))
    ));
    assert!(journal.retire(&scope, &receipt.attempt_id, 103).unwrap());
    assert!(journal.retire(&scope, &receipt.attempt_id, 104).unwrap());
    let retired = db.open().get(&scope).unwrap().unwrap();
    assert!(retired.retired);
    assert!(matches!(retired.delivery.status, Status::Uncertain(_)));
    assert!(journal.unresolved("T1", "U1").unwrap().is_empty());
    assert!(matches!(
        journal
            .begin(scope.clone(), "New".into(), request(&scope), 159)
            .unwrap(),
        Begin::Blocked(_)
    ));
    let replacement = start(&mut journal, &scope, 160);
    assert_ne!(receipt.attempt_id, replacement.attempt_id);
    assert!(
        !journal
            .finish(&scope, &receipt.attempt_id, success(&scope), 161)
            .unwrap()
    );
    assert!(!journal.retire(&scope, &receipt.attempt_id, 161).unwrap());
    assert_eq!(journal.get(&scope).unwrap(), Some(replacement));
}

#[test]
fn write_rate_limit_is_a_definite_rejection_only_without_previous_uncertainty() {
    let db = Database::new();
    let mut journal = db.open();
    let scope = scope();
    let receipt = start(&mut journal, &scope, 100);
    let limited = || {
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        })
    };
    assert!(
        journal
            .finish(&scope, &receipt.attempt_id, limited(), 101)
            .unwrap()
    );
    let failed = journal.get(&scope).unwrap().unwrap();
    assert!(matches!(failed.delivery.status, Status::Failed(_)));
    assert_eq!(failed.delivery.retry_at(), 161);
    assert!(
        journal
            .finish(&scope, &receipt.attempt_id, limited(), 102)
            .unwrap()
    );
    assert_eq!(
        journal.get(&scope).unwrap(),
        Some(failed),
        "duplicate callback does not extend or rewrite outcome"
    );
    assert!(matches!(
        journal
            .begin(scope.clone(), "Again".into(), request(&scope), 160)
            .unwrap(),
        Begin::Blocked(_)
    ));
    let next = start(&mut journal, &scope, 161);
    let Some(Begin::Started(resend)) = journal.resend(&scope, &next.attempt_id, 162).unwrap()
    else {
        panic!()
    };
    assert!(
        journal
            .finish(&scope, &resend.attempt_id, limited(), 163)
            .unwrap()
    );
    assert!(journal.get(&scope).unwrap().unwrap().unresolved());
    assert!(matches!(
        journal.resend(&scope, &resend.attempt_id, 222).unwrap(),
        Some(Begin::Blocked(_))
    ));
}

#[test]
fn repeated_callbacks_cannot_convert_recorded_ambiguity_into_rejection() {
    let db = Database::new();
    let mut journal = db.open();
    let scope = scope();
    let receipt = start(&mut journal, &scope, 100);
    let lost = || Err(Error::Offline("test".into()));
    assert!(
        journal
            .finish(&scope, &receipt.attempt_id, lost(), 101)
            .unwrap()
    );
    assert!(
        journal
            .finish(&scope, &receipt.attempt_id, lost(), 102)
            .unwrap()
    );
    assert!(
        !journal
            .finish(
                &scope,
                &receipt.attempt_id,
                Err(Error::Api("not_in_channel".into())),
                103
            )
            .unwrap()
    );
    assert!(journal.get(&scope).unwrap().unwrap().unresolved());
    let message = found(&receipt);
    assert!(
        journal
            .confirm(
                &scope,
                &receipt.attempt_id,
                &receipt.delivery.request.client_msg_id,
                "U1",
                &[message],
                104
            )
            .unwrap()
    );
}

#[test]
fn write_response_does_not_shorten_an_already_persisted_check_cooldown() {
    let db = Database::new();
    let mut journal = db.open();
    let scope = scope();
    let receipt = start(&mut journal, &scope, 100);
    assert!(
        journal
            .defer_check(&scope, &receipt.attempt_id, 101, 999)
            .unwrap()
    );
    assert!(
        journal
            .finish(
                &scope,
                &receipt.attempt_id,
                Err(Error::RateLimited {
                    retry_after_secs: Some(30)
                }),
                102
            )
            .unwrap()
    );
    assert_eq!(
        journal.get(&scope).unwrap().unwrap().delivery.retry_at(),
        999
    );
}

#[test]
fn account_workspace_user_source_and_destination_remain_isolated() {
    let db = Database::new();
    let mut journal = db.open();
    let original = scope();
    start(&mut journal, &original, 100);
    for n in 0..5 {
        let mut other = original.clone();
        match n {
            0 => other.team_id = "T2".into(),
            1 => other.user_id = "U2".into(),
            2 => other.source = "C_OTHER".into(),
            3 => other.timestamp = "100.000002".into(),
            _ => other.destination = "C_OTHER".into(),
        }
        assert!(journal.get(&other).unwrap().is_none());
        start(&mut journal, &other, 101);
    }
    assert_eq!(journal.unresolved("T1", "U1").unwrap().len(), 4);
    assert_eq!(
        journal
            .list_source("T1", "U1", "C_SOURCE", "100.000001")
            .unwrap()
            .len(),
        2
    );
    assert!(Journal::open(&db.0, OTHER_ACCOUNT).is_err());
    assert!(Journal::open(&db.0, "../../bad").is_err());
    let second = Database::new();
    let mut other = Journal::open(&second.0, OTHER_ACCOUNT).unwrap();
    assert!(other.unresolved("T1", "U1").unwrap().is_empty());
    start(&mut other, &original, 100);
}

#[test]
fn failed_commit_cannot_grant_dispatch_or_replace_prior_receipt() {
    let db = Database::new();
    let mut journal = db.open();
    journal.conn.execute_batch("CREATE TRIGGER reject_receipt BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    assert!(
        journal
            .begin(scope(), "Name".into(), request(&scope()), 100)
            .is_err()
    );
    assert!(journal.get(&scope()).unwrap().is_none());
    journal
        .conn
        .execute_batch("DROP TRIGGER reject_receipt;")
        .unwrap();
    let original = start(&mut journal, &scope(), 100);
    journal.conn.execute_batch("CREATE TRIGGER reject_update BEFORE UPDATE ON receipts BEGIN SELECT RAISE(ABORT,'test disk failure'); END;").unwrap();
    assert!(journal.resend(&scope(), &original.attempt_id, 101).is_err());
    assert_eq!(db.open().get(&scope()).unwrap(), Some(original));
}

#[test]
fn terminal_retention_never_prunes_unresolved_or_active_cooldowns() {
    let db = Database::new();
    let mut journal = db.open();
    let unresolved = start(&mut journal, &scope(), 1);
    let mut deferred = scope();
    deferred.destination = "COOLDOWN".into();
    let cooldown = start(&mut journal, &deferred, 2);
    journal
        .defer_check(&deferred, &cooldown.attempt_id, 3, 10_000)
        .unwrap();
    journal.retire(&deferred, &cooldown.attempt_id, 4).unwrap();
    for n in 0..140 {
        let mut scope = scope();
        scope.destination = format!("DEST_{n}");
        let receipt = start(&mut journal, &scope, 100 + n);
        journal
            .finish(&scope, &receipt.attempt_id, success(&scope), 100 + n)
            .unwrap();
    }
    let count: i64 = journal
        .conn
        .query_row("SELECT COUNT(*) FROM receipts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, MAX_TERMINAL + 2);
    assert_eq!(journal.get(&scope()).unwrap(), Some(unresolved));
    assert_eq!(
        journal.get(&deferred).unwrap().unwrap().delivery.retry_at(),
        10_000
    );
}

#[test]
fn invalid_payload_schema_and_permissions_fail_safely() {
    let db = Database::new();
    let mut journal = db.open();
    let mut bad = request(&scope());
    bad.destination = "OTHER".into();
    assert!(journal.begin(scope(), "Name".into(), bad, 100).is_err());
    let mut bad = request(&scope());
    bad.client_msg_id = "bad".into();
    assert!(journal.begin(scope(), "Name".into(), bad, 100).is_err());
    assert!(
        journal
            .begin(scope(), "Name".into(), request(&scope()), u64::MAX)
            .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&db.0).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    journal
        .conn
        .execute_batch("PRAGMA user_version=99;")
        .unwrap();
    drop(journal);
    assert!(Journal::open(&db.0, ACCOUNT).is_err());
}
