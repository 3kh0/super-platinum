use super::super::{Owner, api::Frozen, catalog::Catalog};
use super::*;
use serde_json::{Value, json};
const ACCOUNT: &str = "c765fef0-1f85-49f0-ab9c-fdb779292337";
const RUN: &str = "a697e70a-f042-4bc7-b254-56e8d566e58e";
const OTHER_RUN: &str = "6f0bcd11-09b1-4c76-a7e8-4cd8dc912a76";
fn now(mono: i64) -> Stamp {
    Stamp {
        run: RUN.into(),
        mono,
        wall: 1_000_000 + mono,
    }
}
fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    install(&mut conn, ACCOUNT).unwrap();
    conn
}
fn prepared(team: &str, channel: &str) -> Record {
    let owner = Owner {
        account: ACCOUNT.into(),
        team: team.into(),
        user: "U1".into(),
        channel: channel.into(),
        thread: None,
    };
    let catalog=Catalog::decode(owner.clone(),json!({"ok":true,"app_actions":[],"commands":[{"type":"app","name":"/example","app":"A1","teams":[team],"is_runnable_from_thread":true}]})).unwrap();
    let frozen = Frozen::capture(
        owner,
        1,
        json!({"ops":[{"insert":"/example secret-free test"}]}),
        "/example".into(),
        None,
        &catalog.choice(0).unwrap(),
        &catalog,
    )
    .unwrap();
    Record::decode(&serde_json::to_vec(&json!({"id":uuid::Uuid::new_v4().to_string(),"version":1,"frozen":frozen,"phase":"Prepared","retry_after_secs":null,"http_status":null,"retired":false})).unwrap()).unwrap()
}
fn next(old: &Record, phase: Value, status: Option<u16>, retry: Option<u64>) -> Record {
    let mut value = serde_json::to_value(old).unwrap();
    value["version"] = json!(old.version() + 1);
    value["phase"] = phase;
    value["http_status"] = json!(status);
    value["retry_after_secs"] = json!(retry);
    Record::decode(&serde_json::to_vec(&value).unwrap()).unwrap()
}
fn armed(old: &Record) -> Record {
    next(old, json!("DispatchPossible"), None, None)
}
fn ack(old: &Record, retry: Option<u64>) -> Record {
    next(
        old,
        json!({"Completed":{"Acknowledged":{"keep_input":false,"feedback":null,"raw":{"ok":true}}}}),
        None,
        retry,
    )
}
fn retired(old: &Record) -> Record {
    let mut value = serde_json::to_value(old).unwrap();
    value["version"] = json!(old.version() + 1);
    value["retired"] = json!(true);
    Record::decode(&serde_json::to_vec(&value).unwrap()).unwrap()
}
fn apply(
    conn: &mut Connection,
    old: Option<&Record>,
    next: &Record,
    now: &Stamp,
) -> Result<(), Failure> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    match transaction::apply(&tx, ACCOUNT, old, next, now) {
        Err(Failure::RateLimited) => {
            tx.commit().map_err(storage)?;
            Err(Failure::RateLimited)
        }
        Err(error) => Err(error),
        Ok(()) => tx.commit().map_err(storage),
    }
}
fn only(conn: &mut Connection) -> Record {
    let mut rows = load(conn, ACCOUNT).unwrap();
    assert_eq!(rows.len(), 1);
    rows.remove(0)
}

