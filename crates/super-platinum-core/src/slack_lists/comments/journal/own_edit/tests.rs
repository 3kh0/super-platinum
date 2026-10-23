use super::*;
use crate::slack_lists::{
    Document, Record,
    comments::{OwnEditIntent, OwnEditPolicy, ReplyIntent, ReplyPolicy},
};
use serde_json::json;
use std::path::PathBuf;
const ACCOUNT: &str = "11111111-2222-4333-8444-555555555555";
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("list-own-edit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("journal.sqlite")
    }
    fn open(&self) -> Journal {
        Journal::open(self.db(), ACCOUNT).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.db());
        let _ = std::fs::remove_file(self.0.join("journal.sqlite-journal"));
        let _ = std::fs::remove_dir(&self.0);
    }
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn fixture() -> (Page, OwnEditSeed, OwnEditRecord, OwnEditAccepted) {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"date_created":1,"created_by":"U1"})).unwrap();
    let mut blocks = content::plain("Original");
    blocks.push(json!({"type":"context","elements":[],"opaque":{"keep":true}}));
    let page=Page::parse(&Target::capture(&doc,&row).unwrap(),json!({"ok":true,"messages":[
        {"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"U1","type":"message","text":"Original","blocks":blocks}]})).unwrap();
    let policy = OwnEditPolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        vec![-1],
        false,
        false,
    )
    .unwrap();
    let seed = OwnEditSeed::capture(
        "T1",
        "U1",
        &page,
        &page.messages()[1],
        &policy,
        1700000002000,
    )
    .unwrap();
    let intent =
        OwnEditIntent::capture(&seed, &page.messages()[1], &policy, edited(), 1700000002000)
            .unwrap();
    let record = intent.record().clone();
    let accepted=intent.accept(json!({"ok":true,"channel":"C01234567","ts":record.ts(),"message":{"blocks":record.blocks(),"text":"Changed"},"opaque_ack":17})).unwrap();
    (page, seed, record, accepted)
}
fn edited() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","opaque":7,"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Changed \u{4e2d}","style":{"bold":true}},{"type":"user","user_id":"U2","opaque":{"keep":true}}]}]}),
    ]
}
fn saved(j: &mut Journal, seed: &OwnEditSeed) -> EditInventory {
    let inventory = j
        .edit_inventory(&scope_for(seed).unwrap(), seed.ts())
        .unwrap();
    j.edit_save(&inventory, seed, &id(), edited()).unwrap()
}
fn observed(page: &Page, record: &OwnEditRecord) -> Page {
    let mut raw = page.raw().clone();
    raw["messages"][1]["blocks"] = json!(record.blocks());
    raw["readback_opaque"] = json!(29);
    Page::parse(page.target(), raw).unwrap()
}

#[test]
fn exact_edit_seed_blocks_and_pending_attempt_survive_actual_reopen() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let scope = scope_for(&seed).unwrap();
    let attempt_id = id();
    let expected = {
        let mut j = temp.open();
        let i = saved(&mut j, &seed);
        j.edit_begin(&i, &attempt_id, &record).unwrap()
    };
    let mut j = temp.open();
    let restored = j.edit_inventory(&scope, seed.ts()).unwrap();
    assert_eq!(restored, expected);
    assert_eq!(restored.seed(), Some(&seed));
    assert_eq!(restored.blocks(), edited());
    assert_eq!(restored.attempt().unwrap().phase(), EditPhase::Pending);
    assert_eq!(restored.attempt().unwrap().record(), &record);
    assert!(j.edit_begin(&restored, &id(), &record).is_err());
    assert!(j.edit_clear(&restored).is_err());
}
#[test]
fn empty_edit_is_a_durable_unsent_draft_and_never_a_delete() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let i = saved(&mut j, &seed);
    let empty = j.edit_save(&i, &seed, &id(), vec![]).unwrap();
    drop(j);
    let mut j = temp.open();
    let restored = j.edit_inventory(empty.scope(), empty.ts()).unwrap();
    assert_eq!(restored, empty);
    assert!(restored.blocks().is_empty());
    assert!(j.edit_begin(&restored, &id(), &record).is_err());
    assert!(j.edit_clear(&restored).unwrap().seed().is_none());
}
#[test]
fn reply_draft_and_history_are_unchanged_by_every_edit_transaction() {
    let temp = Temp::new();
    let (page, seed, record, accepted) = fixture();
    let scope = scope_for(&seed).unwrap();
    let mut j = temp.open();
    let reply = ReplyIntent::capture(
        "T1",
        "U1",
        page.target(),
        page.root().unwrap(),
        &ReplyPolicy::observed(json!({"id":"C01234567"}), false, true),
        "Independent reply",
        &id(),
        "1700000002.000001",
    )
    .unwrap();
    let reply_initial = j.inventory(&scope).unwrap();
    let reply_saved = j
        .save(&reply_initial, &id(), reply.record().blocks().to_vec())
        .unwrap();
    j.begin(&reply_saved, reply.record().clone()).unwrap();
    let before: Vec<(String, String)> = j
        .conn
        .prepare("SELECT owner,body FROM drafts UNION ALL SELECT owner,body FROM history")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let i = saved(&mut j, &seed);
    let aid = id();
    j.edit_begin(&i, &aid, &record).unwrap();
    let i = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            0,
        )
        .unwrap();
    j.edit_clear(&i).unwrap();
    let after: Vec<(String, String)> = j
        .conn
        .prepare("SELECT owner,body FROM drafts UNION ALL SELECT owner,body FROM history")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        j.inventory(&scope).unwrap().blocks(),
        reply.record().blocks()
    );
}
#[test]
fn cas_tombstones_prevent_aba_stale_save_and_attempt_id_reuse() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let zero = j.edit_inventory(&scope, seed.ts()).unwrap();
    let one = j.edit_save(&zero, &seed, &id(), edited()).unwrap();
    let aid = id();
    let pending = j.edit_begin(&one, &aid, &record).unwrap();
    assert!(j.edit_save(&one, &seed, &id(), edited()).is_err());
    let terminal = j
        .edit_resolve(&scope, seed.ts(), &aid, EditResolution::NotRequested, 77)
        .unwrap();
    let tombstone = j.edit_clear(&terminal).unwrap();
    assert!(tombstone.revision() > pending.revision());
    assert_eq!(tombstone.retry_at(), 77);
    assert!(j.edit_save(&zero, &seed, &id(), edited()).is_err());
    assert!(j.edit_clear(&terminal).is_err());
    let fresh = j.edit_save(&tombstone, &seed, &id(), edited()).unwrap();
    assert!(j.edit_begin(&fresh, &aid, &record).is_err());
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), fresh);
    assert_eq!(
        j.edit_attempt(&scope, seed.ts(), &aid)
            .unwrap()
            .unwrap()
            .phase(),
        EditPhase::NotSent
    );
}
#[test]
fn uncertain_lock_and_original_raw_survive_readback_separately() {
    let temp = Temp::new();
    let (page, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    j.edit_begin(&i, &aid, &record).unwrap();
    let raw = json!({"ok":true,"malformed_original_ack":31});
    let i = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Uncertain(Some(&raw)),
            91,
        )
        .unwrap();
    drop(j);
    let mut j = temp.open();
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), i);
    assert!(j.edit_clear(&i).is_err());
    assert!(
        j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::NotRequested, 0)
            .is_err()
    );
    let rejected = json!({"ok":false,"error":"cant_update_message"});
    assert!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Rejected(&rejected),
            0
        )
        .is_err()
    );
    let page = observed(&page, &record);
    let i = j
        .edit_resolve(&scope, seed.ts(), &aid, EditResolution::Observed(&page), 0)
        .unwrap();
    let attempt = i.attempt().unwrap();
    assert_eq!(attempt.raw(), Some(&raw));
    assert_eq!(attempt.readback(), Some(page.raw()));
    assert_eq!(attempt.phase(), EditPhase::Observed);
    assert_eq!(i.retry_at(), 91);
    assert_eq!(i.blocks(), edited());
    assert_eq!(
        j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::Observed(&page), 0)
            .unwrap(),
        i
    );
}
#[test]
fn confirmed_terminal_is_idempotent_and_rejects_conflicting_rewrites() {
    let temp = Temp::new();
    let (page, seed, record, accepted) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    j.edit_begin(&i, &aid, &record).unwrap();
    let terminal = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            123,
        )
        .unwrap();
    drop(j);
    let mut j = temp.open();
    let repeated = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted.clone()),
            100,
        )
        .unwrap();
    assert_eq!(terminal, repeated);
    assert_eq!(terminal.attempt().unwrap().raw(), Some(accepted.raw()));
    assert!(terminal.attempt().unwrap().readback().is_none());
    assert!(
        j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::NotRequested, 0)
            .is_err()
    );
    assert!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Observed(&observed(&page, &record)),
            0
        )
        .is_err()
    );
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), terminal);
    let tombstone = j.edit_clear(&terminal).unwrap();
    assert_eq!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            100
        )
        .unwrap(),
        tombstone
    );
    let newer = j.edit_save(&tombstone, &seed, &id(), edited()).unwrap();
    let pending = j.edit_begin(&newer, &id(), &record).unwrap();
    assert_eq!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            100
        )
        .unwrap(),
        pending
    );
}

