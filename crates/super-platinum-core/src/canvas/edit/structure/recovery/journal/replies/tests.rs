use super::*;
use serde_json::json;
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("canvas-reply-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("reply.sqlite"));
        let _ = std::fs::remove_dir(&self.0);
    }
}
const ROOT: &str = "1700000000.000100";
fn setup() -> (Temp, Journal, Scope) {
    let dir = Temp::new();
    let journal = Journal::open(
        dir.path().join("reply.sqlite"),
        "147b5480-9f1a-4443-89da-836472cbb209",
    )
    .unwrap();
    (
        dir,
        journal,
        Scope {
            workspace: "T1".into(),
            user: "U1".into(),
            file: "F123".into(),
        },
    )
}
fn save(j: &mut Journal, s: &Scope, prior: Option<&ReplyDraft>, text: &str) -> ReplyDraft {
    let ReplySave::Stored(d) = j
        .save_reply_draft(s, ROOT, prior, &uuid::Uuid::new_v4().to_string(), text)
        .unwrap()
    else {
        panic!("stored")
    };
    d
}
fn intent(s: &Scope, text: &str) -> crate::canvas::comments::reply::ReplyIntent {
    let f =
        serde_json::from_value(json!({"id":s.file,"filetype":"quip","access":"comment"})).unwrap();
    let r=serde_json::from_value(json!({"type":"message","channel":"C123","ts":ROOT,"subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"quip-thread"}})).unwrap();
    crate::canvas::comments::reply::ReplyIntent::capture(
        &s.workspace,
        &s.user,
        &f,
        &r,
        text,
        &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap()
}
fn ack(i: crate::canvas::comments::reply::ReplyIntent) -> ReplyAccepted {
    let raw = json!({"ok":true,"channel":"C123","ts":"1700000001.000100","message":{"type":"message","channel":"C123","thread_ts":ROOT,"ts":"1700000001.000100","text":i.record().text(),"client_msg_id":i.record().client_msg_id()}});
    i.accept(&raw).unwrap()
}
#[test]
fn stale_writer_preserves_both_versions_and_review_fences_retry() {
    let (_d, mut j, s) = setup();
    let a = save(&mut j, &s, None, "before");
    let b = save(&mut j, &s, Some(&a), "first writer");
    let c = save(&mut j, &s, Some(&a), "second writer");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    assert_eq!(inv.drafts().len(), 2);
    assert!(j.choose_reply_version(&inv, &b).unwrap().is_some());
    assert!(matches!(
        j.save_reply_draft(&s, ROOT, Some(&a), c.capture(), c.text())
            .unwrap(),
        ReplySave::Retired
    ));
    assert_eq!(j.reply_inventory(&s, ROOT).unwrap().drafts(), &[b]);
}
#[test]
fn restart_restores_unknown_receipt_and_cannot_readmit() {
    let (d, mut j, s) = setup();
    save(&mut j, &s, None, "original");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    let i = intent(&s, "original");
    let record = i.record().clone();
    let r = j.begin_reply(&inv, record.clone()).unwrap().unwrap();
    drop(j);
    let mut j = Journal::open(
        d.path().join("reply.sqlite"),
        "147b5480-9f1a-4443-89da-836472cbb209",
    )
    .unwrap();
    let restored = j.reply_inventory(&s, ROOT).unwrap();
    assert_eq!(restored.attempt().unwrap().id(), r.id());
    assert!(j.begin_reply(&restored, record).is_err());
    assert!(j.reply_not_sent(&r).unwrap());
    assert!(j.reply_not_sent(&r).unwrap());
    assert_eq!(
        j.reply_inventory(&s, ROOT).unwrap().drafts()[0].text(),
        "original"
    );
}
#[test]
fn acknowledged_send_clears_only_exact_predecessor_and_keeps_newer_edit() {
    let (_d, mut j, s) = setup();
    let old = save(&mut j, &s, None, "sent");
    let i = intent(&s, "sent");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    let r = j.begin_reply(&inv, i.record().clone()).unwrap().unwrap();
    let new = save(&mut j, &s, Some(&old), "newer local");
    let a = ack(i);
    assert!(j.reply_acknowledged(&r, &a).unwrap());
    assert!(j.reply_acknowledged(&r, &a).unwrap());
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    assert!(inv.attempt().is_none());
    assert_eq!(inv.drafts(), &[new]);
    assert_eq!(
        j.reply_history(&s, None).unwrap()[0]
            .receipt
            .record()
            .text(),
        "sent"
    );
}
#[test]
fn explicit_unknown_review_retains_original_and_late_ack_does_not_clear_new_draft() {
    let (_d, mut j, s) = setup();
    save(&mut j, &s, None, "uncertain");
    let i = intent(&s, "uncertain");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    let r = j.begin_reply(&inv, i.record().clone()).unwrap().unwrap();
    let observed=serde_json::from_value(json!({"type":"message","ts":ROOT,"subtype":"document_comment_root","document_comment":{"thread_id":"quip-thread"}})).unwrap();
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    assert!(j.archive_unconfirmed_reply(&inv, &observed).unwrap());
    let next = save(&mut j, &s, None, "new draft");
    assert!(j.reply_acknowledged(&r, &ack(i)).unwrap());
    assert_eq!(j.reply_inventory(&s, ROOT).unwrap().drafts(), &[next]);
    assert_eq!(
        j.reply_history(&s, None).unwrap()[0].status,
        "reviewed_outcome_unknown"
    );
}
#[test]
fn unrelated_thread_write_does_not_invalidate_inventory_content_cas() {
    let (_d, mut j, s) = setup();
    let a = save(&mut j, &s, None, "original");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    j.save_reply_draft(
        &s,
        "1700000005.000100",
        None,
        &uuid::Uuid::new_v4().to_string(),
        "other",
    )
    .unwrap();
    assert!(j.choose_reply_version(&inv, &a).unwrap().is_some());
}
#[test]
fn wrong_owner_and_capacity_fail_closed() {
    let (_d, mut j, s) = setup();
    let a = save(&mut j, &s, None, "before");
    for n in 0..15 {
        save(&mut j, &s, None, &format!("version{n}"));
    }
    assert!(
        j.save_reply_draft(
            &s,
            ROOT,
            Some(&ReplyDraft { revision: 99, ..a }),
            &uuid::Uuid::new_v4().to_string(),
            "retained outside store"
        )
        .is_err()
    );
    assert_eq!(j.reply_inventory(&s, ROOT).unwrap().drafts().len(), 16);
    let mut foreign = s.clone();
    foreign.user = "other".into();
    assert!(
        j.reply_inventory(&foreign, ROOT)
            .unwrap()
            .drafts()
            .is_empty()
    );
}

#[test]
fn same_ts_different_quip_thread_cannot_dispose_unknown_attempt() {
    let (_d, mut j, s) = setup();
    save(&mut j, &s, None, "original");
    let i = intent(&s, "original");
    let before = j.reply_inventory(&s, ROOT).unwrap();
    j.begin_reply(&before, i.record().clone()).unwrap().unwrap();
    let before = j.reply_inventory(&s, ROOT).unwrap();
    let foreign=serde_json::from_value(json!({"type":"message","ts":ROOT,"subtype":"document_comment_root","document_comment":{"thread_id":"foreign"}})).unwrap();
    assert!(j.archive_unconfirmed_reply(&before, &foreign).is_err());
    assert!(j.reply_inventory(&s, ROOT).unwrap() == before);
}
#[test]
fn corrupt_receipt_and_account_reopen_fail_closed() {
    let (d, mut j, s) = setup();
    save(&mut j, &s, None, "original");
    let i = intent(&s, "original");
    let before = j.reply_inventory(&s, ROOT).unwrap();
    j.begin_reply(&before, i.record().clone()).unwrap().unwrap();
    j.conn
        .execute("UPDATE reply_attempts SET body='{}'", [])
        .unwrap();
    assert!(j.reply_inventory(&s, ROOT).is_err());
    assert!(
        Journal::open(
            d.path().join("reply.sqlite"),
            "21381bbf-dccd-4e51-98aa-73fc9b4d971e"
        )
        .is_err()
    );
}
#[test]
fn explicit_empty_draft_disposal_fences_delayed_writer() {
    let (_d, mut j, s) = setup();
    let a = save(&mut j, &s, None, "text");
    let before = j.reply_inventory(&s, ROOT).unwrap();
    assert!(j.discard_reply_draft(&before).unwrap());
    assert!(matches!(
        j.save_reply_draft(&s, ROOT, None, a.capture(), a.text())
            .unwrap(),
        ReplySave::Retired
    ));
    assert!(j.reply_inventory(&s, ROOT).unwrap().drafts().is_empty());
}

#[test]
fn current_schema_missing_clock_is_not_repaired_into_fresh_authority() {
    let (d, j, _) = setup();
    j.conn.execute("DELETE FROM reply_clock", []).unwrap();
    drop(j);
    assert!(
        Journal::open(
            d.path().join("reply.sqlite"),
            "147b5480-9f1a-4443-89da-836472cbb209"
        )
        .is_err()
    );
}
#[test]
fn shared_file_lock_blocks_structure_and_resolve_reads() {
    let (_d, mut j, s) = setup();
    save(&mut j, &s, None, "original");
    let i = intent(&s, "original");
    let inv = j.reply_inventory(&s, ROOT).unwrap();
    j.begin_reply(&inv, i.record().clone()).unwrap().unwrap();
    assert!(j.get(&s).is_err());
    assert!(j.comment(&s).is_err());
}

mod rich;
