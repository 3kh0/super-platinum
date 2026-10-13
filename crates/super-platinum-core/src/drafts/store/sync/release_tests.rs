use super::tests::{Database, enroll, local, scope, state};
use super::*;
use crate::drafts::sync::{ConsumeKind, RemoteDraft};

fn applied(result: Commit) -> Stored {
    match result {
        Commit::Applied(stored) => stored,
        _ => panic!("expected commit"),
    }
}

#[test]
fn positive_send_release_preserves_newer_local_bytes_and_allows_legacy_edits() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let mut row = enroll(&mut store, &scope);
    row.state
        .request_consume(&scope, row.state.local_revision(), ConsumeKind::Send)
        .unwrap();
    row = applied(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &LocalDraft::default(),
                &row.state,
                None,
            )
            .unwrap(),
    );
    let Prepare::Prepared {
        mut stored,
        operation,
    } = store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!("send prepared")
    };
    let mut newer = local("Newer local 日本語 🚀");
    newer.composer.set_selection(1, 6);
    stored.state.edit(
        &scope,
        state(&scope, "Newer local 日本語 🚀").desired().cloned(),
    );
    stored = applied(
        store
            .commit_synced(&scope, Some(stored.revision), &newer, &stored.state, None)
            .unwrap(),
    );
    assert!(stored.state.confirm_sent(
        &scope,
        &operation.id,
        &operation.client_msg_id,
        None,
        "1800000000.000001"
    ));
    stored = applied(
        store
            .commit_synced(&scope, Some(stored.revision), &newer, &stored.state, None)
            .unwrap(),
    );
    let destination = serde_json::to_string(&scope.key).unwrap();
    let before: String = store
        .0
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&destination],
            |row| row.get(0),
        )
        .unwrap();
    let Release::Released(retained) = store.release_synced(&scope, stored.revision).unwrap() else {
        panic!("clean release")
    };
    assert_eq!(
        serde_json::to_value(retained).unwrap(),
        serde_json::to_value(&newer).unwrap()
    );
    let after: String = store
        .0
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&destination],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .synced
            .is_empty()
    );
    store
        .save([(
            scope.key.clone(),
            local("<@U123> now accepted by ordinary local store"),
        )])
        .unwrap();
    assert_eq!(
        store.load().unwrap()[0].1.composer.text,
        "<@U123> now accepted by ordinary local store"
    );
}

#[test]
fn stale_revision_and_absent_row_never_release_another_revision() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    assert!(matches!(
        store.release_synced(&scope, row.revision + 1).unwrap(),
        Release::Stale(Some(_))
    ));
    assert!(matches!(
        store.release_synced(&scope, row.revision).unwrap(),
        Release::Released(_)
    ));
    assert!(matches!(
        store.release_synced(&scope, row.revision).unwrap(),
        Release::Stale(None)
    ));
}

