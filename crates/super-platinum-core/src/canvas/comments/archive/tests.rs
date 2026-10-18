use super::*;
use serde_json::json;
pub(crate) fn file() -> File {
    serde_json::from_value(
        json!({"id":"F123","filetype":"quip","access":"write","is_locked":false}),
    )
    .unwrap()
}
pub(crate) fn root(value: bool) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"C123","ts":"1700000000.000100","subtype":"document_comment_root","reply_count":1,"text":"Original context","document_comment":{"thread_id":"quip-thread-1","is_archived":value}})).unwrap()
}
#[test]
fn exact_resolve_and_reopen_keep_both_thread_identities() {
    let intent = ArchiveIntent::capture(&file(), &root(false)).unwrap();
    assert_eq!(intent.method(), "quip.thread.archive");
    assert_eq!(intent.root_ts(), "1700000000.000100");
    let accepted = intent
        .accept(&json!({"ok":true,"message":root(true)}))
        .unwrap();
    accepted.readback(&file(), &root(true)).unwrap();
    assert!(accepted.readback(&file(), &root(false)).is_err());
    assert_eq!(
        ArchiveIntent::capture(&file(), &root(true))
            .unwrap()
            .method(),
        "quip.thread.unarchive"
    );
}
#[test]
fn comment_only_locked_empty_or_wrong_file_is_not_archive_authority() {
    for access in ["read", "comment", ""] {
        let mut file = file();
        file.extra.insert("access".into(), json!(access));
        assert!(ArchiveIntent::capture(&file, &root(false)).is_err());
    }
    let mut file = file();
    file.extra.insert("is_locked".into(), json!(true));
    assert!(ArchiveIntent::capture(&file, &root(false)).is_err());
    let mut message = root(false);
    message.reply_count = Some(0);
    assert!(ArchiveIntent::capture(&super::tests::file(), &message).is_err());
    message = root(false);
    message.channel = Some("Cforeign".into());
    assert!(ArchiveIntent::capture(&super::tests::file(), &message).is_err());
}
#[test]
fn changed_root_and_permission_refuse_before_dispatch() {
    let intent = ArchiveIntent::capture(&file(), &root(false)).unwrap();
    let mut changed = root(false);
    changed.text = Some("Edited context".into());
    assert!(intent.revalidate(&file(), &changed).is_err());
    assert!(intent.revalidate(&file(), &root(true)).is_err());
    let mut changed_file = file();
    changed_file.extra.insert("access".into(), json!("comment"));
    assert!(intent.revalidate(&changed_file, &root(false)).is_err());
}
#[test]
fn malformed_or_foreign_ack_never_creates_success() {
    for response in [
        json!({"ok":false,"error":"unknown_code"}),
        json!({"ok":true}),
        json!({"ok":true,"error":"warning","message":root(true)}),
        json!({"ok":true,"message":root(false)}),
    ] {
        assert!(
            ArchiveIntent::capture(&file(), &root(false))
                .unwrap()
                .accept(&response)
                .is_err()
        );
    }
    let mut changed = root(true);
    changed.extra.get_mut("document_comment").unwrap()["thread_id"] = json!("foreign");
    assert!(
        ArchiveIntent::capture(&file(), &root(false))
            .unwrap()
            .accept(&json!({"ok":true,"message":changed}))
            .is_err()
    );
}
#[test]
fn inert_record_preserves_original_root_and_cannot_change_requested_outcome() {
    let record = ArchiveIntent::capture(&file(), &root(false))
        .unwrap()
        .record("T1", "U1")
        .unwrap();
    let encoded = serde_json::to_value(&record).unwrap();
    let decoded: ArchiveRecord = serde_json::from_value(encoded.clone()).unwrap();
    decoded.validate().unwrap();
    assert!(decoded == record);
    let mut invalid = encoded;
    invalid["desired"] = json!(false);
    assert!(
        serde_json::from_value::<ArchiveRecord>(invalid)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn cheap_action_label_is_permission_and_identity_bound_but_not_dispatch_authority() {
    assert_eq!(ArchiveIntent::action(&file(), &root(false)), Some(true));
    assert_eq!(ArchiveIntent::action(&file(), &root(true)), Some(false));
    let mut other = root(false);
    other.channel = Some("Cforeign".into());
    assert_eq!(ArchiveIntent::action(&file(), &other), None);
    let mut readonly = file();
    readonly.extra.insert("access".into(), json!("comment"));
    assert_eq!(ArchiveIntent::action(&readonly, &root(false)), None);
}
