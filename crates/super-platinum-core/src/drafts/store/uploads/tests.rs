use super::*;
use crate::drafts::store::sync::tests::Database;

pub(in crate::drafts::store) fn draft() -> Draft {
    Draft {
        account_id: "00000000-0000-4000-8000-000000000001".into(),
        team_id: "T1".into(),
        user_id: "U1".into(),
        channel_id: "C1".into(),
        thread_ts: Some("1.000001".into()),
        reply_broadcast: true,
        client_msg_id: "super-platinum-file-1000001".into(),
        optimistic_ts: "2.000001".into(),
        caption: "Caption 🦀".into(),
        attachments: vec![Attachment {
            name: "example.txt".into(),
            bytes: 17,
        }],
    }
}
#[test]
fn receipt_roundtrip_is_separate_from_local_drafts_and_contains_only_safe_display_metadata() {
    let db = Database::new();
    let expected = draft();
    let reserved = db.open().reserve_upload(&expected).unwrap();
    assert!(reserved.created);
    let finalized = db
        .open()
        .finalize_upload(&reserved.stored.ticket, vec!["F1".into()])
        .unwrap()
        .unwrap();
    // Reopening uses the same account-isolated DraftStore, not the transcript cache.
    assert_eq!(
        db.open().load_uploads(&expected.account_id).unwrap(),
        vec![finalized.clone()]
    );
    let mut store = db.open();
    let key = crate::drafts::DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    store
        .save([(
            key.clone(),
            crate::drafts::LocalDraft {
                composer: crate::ComposerState::with_text("separate draft"),
                ..Default::default()
            },
        )])
        .unwrap();
    store
        .save([(key, crate::drafts::LocalDraft::default())])
        .unwrap();
    assert_eq!(
        store.load_uploads(&expected.account_id).unwrap(),
        vec![finalized]
    );
    let value = serde_json::to_value(&expected.attachments[0]).unwrap();
    assert_eq!(value, serde_json::json!({"name":"example.txt","bytes":17}));
    assert!(
        serde_json::from_value::<Attachment>(
            serde_json::json!({"name":"x","bytes":1,"path":"/private/file"})
        )
        .is_err()
    );
}
#[test]
fn exact_retirement_compacts_rows_and_stale_completion_never_resurrects() {
    let db = Database::new();
    let mut store = db.open();
    let draft = draft();
    let first = store.reserve_upload(&draft).unwrap().stored;
    let finalized = store
        .finalize_upload(&first.ticket, vec!["F1".into()])
        .unwrap()
        .unwrap();
    assert!(!store.retire_upload(&first).unwrap());
    let mut wrong = finalized.clone();
    wrong.draft.caption.push('!');
    assert!(!store.retire_upload(&wrong).unwrap());
    assert!(store.retire_upload(&finalized).unwrap());
    assert!(!store.retire_upload(&finalized).unwrap());
    assert!(
        store
            .finalize_upload(&first.ticket, vec!["F1".into()])
            .unwrap()
            .is_none()
    );
    let mut next = draft.clone();
    next.client_msg_id.push_str("-next");
    let second = store.reserve_upload(&next).unwrap().stored;
    assert!(second.ticket.sequence() > first.ticket.sequence());
    assert!(
        store
            .finalize_upload(&first.ticket, vec!["F1".into()])
            .unwrap()
            .is_none()
    );
    assert_eq!(store.load_uploads(&draft.account_id).unwrap(), vec![second]);
    let count: i64 = store
        .0
        .query_row("SELECT count(*) FROM upload_receipts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn active_identity_is_idempotent_but_changed_draft_or_final_ids_are_rejected() {
    let db = Database::new();
    let mut store = db.open();
    let draft = draft();
    let first = store.reserve_upload(&draft).unwrap();
    let again = store.reserve_upload(&draft).unwrap();
    assert!(!again.created);
    assert_eq!(first.stored, again.stored);
    let mut changed = draft.clone();
    changed.channel_id = "C2".into();
    assert!(store.reserve_upload(&changed).is_err());
    assert!(
        store
            .finalize_upload(&first.stored.ticket, vec!["U1".into()])
            .is_err()
    );
    let finalized = store
        .finalize_upload(&first.stored.ticket, vec!["F1".into()])
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .finalize_upload(&first.stored.ticket, vec!["F1".into()])
            .unwrap(),
        Some(finalized)
    );
    assert!(
        store
            .finalize_upload(&first.stored.ticket, vec!["F2".into()])
            .is_err()
    );
    assert!(
        store
            .finalize_upload(&first.stored.ticket, vec!["F1".into(), "F1".into()])
            .is_err()
    );
}
#[test]
fn account_and_journal_incarnation_isolate_identical_sequence_numbers() {
    let a = Database::new();
    let b = Database::new();
    let draft = draft();
    let first = a.open().reserve_upload(&draft).unwrap().stored;
    let replacement = b.open().reserve_upload(&draft).unwrap().stored;
    assert_eq!(first.ticket.sequence(), replacement.ticket.sequence());
    assert_ne!(first.ticket, replacement.ticket);
    assert!(
        b.open()
            .finalize_upload(&first.ticket, vec!["F1".into()])
            .unwrap()
            .is_none()
    );
    assert!(!b.open().retire_upload(&first).unwrap());
    let mut other = draft.clone();
    other.account_id = "00000000-0000-4000-8000-000000000002".into();
    assert!(a.open().reserve_upload(&other).is_err());
    assert!(a.open().load_uploads(&other.account_id).is_err());
    assert_eq!(
        b.open().load_uploads(&draft.account_id).unwrap(),
        vec![replacement]
    );
}
#[test]
fn capacity_limits_only_active_records_and_retirement_frees_space() {
    let db = Database::new();
    let mut store = db.open();
    let original = draft();
    let mut first = None;
    for n in 0..MAX_ACTIVE {
        let mut value = original.clone();
        value.client_msg_id = format!("file-{n}");
        let row = store.reserve_upload(&value).unwrap().stored;
        if n == 0 {
            first = Some(row);
        }
    }
    assert!(store.reserve_upload(&original).is_err());
    assert!(store.retire_upload(&first.unwrap()).unwrap());
    assert!(store.reserve_upload(&original).unwrap().created);
    assert_eq!(
        store.load_uploads(&original.account_id).unwrap().len(),
        MAX_ACTIVE
    );
}
#[test]
fn malformed_metadata_and_unknown_disk_version_fail_closed() {
    let db = Database::new();
    let mut store = db.open();
    let valid = draft();
    let mut bad = valid.clone();
    bad.attachments[0].name = "/private/path".into();
    assert!(store.reserve_upload(&bad).is_err());
    bad = valid.clone();
    bad.thread_ts = None;
    assert!(store.reserve_upload(&bad).is_err());
    bad = valid.clone();
    bad.user_id = "token-value".into();
    assert!(store.reserve_upload(&bad).is_err());
    bad = valid.clone();
    bad.optimistic_ts = "NaN".into();
    assert!(store.reserve_upload(&bad).is_err());
    store.reserve_upload(&valid).unwrap();
    store
        .0
        .execute("UPDATE upload_receipts SET format_version=99", [])
        .unwrap();
    assert!(store.load_uploads(&valid.account_id).is_err());
}

