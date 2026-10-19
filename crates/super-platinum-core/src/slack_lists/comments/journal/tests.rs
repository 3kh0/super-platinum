use super::*;
use crate::slack_lists::{
    Document, Record,
    comments::{ReplyIntent, ReplyPolicy},
};
use serde_json::json;
const ACCOUNT: &str = "11111111-2222-4333-8444-555555555555";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("list-comment-journal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        for name in ["j", "journal.sqlite"] {
            let _ = std::fs::remove_file(self.0.join(name));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}
fn record() -> ReplyRecord {
    let d=Document::parse(json!({"id":"F01234567","mode":"list","access":"read","list_metadata":{"schema":[],"views":[]}})).unwrap();
    let r = Record::parse(json!({"id":"Rec1","fields":[],"date_created":123,"created_by":"UTEST"}))
        .unwrap();
    let p=Page::parse(&Target::capture(&d,&r).unwrap(),json!({"ok":true,"messages":[{"type":"message","subtype":"list_record_comment","ts":"1700000000.000001","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}}]})).unwrap();
    ReplyIntent::capture_rich("TTEST","UTEST",p.target(),p.root().unwrap(),&ReplyPolicy::observed(json!({"id":"C01234567"}),false,true),vec![json!({"type":"rich_text","opaque":7,"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review\n","style":{"bold":true}},{"type":"user","user_id":"U1","opaque":{"keep":true}}]}]})],&uuid::Uuid::new_v4().to_string(),"1700000001.000001").unwrap().record().clone()
}
fn scope(r: &ReplyRecord) -> Scope {
    Scope::capture(r.workspace(), r.user(), r.target()).unwrap()
}
fn ack(r: &ReplyRecord) -> Value {
    json!({"type":"message","ts":"1700000002.000001","thread_ts":r.target().root_ts(),"user":r.user(),"client_msg_id":r.client_msg_id(),"blocks":r.blocks()})
}
fn saved(j: &mut Journal, r: &ReplyRecord) -> Inventory {
    let i = j.inventory(&scope(r)).unwrap();
    j.save(&i, &uuid::Uuid::new_v4().to_string(), r.blocks().to_vec())
        .unwrap()
}
#[test]
fn exact_rich_draft_and_inert_attempt_survive_reopen() {
    let dir = Temp::new();
    let path = dir.path().join("journal.sqlite");
    let r = record();
    {
        let mut j = Journal::open(&path, ACCOUNT).unwrap();
        let i = saved(&mut j, &r);
        j.begin(&i, r.clone()).unwrap();
    }
    let mut j = Journal::open(&path, ACCOUNT).unwrap();
    let i = j.inventory(&scope(&r)).unwrap();
    assert_eq!(i.blocks(), r.blocks());
    assert_eq!(i.attempt().unwrap().record(), &r);
    assert!(j.begin(&i, r.clone()).is_err());
    let i = j
        .resolve(&scope(&r), &r, Resolution::Accepted(ack(&r)))
        .unwrap();
    assert!(i.blocks().is_empty());
    assert!(i.attempt().is_none());
}
#[test]
fn cas_and_uuid_history_prevent_duplicate_send() {
    let dir = Temp::new();
    let mut j = Journal::open(dir.path().join("j"), ACCOUNT).unwrap();
    let r = record();
    let i = saved(&mut j, &r);
    assert!(
        j.save(
            &j.inventory(&scope(&r)).unwrap(),
            &uuid::Uuid::new_v4().to_string(),
            r.blocks().to_vec()
        )
        .is_ok()
    );
    assert!(j.begin(&i, r.clone()).is_err());
    let i = j.inventory(&scope(&r)).unwrap();
    j.begin(&i, r.clone()).unwrap();
    let i = j.resolve(&scope(&r), &r, Resolution::NotRequested).unwrap();
    assert!(j.begin(&i, r.clone()).is_err());
    assert_eq!(i.blocks(), r.blocks());
}
#[test]
fn accepted_old_attempt_cannot_clear_newer_even_identical_capture() {
    for identical in [false, true] {
        let dir = Temp::new();
        let mut j = Journal::open(dir.path().join("j"), ACCOUNT).unwrap();
        let r = record();
        let i = saved(&mut j, &r);
        let i = j.begin(&i, r.clone()).unwrap();
        let blocks = if identical {
            r.blocks().to_vec()
        } else {
            vec![json!({"type":"unknown_future","opaque":[1,2,3]})]
        };
        let i = j
            .save(&i, &uuid::Uuid::new_v4().to_string(), blocks.clone())
            .unwrap();
        let capture = i.capture().to_owned();
        let i = j
            .resolve(&scope(&r), &r, Resolution::Accepted(ack(&r)))
            .unwrap();
        assert_eq!(i.blocks(), blocks);
        assert_eq!(i.capture(), capture);
        assert!(i.attempt().is_none());
    }
}
#[test]
fn uncertain_and_malformed_resolution_never_unlock() {
    let dir = Temp::new();
    let mut j = Journal::open(dir.path().join("j"), ACCOUNT).unwrap();
    let r = record();
    let i = saved(&mut j, &r);
    j.begin(&i, r.clone()).unwrap();
    let raw = json!({"ok":true,"opaque":"unconfirmed"});
    j.resolve(&scope(&r), &r, Resolution::Uncertain(Some(raw.clone())))
        .unwrap();
    let i = j.inventory(&scope(&r)).unwrap();
    assert_eq!(i.attempt().unwrap().raw(), Some(&raw));
    let mut wrong = ack(&r);
    wrong["client_msg_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert!(
        j.resolve(&scope(&r), &r, Resolution::Accepted(wrong))
            .is_err()
    );
    assert!(
        j.resolve(&scope(&r), &r, Resolution::Rejected(json!({"ok":true})))
            .is_err()
    );
    assert!(j.inventory(&scope(&r)).unwrap().attempt().is_some());
    let i = j
        .resolve(
            &scope(&r),
            &r,
            Resolution::Rejected(json!({"ok":false,"error":"not_allowed"})),
        )
        .unwrap();
    assert!(i.attempt().is_none());
    assert_eq!(i.blocks(), r.blocks());
}
#[test]
fn scope_ignores_mutable_labels_but_separates_incarnation_and_root() {
    let r = record();
    let s = scope(&r);
    let mut row = r.target().record_raw().clone();
    row["label"] = json!("renamed");
    row["thread_ts"] = json!(r.target().root_ts());
    let target = r
        .target()
        .refresh(
            &json!({"ok":true,"file":r.target().document_raw()}),
            &json!({"ok":true,"record":row}),
            false,
        )
        .unwrap();
    // Scope itself deliberately records creation identity and root, never mutable row labels.
    assert!(!serde_json::to_string(&s).unwrap().contains("fields"));
    let mut changed = s.clone();
    changed.root = "1700000003.000001".into();
    assert_ne!(s, changed);
    changed = s.clone();
    changed.created = Some(json!(124));
    assert_ne!(s, changed);
    assert_eq!(s, Scope::capture(r.workspace(), r.user(), &target).unwrap());
}
#[test]
fn wrong_account_corrupt_and_oversized_storage_refuse_without_overwrite() {
    let dir = Temp::new();
    let path = dir.path().join("j");
    let r = record();
    let mut j = Journal::open(&path, ACCOUNT).unwrap();
    let i = saved(&mut j, &r);
    assert!(Journal::open(&path, "99999999-2222-4333-8444-555555555555").is_err());
    assert!(
        j.save(
            &i,
            &uuid::Uuid::new_v4().to_string(),
            vec![json!("x".repeat(MAX_BODY))]
        )
        .is_err()
    );
    assert_eq!(j.inventory(&scope(&r)).unwrap().blocks(), r.blocks());
    j.conn.execute("UPDATE drafts SET body='{'", []).unwrap();
    assert!(j.inventory(&scope(&r)).is_err());
    assert!(
        j.save(&i, &uuid::Uuid::new_v4().to_string(), vec![])
            .is_err()
    );
}
