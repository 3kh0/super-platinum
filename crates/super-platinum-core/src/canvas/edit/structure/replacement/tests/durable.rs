//! Temporary SQLite only: accepted native-wire pair -> durable completion ->
//! raw fresh load. Neither inert checkpoint nor matching visible content is a token.
use super::journal::{Store, started};
use super::*;
use recovery::journal::{Journal, Receipt, ReplacementRestore};
use rusqlite::Connection;

fn first() -> (Store, CanvasCollabSnapshot, Receipt, ConfirmedReplacement) {
    let (_, fresh, accepted) = super::native_opaque::captured();
    let store = Store::new();
    let receipt = started(
        &mut store.open(),
        accepted.checkpoint.as_ref().unwrap().clone(),
    );
    let token = accepted.confirmed_readback(&fresh, &receipt).unwrap();
    (store, fresh, receipt, token)
}
fn restored(store: &Store, receipt: &Receipt, raw: &CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    match store
        .open()
        .restore_replacement(receipt.checkpoint().scope(), raw)
        .unwrap()
    {
        ReplacementRestore::Restored(s) => s,
        _ => panic!("expected durable confirmed provenance"),
    }
}
fn blocked(j: &mut Journal, receipt: &Receipt, raw: &CanvasCollabSnapshot) {
    assert!(matches!(
        j.restore_replacement(receipt.checkpoint().scope(), raw)
            .unwrap(),
        ReplacementRestore::Blocked
    ));
}
fn live(s: &CanvasCollabSnapshot) -> String {
    s.sections
        .iter()
        .find(|r| r.id() != "title" && r.deleted().unwrap() != Some(true))
        .unwrap()
        .id()
        .into()
}
fn edit(s: &CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    let id = live(s);
    let p = text::TextIntent::capture(s, &id, 100)
        .unwrap()
        .prepare(
            s,
            &mut super::super::super::super::EditSession::new(),
            &TextDraft::from_rtml("<b>After cold restart</b>").unwrap(),
            100,
        )
        .unwrap();
    let update = Section::from_raw(p.document_data().message(1).unwrap().unwrap()).unwrap();
    let old = s.section(&id).unwrap();
    let sequence = old.sequence().unwrap().unwrap() + 1000;
    let mut row = old.merge_partial(&update).unwrap();
    let wire = row
        .crdt()
        .unwrap()
        .unwrap()
        .remove(12)
        .unwrap()
        .replace_fields(3, &[n(3, sequence)])
        .unwrap()
        .replace_fields(4, &[n(4, 1_000_000)])
        .unwrap();
    row = changed(&changed(&row, n(2, sequence)), m(38, &wire));
    let accepted = p.validate_ack(&ack(&[row.clone()])).unwrap();
    let mut fresh = s.clone();
    fresh.replacement_provenance = None;
    *fresh.sections.iter_mut().find(|r| r.id() == id).unwrap() = row;
    assert!(accepted.confirms_readback(&fresh).unwrap());
    fresh
}
fn next(
    store: &Store,
    source: &CanvasCollabSnapshot,
) -> (CanvasCollabSnapshot, Receipt, ConfirmedReplacement) {
    let p = ReplacementIntent::main_canvas(source, &live(source), 100)
        .unwrap()
        .prepare(source, 100)
        .unwrap();
    let receipt = started(&mut store.open(), p.checkpoint().unwrap());
    let rows = pair(&p, source);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let mut raw = source.clone();
    raw.replacement_provenance = None;
    raw.sections.retain(|r| r.id() != rows[0].id());
    raw.sections.extend(rows);
    let token = accepted.confirmed_readback(&raw, &receipt).unwrap();
    (raw, receipt, token)
}
fn body(store: &Store) -> String {
    Connection::open(&store.0)
        .unwrap()
        .query_row("SELECT body FROM confirmed_replacements", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn confirmed_restart_edit_repeat_and_second_restart_preserve_exact_cohort() {
    let (store, raw, receipt, token) = first();
    blocked(&mut store.open(), &receipt, &raw);
    assert!(Inventory::capture(&raw).is_err());
    assert!(store.open().complete_replacement(&receipt, &token).unwrap());
    assert!(
        store
            .open()
            .get(receipt.checkpoint().scope())
            .unwrap()
            .is_none()
    );
    let source = restored(&store, &receipt, &raw);
    assert_eq!(source.sections, raw.sections);
    assert!(can_replace_main_canvas(&source, &live(&source), 100));
    let edited = edit(&source);
    let source = restored(&store, &receipt, &edited);
    let (raw2, receipt2, token2) = next(&store, &source);
    blocked(&mut store.open(), &receipt2, &raw2);
    assert!(
        store
            .open()
            .complete_replacement(&receipt2, &token2)
            .unwrap()
    );
    let source2 = restored(&store, &receipt2, &raw2);
    assert!(can_replace_main_canvas(&source2, &live(&source2), 100));
    for row in raw
        .sections
        .iter()
        .filter(|r| r.deleted().unwrap() == Some(true))
    {
        assert_eq!(source2.section(row.id()), Some(row));
    }
    // A later successful record cannot be overwritten by an older token.
    let latest = body(&store);
    assert!(!store.open().complete_replacement(&receipt, &token).unwrap());
    assert_eq!(body(&store), latest);
}

#[test]
fn completion_retry_is_exact_and_pending_unknown_always_wins() {
    let (store, raw, receipt, token) = first();
    assert!(store.open().complete_replacement(&receipt, &token).unwrap());
    let saved = body(&store);
    assert!(store.open().complete_replacement(&receipt, &token).unwrap());
    assert_eq!(body(&store), saved);
    let newer = started(&mut store.open(), receipt.checkpoint().clone());
    assert_ne!(newer.operation_id(), receipt.operation_id());
    blocked(&mut store.open(), &newer, &raw);
    assert!(!store.open().complete_replacement(&newer, &token).unwrap());
    assert!(!store.open().complete_replacement(&receipt, &token).unwrap());
    assert!(
        store
            .open()
            .get(receipt.checkpoint().scope())
            .unwrap()
            .as_ref()
            == Some(&newer)
    );
    assert_eq!(body(&store), saved);
    // Definite non-dispatch cleanup does not manufacture a new confirmed record.
    assert!(store.open().complete(&newer).unwrap());
    assert_eq!(body(&store), saved);
    restored(&store, &receipt, &raw);
    let empty = Store::new();
    let unknown = started(&mut empty.open(), receipt.checkpoint().clone());
    blocked(&mut empty.open(), &unknown, &raw);
    assert!(empty.open().complete(&unknown).unwrap());
    assert!(matches!(
        empty
            .open()
            .restore_replacement(unknown.checkpoint().scope(), &raw)
            .unwrap(),
        ReplacementRestore::Absent
    ));
}

#[test]
fn token_requires_matching_checkpoint_session_targets_and_one_receipt() {
    let (source, raw, accepted) = super::native_opaque::captured();
    let store = Store::new();
    let actual = started(
        &mut store.open(),
        accepted.checkpoint.as_ref().unwrap().clone(),
    );
    let token = accepted.confirmed_readback(&raw, &actual).unwrap();
    for variant in 0..2 {
        let other = Store::new();
        let mut intent = ReplacementIntent::main_canvas(&source, "neighbor", 100).unwrap();
        if variant == 0 {
            intent.replacement = accepted.replacement_section().into();
        } else {
            intent.session = accepted.session_id().into();
        }
        let p = intent.prepare(&source, 100).unwrap();
        let wrong = started(&mut other.open(), p.checkpoint().unwrap());
        assert!(accepted.confirmed_readback(&raw, &wrong).is_err());
        assert!(!other.open().complete_replacement(&wrong, &token).unwrap());
        blocked(&mut other.open(), &wrong, &raw);
    }
    assert!(store.open().complete_replacement(&actual, &token).unwrap());
    let same = started(&mut store.open(), actual.checkpoint().clone());
    assert!(accepted.confirmed_readback(&raw, &same).is_err());
    assert!(!store.open().complete_replacement(&same, &token).unwrap());
}

#[test]
fn failed_commit_preserves_pending_and_prior_confirmed_record() {
    let (store, raw, receipt, token) = first();
    let db = Connection::open(&store.0).unwrap();
    db.execute_batch("CREATE TRIGGER fail_retire BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(store.open().complete_replacement(&receipt, &token).is_err());
    assert!(
        store
            .open()
            .get(receipt.checkpoint().scope())
            .unwrap()
            .as_ref()
            == Some(&receipt)
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM confirmed_replacements", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_retire").unwrap();
    assert!(store.open().complete_replacement(&receipt, &token).unwrap());
    let prior = body(&store);
    let source = restored(&store, &receipt, &raw);
    let (raw2, r2, t2) = next(&store, &source);
    db.execute_batch("CREATE TRIGGER fail_retire BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(store.open().complete_replacement(&r2, &t2).is_err());
    assert_eq!(body(&store), prior);
    blocked(&mut store.open(), &r2, &raw2);
    db.execute_batch("DROP TRIGGER fail_retire").unwrap();
    assert!(store.open().complete_replacement(&r2, &t2).unwrap());
}

#[test]
fn restore_rechecks_rows_orphans_history_and_scope_without_normalizing_bytes() {
    let (store, raw, receipt, token) = first();
    assert!(store.open().complete_replacement(&receipt, &token).unwrap());
    for id in ["neighbor", "paragraph"] {
        let mut fresh = raw.clone();
        fresh.sections.retain(|r| r.id() != id);
        assert!(
            store
                .open()
                .restore_replacement(receipt.checkpoint().scope(), &fresh)
                .is_err()
        );
        let mut fresh = raw.clone();
        let row = fresh.sections.iter_mut().find(|r| r.id() == id).unwrap();
        *row = changed(row, n(1001, 2));
        assert!(
            store
                .open()
                .restore_replacement(receipt.checkpoint().scope(), &fresh)
                .is_err()
        );
    }
    let mut fresh = raw.clone();
    let row = fresh
        .sections
        .iter_mut()
        .find(|r| r.id() == "neighbor")
        .unwrap();
    *row = Section::from_raw(row.raw().remove(1001).unwrap()).unwrap();
    assert!(
        store
            .open()
            .restore_replacement(receipt.checkpoint().scope(), &fresh)
            .is_err()
    );
    let mut fresh = raw.clone();
    fresh.sections[0] = changed(&fresh.sections[0], t(7, "foreign"));
    assert!(
        store
            .open()
            .restore_replacement(receipt.checkpoint().scope(), &fresh)
            .is_err()
    );
    let mut scope = receipt.checkpoint().scope().clone();
    scope.user = "other".into();
    assert!(store.open().restore_replacement(&scope, &raw).is_err());
    assert!(Journal::open(&store.0, "00000000-0000-4000-8000-000000000993").is_err());
    let db = Connection::open(&store.0).unwrap();
    let saved = body(&store);
    for key in ["account", "thread", "document", "operation"] {
        let mut value: serde_json::Value = serde_json::from_str(&saved).unwrap();
        value[key] = "foreign".into();
        db.execute(
            "UPDATE confirmed_replacements SET body=?1",
            [value.to_string()],
        )
        .unwrap();
        assert!(
            store
                .open()
                .restore_replacement(receipt.checkpoint().scope(), &raw)
                .is_err()
        );
    }
    db.execute("UPDATE confirmed_replacements SET body=?1", [saved])
        .unwrap();
    restored(&store, &receipt, &raw);
}

#[test]
fn v5_migration_keeps_pending_bytes_and_v6_missing_table_is_corruption() {
    let (store, raw, receipt, _) = first();
    let db = Connection::open(&store.0).unwrap();
    let before: String = db
        .query_row("SELECT body FROM receipts", [], |r| r.get(0))
        .unwrap();
    db.execute_batch("DROP TABLE confirmed_replacements; PRAGMA user_version=5;")
        .unwrap();
    blocked(&mut store.open(), &receipt, &raw);
    let after: String = db
        .query_row("SELECT body FROM receipts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM confirmed_replacements", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TABLE confirmed_replacements")
        .unwrap();
    assert!(Journal::open(&store.0, "00000000-0000-4000-8000-000000000781").is_err());
}

#[test]
fn confirmed_records_survive_more_than_32_file_scopes_without_cross_retirement() {
    let (base, _, _) = super::native_opaque::captured();
    let store = Store::new();
    let mut completed = Vec::new();
    let mut first_token = None;
    for index in 0..33 {
        // Reconstruct a validated owner envelope, not a public File-only mutation
        // leaving the snapshot's private file identity unchanged.
        let mut file = base.file.clone();
        file.id = Some(format!("Fconfirmed{index}"));
        let source =
            CanvasCollabSnapshot::from_responses(file, base.init.clone(), base.load.clone())
                .unwrap();
        assert_eq!(source.sections, base.sections);
        let p = ReplacementIntent::main_canvas(&source, "neighbor", 100)
            .unwrap()
            .prepare(&source, 100)
            .unwrap();
        let receipt = started(&mut store.open(), p.checkpoint().unwrap());
        let rows = pair(&p, &source);
        let accepted = p.validate_ack(&ack(&rows)).unwrap();
        let mut fresh = source.clone();
        fresh.sections.retain(|r| r.id() != "neighbor");
        fresh.sections.extend(rows);
        let token = accepted.confirmed_readback(&fresh, &receipt).unwrap();
        if let Some(old) = first_token.as_ref() {
            assert!(!store.open().complete_replacement(&receipt, old).unwrap());
            assert!(
                store
                    .open()
                    .get(receipt.checkpoint().scope())
                    .unwrap()
                    .as_ref()
                    == Some(&receipt)
            );
        }
        assert!(
            store.open().complete_replacement(&receipt, &token).unwrap(),
            "scope {index}"
        );
        if first_token.is_none() {
            first_token = Some(token);
        }
        completed.push((receipt, fresh));
    }
    // Close/reopen SQLite independently for each restore. The in-flight receipt
    // limit does not become a lifetime limit for successful confirmed canvases.
    for (receipt, raw) in &completed {
        assert!(
            store
                .open()
                .get(receipt.checkpoint().scope())
                .unwrap()
                .is_none()
        );
        let s = restored(&store, receipt, raw);
        assert_eq!(s.sections, raw.sections);
        assert!(can_replace_main_canvas(&s, &live(&s), 100));
    }
    let db = Connection::open(&store.0).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM confirmed_replacements", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        33
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM receipts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