#[test]
fn self_user_and_workspace_identity_survive_load_without_reassignment() {
    let db = Database::new();
    let mut store = db.open();
    let mut first = draft();
    // These are legal Unix filename characters; display metadata is inert text,
    // never used as a filesystem path or injected markup.
    first.attachments[0].name = "name\\with\ttab.txt".into();
    let a = store.reserve_upload(&first).unwrap().stored;
    let mut second = first.clone();
    second.user_id = "U2".into();
    second.team_id = "T2".into();
    second.client_msg_id.push_str("-second");
    let b = store.reserve_upload(&second).unwrap().stored;
    let loaded = store.load_uploads(&first.account_id).unwrap();
    assert_eq!(loaded, vec![a.clone(), b.clone()]);
    // The desktop must filter all three owner identities before presentation;
    // loading the account never relabels a different user's receipt as current.
    assert_eq!(
        loaded
            .iter()
            .filter(|v| v.draft.team_id == "T1" && v.draft.user_id == "U1")
            .collect::<Vec<_>>(),
        vec![&a]
    );
    assert_eq!(
        loaded
            .iter()
            .filter(|v| v.draft.team_id == "T2" && v.draft.user_id == "U2")
            .collect::<Vec<_>>(),
        vec![&b]
    );
}
#[test]
fn reconstructed_database_does_not_reuse_old_ticket_even_when_sequence_restarts() {
    let db = Database::new();
    let value = draft();
    let old = db.open().reserve_upload(&value).unwrap().stored;
    std::fs::remove_file(&db.path).unwrap();
    let fresh = db.open().reserve_upload(&value).unwrap().stored;
    assert_eq!(old.ticket.sequence(), fresh.ticket.sequence());
    assert_ne!(old.ticket, fresh.ticket);
    assert!(!db.open().retire_upload(&old).unwrap());
    assert!(
        db.open()
            .finalize_upload(&old.ticket, vec!["F1".into()])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.open().load_uploads(&value.account_id).unwrap(),
        vec![fresh]
    );
}
