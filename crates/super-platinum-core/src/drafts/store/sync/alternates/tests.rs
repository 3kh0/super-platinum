use super::super::tests::{Database, enroll, local, scope};
use super::*;

#[test]
fn refused_local_writers_each_retain_only_their_latest_revision_across_restart() {
    let db = Database::new();
    let scope = scope();
    let a = LocalWriter::default();
    let b = LocalWriter::default();
    {
        let mut store = db.open();
        let row = enroll(&mut store, &scope);
        assert!(matches!(
            store.release_synced(&scope, row.revision).unwrap(),
            Release::Released(_)
        ));
        store
            .save([(scope.key.clone(), local("New visit owns this"))])
            .unwrap();
        for (writer, revision, text) in [
            (&a, 1, "A first"),
            (&b, 1, "B first"),
            (&a, 2, "A latest"),
            (&b, 3, "B latest"),
            (&a, 1, "A stale response"),
        ] {
            assert!(
                !store
                    .save_owned_local_if_unchanged(
                        &scope,
                        &local("Old base"),
                        &local(text),
                        writer,
                        revision
                    )
                    .unwrap()
            );
        }
    }
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(loaded.local[0].1.composer.text, "New visit owns this");
    assert_eq!(
        loaded.blocked_local[&scope.key].draft.composer.text,
        "A first"
    );
    let rows = &loaded.local_alternates[&scope.key];
    assert_eq!(rows.len(), 2);
    let a_row = rows.iter().find(|row| row.writer_id == a.id()).unwrap();
    assert_eq!(a_row.revision, 2);
    assert_eq!(a_row.draft.composer.text, "A latest");
    let b_row = rows.iter().find(|row| row.writer_id == b.id()).unwrap();
    assert_eq!(b_row.revision, 3);
    assert_eq!(b_row.draft.composer.text, "B latest");
    let mut other = scope.clone();
    other.account_id = "00000000-0000-4000-8000-000000000002".into();
    assert!(
        db.open()
            .save_owned_local_if_unchanged(
                &other,
                &local("Old base"),
                &local("Wrong account"),
                &a,
                99
            )
            .is_err()
    );
}

#[test]
fn managed_fallbacks_preserve_foreign_versions_and_block_unreviewed_dispatch_release_settlement() {
    let db = Database::new();
    let scope = scope();
    let mut store = db.open();
    let row = enroll(&mut store, &scope);
    let a = LocalWriter::default();
    let b = LocalWriter::default();
    for (writer, revision, text) in [
        (&a, 1, "A first"),
        (&b, 1, "B first"),
        (&a, 2, "A latest"),
        (&b, 2, "B latest"),
    ] {
        store
            .save_owned_local_alternate(&scope, &local(text), writer, revision)
            .unwrap();
    }
    // Sidecar ownership is a gate even if compatibility metadata is missing.
    store
        .0
        .execute("DELETE FROM composer_draft_blocked", [])
        .unwrap();
    assert!(matches!(
        store.prepare_synced(&scope, row.revision).unwrap(),
        Prepare::Blocked(_)
    ));
    assert!(matches!(
        store.release_synced(&scope, row.revision).unwrap(),
        Release::Blocked(_)
    ));
    let mut consuming = row.state.clone();
    consuming
        .request_consume(
            &scope,
            consuming.local_revision(),
            crate::drafts::sync::ConsumeKind::Send,
        )
        .unwrap();
    assert!(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &LocalDraft::default(),
                &consuming,
                None
            )
            .is_err()
    );
    assert!(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &local("Original 🚀"),
                &row.state,
                Some("stale token")
            )
            .is_err()
    );
    // An operation receipt may still preserve its paired content; no alternate
    // is discarded or implicitly settled by an unrelated metadata commit.
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(row.revision),
                &local("Original 🚀"),
                &row.state,
                None
            )
            .unwrap(),
        Commit::Applied(_)
    ));
    drop(store);
    let loaded = db
        .open()
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    let rows = &loaded.local_alternates[&scope.key];
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.writer_id == a.id() && row.draft.composer.text == "A latest")
    );
    assert!(
        rows.iter()
            .any(|row| row.writer_id == b.id() && row.draft.composer.text == "B latest")
    );
}