#[test]
fn issued_429_retains_distinct_no_json_proof_and_cooldown_after_reopen_and_clear() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    let pending = j.edit_begin(&i, &aid, &record).unwrap();
    assert!(
        j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::RateLimited, 0)
            .is_err()
    );
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), pending);
    let limited = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::RateLimited,
            1700000099,
        )
        .unwrap();
    drop(j);
    let mut j = temp.open();
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), limited);
    let attempt = limited.attempt().unwrap();
    assert_eq!(attempt.phase(), EditPhase::RateLimited);
    assert!(attempt.raw().is_none());
    assert!(!attempt.phase().locked());
    assert_eq!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::RateLimited,
            1700000090
        )
        .unwrap(),
        limited
    );
    assert_eq!(j.edit_clear(&limited).unwrap().retry_at(), 1700000099);
}
#[test]
fn saved_newer_capture_survives_old_attempt_resolution() {
    let temp = Temp::new();
    let (page, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    let i = j.edit_begin(&i, &aid, &record).unwrap();
    let newer = j
        .edit_save(&i, &seed, &id(), content::plain("Newer retained draft"))
        .unwrap();
    let resolved = j
        .edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Observed(&observed(&page, &record)),
            0,
        )
        .unwrap();
    assert_eq!(resolved.blocks(), newer.blocks());
    assert_eq!(resolved.capture(), newer.capture());
    assert_ne!(resolved.capture(), resolved.attempt().unwrap().capture());
}
#[test]
fn invalid_outcome_foreign_identity_and_draft_reseed_cannot_unlock_or_overwrite() {
    let temp = Temp::new();
    let (page, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    let pending = j.edit_begin(&i, &aid, &record).unwrap();
    for raw in [
        json!({"ok":false,"error":"internal_error"}),
        json!({"ok":false,"error":"fatal_error"}),
        json!({"ok":true}),
        json!({"ok":false}),
    ] {
        assert!(
            j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::Rejected(&raw), 0)
                .is_err()
        );
    }
    assert!(
        j.edit_resolve(&scope, seed.ts(), &id(), EditResolution::NotRequested, 0)
            .is_err()
    );
    assert!(
        j.edit_resolve(&scope, seed.ts(), &aid, EditResolution::Observed(&page), 0)
            .is_err()
    );
    let mut foreign = scope.clone();
    foreign.user = "UOTHER".into();
    assert!(j.edit_attempt(&foreign, seed.ts(), &aid).is_err());
    let mut other = serde_json::to_value(&seed).unwrap();
    other["http"]["messages"][1]["text"] = json!("Changed elsewhere");
    let other: OwnEditSeed = serde_json::from_value(other).unwrap();
    other.validate().unwrap();
    assert!(j.edit_save(&pending, &other, &id(), edited()).is_err());
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), pending);
}
#[test]
fn sqlite_resolution_failure_rolls_back_and_retry_preserves_original_proof() {
    let temp = Temp::new();
    let (_, seed, record, accepted) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    let pending = j.edit_begin(&i, &aid, &record).unwrap();
    j.conn.execute_batch("CREATE TRIGGER fail_edit_resolution BEFORE UPDATE ON edit_history BEGIN SELECT RAISE(ABORT,'fixture local failure'); END;").unwrap();
    assert!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            0
        )
        .is_err()
    );
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), pending);
    assert_eq!(
        j.edit_attempt(&scope, seed.ts(), &aid)
            .unwrap()
            .unwrap()
            .phase(),
        EditPhase::Pending
    );
    j.conn
        .execute_batch("DROP TRIGGER fail_edit_resolution;")
        .unwrap();
    assert_eq!(
        j.edit_resolve(
            &scope,
            seed.ts(),
            &aid,
            EditResolution::Confirmed(&accepted),
            0
        )
        .unwrap()
        .attempt()
        .unwrap()
        .phase(),
        EditPhase::Confirmed
    );
}
#[test]
fn real_exclusive_lock_refuses_save_then_releases_without_mutating_original() {
    let temp = Temp::new();
    let (_, seed, _, _) = fixture();
    let mut j = temp.open();
    j.conn
        .busy_timeout(std::time::Duration::from_millis(1))
        .unwrap();
    let saved = saved(&mut j, &seed);
    let lock = Connection::open(temp.db()).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE;").unwrap();
    assert!(j.edit_save(&saved, &seed, &id(), vec![]).is_err());
    lock.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(j.edit_inventory(saved.scope(), saved.ts()).unwrap(), saved);
    assert!(
        j.edit_save(&saved, &seed, &id(), vec![])
            .unwrap()
            .blocks()
            .is_empty()
    );
}
#[test]
fn malformed_current_history_and_wrong_account_refuse_instead_of_reset() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let aid = id();
    let i = saved(&mut j, &seed);
    j.edit_begin(&i, &aid, &record).unwrap();
    j.conn
        .execute("UPDATE edit_history SET body='{}' WHERE id=?1", [&aid])
        .unwrap();
    assert!(j.edit_inventory(&scope, seed.ts()).is_err());
    drop(j);
    assert!(Journal::open(temp.db(), "99999999-2222-4333-8444-555555555555").is_err());
    let j = temp.open();
    assert!(j.edit_inventory(&scope, seed.ts()).is_err());
}