#[test]
fn pending_uncertain_consumed_and_remote_owned_rows_cannot_be_released() {
    for mode in ["pending", "uncertain", "consume", "remote", "conflict"] {
        let db = Database::new();
        let mut store = db.open();
        let scope = scope();
        let mut row = enroll(&mut store, &scope);
        if mode == "consume" {
            row.state
                .request_consume(&scope, row.state.local_revision(), ConsumeKind::Send)
                .unwrap();
            row = applied(
                store
                    .commit_synced(
                        &scope,
                        Some(row.revision),
                        &LocalDraft::default(),
                        &row.state,
                        None,
                    )
                    .unwrap(),
            );
        } else if mode == "conflict" {
            assert!(row.state.observe_conflict(RemoteDraft {
                scope: scope.clone(),
                id: "DrConflict".into(),
                client_msg_id: row.state.client_msg_id().into(),
                last_updated_ts: "1800000000000".into(),
                snapshot: row.state.desired().unwrap().clone()
            }));
            row = applied(
                store
                    .commit_synced(
                        &scope,
                        Some(row.revision),
                        &local("Original 🚀"),
                        &row.state,
                        None,
                    )
                    .unwrap(),
            );
        } else {
            let Prepare::Prepared { stored, operation } =
                store.prepare_synced(&scope, row.revision).unwrap()
            else {
                panic!("prepared")
            };
            row = stored;
            if mode == "uncertain" {
                assert!(row.state.mark_uncertain(&scope, &operation.id));
                row = applied(
                    store
                        .commit_synced(
                            &scope,
                            Some(row.revision),
                            &local("Original 🚀"),
                            &row.state,
                            None,
                        )
                        .unwrap(),
                );
            } else if mode == "remote" {
                assert!(row.state.confirm(
                    &operation.id,
                    RemoteDraft {
                        scope: scope.clone(),
                        id: "DrStillOwned".into(),
                        client_msg_id: operation.client_msg_id,
                        last_updated_ts: "1800000000000".into(),
                        snapshot: operation.snapshot.unwrap()
                    }
                ));
                row = applied(
                    store
                        .commit_synced(
                            &scope,
                            Some(row.revision),
                            &local("Original 🚀"),
                            &row.state,
                            None,
                        )
                        .unwrap(),
                );
            }
        }
        assert!(
            matches!(
                store.release_synced(&scope, row.revision).unwrap(),
                Release::Blocked(_)
            ),
            "{mode}"
        );
        assert!(
            store
                .load_synced(&scope.account_id, LoadMode::Current)
                .unwrap()
                .synced
                .contains_key(&scope.key)
        );
    }
}

#[test]
fn blocked_alternate_prevents_release_and_late_alternate_cannot_overwrite_released_local() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    store
        .save_local_alternate(&scope, local("Alternate survives"))
        .unwrap();
    assert!(matches!(
        store.release_synced(&scope, row.revision).unwrap(),
        Release::Blocked(_)
    ));
    let loaded = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(
        loaded.blocked_local[&scope.key].draft.composer.text,
        "Alternate survives"
    );
    let token = &loaded.blocked_local[&scope.key].token;
    let row = applied(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &local("Original 🚀"),
                &row.state,
                Some(token),
            )
            .unwrap(),
    );
    assert!(matches!(
        store.release_synced(&scope, row.revision).unwrap(),
        Release::Released(_)
    ));
    assert!(
        store
            .save_local_alternate(
                &scope,
                local("Late alternate must not become authoritative")
            )
            .is_err()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Original 🚀");
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::Current)
            .unwrap()
            .blocked_local
            .is_empty()
    );
}

#[test]
fn conditional_released_write_requires_exact_local_base_and_unowned_row() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let row = enroll(&mut store, &scope);
    let original = local("Original 🚀");
    let next = local("Released edit");
    store.release_synced(&scope, row.revision).unwrap();
    assert!(
        store
            .save_local_if_unchanged(&scope, &original, &next)
            .unwrap()
    );
    assert!(
        !store
            .save_local_if_unchanged(&scope, &original, &local("Obsolete edit"))
            .unwrap()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Released edit");
    let mut foreign = scope.clone();
    foreign.account_id = "00000000-0000-4000-8000-000000000002".into();
    assert!(
        store
            .save_local_if_unchanged(&foreign, &next, &original)
            .is_err()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Released edit");
}

#[test]
fn released_conflict_survives_restart_without_replacing_an_existing_alternate() {
    let db = Database::new();
    let scope = scope();
    {
        let mut store = db.open();
        let row = enroll(&mut store, &scope);
        store.release_synced(&scope, row.revision).unwrap();
        store
            .save([(scope.key.clone(), local("New visit authoritative"))])
            .unwrap();
        assert!(
            !store
                .save_local_if_unchanged(
                    &scope,
                    &local("Original 🚀"),
                    &local("First rejected input")
                )
                .unwrap()
        );
        assert!(
            !store
                .save_local_if_unchanged(
                    &scope,
                    &local("New visit authoritative"),
                    &local("Later rejected input")
                )
                .unwrap()
        );
    }
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert!(loaded.synced.is_empty());
    assert_eq!(loaded.local[0].1.composer.text, "New visit authoritative");
    assert_eq!(
        loaded.blocked_local[&scope.key].draft.composer.text,
        "First rejected input"
    );
}
