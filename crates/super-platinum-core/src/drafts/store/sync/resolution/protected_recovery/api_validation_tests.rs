//! Invalid command inputs and inventory growth must fail inside their transaction.
use super::*;

// Simulate a near-byte-limit but valid inventory with JSON trailing whitespace.
// No production limits/flags are changed, and every operation still uses Client.
fn near_limit(f: &Fixture, table: &str) {
    let (key, team) = match table {
        "composer_draft_writer_lineage" => ("writer_id", "destination"),
        "composer_draft_execution_result" => ("id", "team"),
        _ => panic!("unexpected test table"),
    };
    let conn = Connection::open(f.path.join("drafts.sqlite")).unwrap();
    let bytes: i64 = conn.query_row(&format!("SELECT SUM(LENGTH(CAST({key} AS BLOB))+LENGTH(CAST({team} AS BLOB))+LENGTH(CAST(content AS BLOB))) FROM {table}"), [], |r| r.get(0)).unwrap();
    let padding = (16 * 1024 * 1024 - 100 - bytes) as usize;
    assert!(padding > 0);
    // Pad the oldest entry; target tests change a later/different record.
    conn.execute(
        &format!(
            "UPDATE {table} SET content=content || ?1 WHERE rowid=(SELECT MIN(rowid) FROM {table})"
        ),
        [" ".repeat(padding)],
    )
    .unwrap();
}
fn trim_padding(f: &Fixture, table: &str) {
    let conn = Connection::open(f.path.join("drafts.sqlite")).unwrap();
    conn.execute(&format!("UPDATE {table} SET content=rtrim(content)"), [])
        .unwrap();
}

#[test]
fn stale_checkpoint_foreign_direct_intent_refuses_without_poisoning_inventory() {
    let f = Fixture::new();
    let (stream, before) = f.verified("frozen");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    let consumed = f
        .client
        .consume(stream, before.clone(), ConsumeKind::Send)
        .wait()
        .unwrap();
    let mut foreign = consumed
        .row()
        .state()
        .local_preservation_intent()
        .unwrap()
        .clone();
    foreign.scope.key.channel = "OTHER".into();
    assert!(matches!(
        f.client
            .checkpoint(
                writer.clone(),
                1,
                before.clone(),
                local("retained by caller"),
                Some(foreign)
            )
            .wait(),
        Err(Error::Stale)
    ));
    assert!(f.current().same(&consumed));
    let loaded = f.client.load(LoadMode::Current).wait().unwrap();
    assert!(loaded.managed()[&f.scope.key].same(&consumed));
    assert!(matches!(
        f.client
            .checkpoint(writer, 1, before, local("retained by caller"), None)
            .wait()
            .unwrap(),
        WriteResult::Preserved(_)
    ));
}

#[test]
fn writer_receipt_growth_rolls_back_canonical_and_serial_before_limit_crossing() {
    let f = Fixture::new();
    let before = f.seed("before");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    near_limit(&f, "composer_draft_writer_lineage");
    f.client.load(LoadMode::Current).wait().unwrap();
    assert!(matches!(
        f.client
            .checkpoint(writer.clone(), 1, before.clone(), local("after"), None)
            .wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
    f.client.load(LoadMode::Current).wait().unwrap();
    trim_padding(&f, "composer_draft_writer_lineage");
    let WriteResult::Applied(after) = f
        .client
        .checkpoint(writer, 1, before, local("after"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(after.local().composer.text, "after");
}

#[test]
fn execution_receipt_growth_rolls_back_outcome_then_exact_response_retries() {
    let f = Fixture::new();
    let (stream, current, remote) = f.create_remote();
    // Create establishes a new acknowledged identity; it does not grant the
    // baseline authority required for a later Update.
    assert_eq!(current.row().lineage().verified_epoch(), 0);
    assert_eq!(
        current.row().lineage().basis(),
        crate::drafts::store::protected::Basis::Unverified
    );
    assert!(matches!(
        f.client.prepare(stream.clone(), current.clone()).wait(),
        Err(Error::Gap)
    ));
    assert!(f.current().same(&current));
    let baseline = f
        .client
        .reserve_baseline(stream.clone(), current, 1000)
        .wait()
        .unwrap();
    let BaselineClaim::Ready { completion, .. } =
        f.client.claim_baseline(baseline, 1000).wait().unwrap()
    else {
        panic!("fresh baseline must be ready")
    };
    let BaselineResult::Retained(evidence) = f
        .client
        .retain_baseline(
            completion,
            BaselineResponse::Positive {
                raw: raw(&remote),
                user: "U1".into(),
            },
            0,
        )
        .wait()
        .unwrap()
    else {
        panic!("complete baseline must be retained")
    };
    let current = f
        .client
        .finish_baseline(evidence, RemoteChoice::Remote)
        .wait()
        .unwrap();
    assert_eq!(current.row().lineage().verified_epoch(), stream.epoch());
    assert_eq!(
        current.row().lineage().basis(),
        crate::drafts::store::protected::Basis::Remote
    );
    let writer = f.client.register_writer(current.clone()).wait().unwrap();
    let WriteResult::Applied(edited) = f
        .client
        .checkpoint(writer, 1, current, local("updated"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let ticket = f.client.prepare(stream, edited).wait().unwrap();
    let (operation, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_eq!(operation.kind, OperationKind::Update);
    let before = f.current();
    near_limit(&f, "composer_draft_execution_result");
    f.client.load(LoadMode::Current).wait().unwrap();
    assert!(matches!(
        f.client
            .record(receipt.clone(), Outcome::Uncertain, 0)
            .wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
    f.client.load(LoadMode::Current).wait().unwrap();
    trim_padding(&f, "composer_draft_execution_result");
    let after = f
        .client
        .record(receipt.clone(), Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    assert!(matches!(
        after.row().state().workflow(),
        Workflow::Uncertain(_)
    ));
    let original = f
        .client
        .record(receipt, Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    assert!(original.same(&after));
}