#[test]
fn oversized_receipt_and_orphaned_pending_history_refuse_without_reset() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let initial = j.edit_inventory(&scope, seed.ts()).unwrap();
    let mut raw = serde_json::to_value(&seed).unwrap();
    raw["http"]["opaque_large_receipt"] = json!("x".repeat(super::super::MAX_BODY));
    let large: OwnEditSeed = serde_json::from_value(raw).unwrap();
    large.validate().unwrap();
    assert!(j.edit_save(&initial, &large, &id(), edited()).is_err());
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), initial);
    let saved = saved(&mut j, &seed);
    let aid = id();
    j.edit_begin(&saved, &aid, &record).unwrap();
    j.conn
        .execute(
            "DELETE FROM edit_drafts WHERE owner=?1",
            [key(&scope, seed.ts()).unwrap()],
        )
        .unwrap();
    assert!(j.edit_inventory(&scope, seed.ts()).is_err());
    assert_eq!(
        j.edit_attempt(&scope, seed.ts(), &aid)
            .unwrap()
            .unwrap()
            .phase(),
        EditPhase::Pending
    );
}
#[test]
fn bounded_rows_history_and_body_fail_atomically_without_pruning() {
    let temp = Temp::new();
    let (_, seed, record, _) = fixture();
    let mut j = temp.open();
    let scope = scope_for(&seed).unwrap();
    let initial = j.edit_inventory(&scope, seed.ts()).unwrap();
    for n in 0..MAX_ROWS {
        let value = EditInventory {
            scope: scope.clone(),
            ts: format!("1800000000.{n:06}"),
            revision: 1,
            capture: id(),
            seed: None,
            blocks: vec![],
            plain_delta: None,
            attempt: None,
            retry_at: 0,
        };
        write(&j.conn, &value).unwrap();
    }
    assert!(j.edit_save(&initial, &seed, &id(), edited()).is_err());
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), initial);
    j.conn.execute("DELETE FROM edit_drafts", []).unwrap();
    let saved = saved(&mut j, &seed);
    for _ in 0..MAX_ATTEMPTS {
        let attempt = EditAttempt {
            id: id(),
            record: record.clone(),
            capture: saved.capture.clone(),
            phase: EditPhase::NotSent,
            raw: None,
            readback: None,
            retry_at: 0,
        };
        write_attempt(&j.conn, &scope, seed.ts(), &attempt, true).unwrap();
    }
    assert!(j.edit_begin(&saved, &id(), &record).is_err());
    assert_eq!(j.edit_inventory(&scope, seed.ts()).unwrap(), saved);
    assert!(
        j.edit_save(
            &saved,
            &seed,
            &id(),
            content::plain(&"x".repeat(content::MAX_BLOCK_BYTES + 1))
        )
        .is_err()
    );
    let count: i64 = j
        .conn
        .query_row("SELECT count(*) FROM edit_history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, MAX_ATTEMPTS);
}