#[test]
fn additive_migration_preserves_existing_draft_tables_and_is_idempotent() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT NOT NULL); INSERT INTO composer_drafts VALUES('synthetic-destination','retained-input');").unwrap();
    install(&mut conn, ACCOUNT).unwrap();
    install(&mut conn, ACCOUNT).unwrap();
    assert_eq!(
        conn.query_row("SELECT content FROM composer_drafts", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "retained-input"
    );
    assert!(load(&mut conn, ACCOUNT).unwrap().is_empty());
    assert!(install(&mut conn, OTHER_RUN).is_err());
}
#[test]
fn schema_install_failure_rolls_back_every_new_table() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE unrelated(id INTEGER);CREATE INDEX sp_slash_scope ON unrelated(id);",
    )
    .unwrap();
    assert!(install(&mut conn, ACCOUNT).is_err());
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'sp_slash_%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn partial_future_or_foreign_schema_fails_without_repair() {
    let mut partial = Connection::open_in_memory().unwrap();
    partial
        .execute("CREATE TABLE sp_slash_intents(id TEXT)", [])
        .unwrap();
    assert!(install(&mut partial, ACCOUNT).is_err());
    let mut conn = db();
    conn.execute("UPDATE sp_slash_meta SET version=2", [])
        .unwrap();
    assert!(install(&mut conn, ACCOUNT).is_err());
    assert!(load(&mut conn, ACCOUNT).is_err());
    assert_eq!(
        conn.query_row("SELECT version FROM sp_slash_meta", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn existing_draft_account_mismatch_refuses_migration_and_use() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE composer_draft_owner(singleton INTEGER PRIMARY KEY,account TEXT NOT NULL)",
    )
    .unwrap();
    conn.execute("INSERT INTO composer_draft_owner VALUES(1,?1)", [OTHER_RUN])
        .unwrap();
    assert!(install(&mut conn, ACCOUNT).is_err());
    conn.execute("UPDATE composer_draft_owner SET account=?1", [ACCOUNT])
        .unwrap();
    install(&mut conn, ACCOUNT).unwrap();
    conn.execute("UPDATE composer_draft_owner SET account=?1", [OTHER_RUN])
        .unwrap();
    assert!(apply(&mut conn, None, &prepared("T1", "C1"), &now(0)).is_err());
}
#[test]
fn full_record_cas_preserves_frozen_input_and_rejects_stale_or_changed_content() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    let mut value = serde_json::to_value(&p).unwrap();
    value["frozen"]["original"] = json!({"ops":[{"insert":"changed"}]});
    let changed = Record::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(apply(&mut conn, Some(&changed), &armed(&changed), &now(1)).is_err());
    assert!(only(&mut conn) == p);
    let d = armed(&p);
    apply(&mut conn, Some(&p), &d, &now(2)).unwrap();
    assert_eq!(
        apply(&mut conn, Some(&p), &d, &now(3)),
        Err(Failure::NoReplay)
    );
    assert!(only(&mut conn) == d);
}
#[test]
fn terminal_completion_is_idempotent_but_conflicting_reply_cannot_replace_it() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let d = armed(&p);
    let a = ack(&d, None);
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    apply(&mut conn, Some(&d), &a, &now(2)).unwrap();
    apply(&mut conn, Some(&d), &a, &now(3)).unwrap();
    let wrong = next(&d, json!("Unknown"), None, None);
    assert_eq!(
        apply(&mut conn, Some(&d), &wrong, &now(4)),
        Err(Failure::Stale)
    );
    assert!(only(&mut conn) == a);
    let r = retired(&a);
    apply(&mut conn, Some(&a), &r, &now(5)).unwrap();
    assert!(only(&mut conn).original() == p.original());
    assert!(only(&mut conn).phase() == a.phase());
    let noncanonical = serde_json::to_vec_pretty(&r).unwrap();
    conn.execute("UPDATE sp_slash_intents SET body=?1", [&noncanonical])
        .unwrap();
    assert_eq!(apply(&mut conn, Some(&a), &r, &now(6)), Err(Failure::Stale));
    assert_eq!(
        conn.query_row("SELECT body FROM sp_slash_intents", [], |row| row
            .get::<_, Vec<u8>>(0))
            .unwrap(),
        noncanonical
    );
}
#[test]
fn dispatch_blocks_every_destination_for_same_workspace_user_but_not_other_team() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let second = prepared("T1", "C2");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, None, &second, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &armed(&p), &now(1)).unwrap();
    assert_eq!(
        apply(&mut conn, Some(&second), &armed(&second), &now(1)),
        Err(Failure::NoReplay)
    );
    assert_eq!(
        apply(&mut conn, None, &prepared("T1", "C3"), &now(1)),
        Err(Failure::NoReplay)
    );
    let other = prepared("T2", "C3");
    apply(&mut conn, None, &other, &now(1)).unwrap();
    apply(&mut conn, Some(&other), &armed(&other), &now(2)).unwrap();
}
#[test]
fn terminal_ack_allows_new_intent_but_unknown_never_releases_owner_gate() {
    for terminal_unknown in [false, true] {
        let mut conn = db();
        let p = prepared("T1", "C1");
        let d = armed(&p);
        apply(&mut conn, None, &p, &now(0)).unwrap();
        apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
        let result = if terminal_unknown {
            next(&d, json!("Unknown"), None, None)
        } else {
            ack(&d, None)
        };
        apply(&mut conn, Some(&d), &result, &now(2)).unwrap();
        assert_eq!(
            apply(&mut conn, None, &prepared("T1", "C2"), &now(3)).is_err(),
            terminal_unknown
        );
    }
}
#[test]
fn prepared_from_prior_process_never_dispatches_and_unknown_cannot_retire() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    let mut restart = now(0);
    restart.run = OTHER_RUN.into();
    assert_eq!(
        apply(&mut conn, Some(&p), &armed(&p), &restart),
        Err(Failure::NoReplay)
    );
    let d = armed(&p);
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    let u = next(&d, json!("Unknown"), None, None);
    apply(&mut conn, Some(&d), &u, &now(2)).unwrap();
    assert!(apply(&mut conn, Some(&u), &retired(&u), &now(3)).is_err());
}
#[test]
fn rollback_keeps_dispatch_unresolved_if_cooldown_write_fails() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    let d = armed(&p);
    apply(&mut conn, None, &p, &now(0)).unwrap();
    apply(&mut conn, Some(&p), &d, &now(1)).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_gate BEFORE INSERT ON sp_slash_cooldowns BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    let outcome = next(&d, json!("Unknown"), Some(429), Some(30));
    assert_eq!(
        apply(&mut conn, Some(&d), &outcome, &now(2)),
        Err(Failure::Persistence)
    );
    assert!(only(&mut conn) == d);
    conn.execute_batch("DROP TRIGGER fail_gate").unwrap();
    apply(&mut conn, Some(&d), &outcome, &now(2)).unwrap();
    assert!(only(&mut conn) == outcome);
}
#[test]
fn malformed_or_misindexed_record_blocks_admission_without_repair() {
    let mut conn = db();
    let p = prepared("T1", "C1");
    apply(&mut conn, None, &p, &now(0)).unwrap();
    conn.execute("UPDATE sp_slash_intents SET scope='wrong'", [])
        .unwrap();
    assert!(apply(&mut conn, None, &prepared("T2", "C2"), &now(1)).is_err());
    assert_eq!(
        conn.query_row("SELECT scope FROM sp_slash_intents", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "wrong"
    );
}

mod cooldown_tests;
mod durability_tests;
