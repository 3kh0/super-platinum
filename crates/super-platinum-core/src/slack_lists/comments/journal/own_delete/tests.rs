use super::*;
use crate::canvas::comments::content;
use crate::slack_lists::{
    Document, Record,
    comments::{
        OwnDeleteIntent, OwnDeletePermissionQuery, OwnDeletePolicy, OwnEditIntent, OwnEditPolicy,
        OwnEditSeed,
    },
};
use serde_json::json;
use std::path::PathBuf;
const ACCOUNT: &str = "11111111-2222-4333-8444-555555555555";
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("list-own-delete-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
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
fn fixture() -> (Page, OwnDeleteRecord, OwnDeleteAccepted) {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","fields":[],"date_created":1,"created_by":"U1"})).unwrap();
    let page=Page::parse(&Target::capture(&doc,&row).unwrap(),json!({"ok":true,"has_more":true,"response_metadata":{"next_cursor":"original-cursor"},
        "messages":[{"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":"1700000001.000001","thread_ts":"1700000000.000001","user":"U1","type":"message","text":"Original", "blocks":content::plain("Original")}]})).unwrap();
    let permission = OwnDeletePermissionQuery::capture("T1", "U1", "C01234567", false)
        .unwrap()
        .accept(json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"]}))
        .unwrap();
    let policy = OwnDeletePolicy::observed(
        "T1",
        "U1",
        json!({"id":"C01234567"}),
        true,
        false,
        false,
        &permission,
    )
    .unwrap();
    let intent = OwnDeleteIntent::capture("T1", "U1", &page, &page.messages()[1], &policy).unwrap();
    let record = intent.record().clone();
    let accepted = intent
        .accept(json!({"ok":true,"channel":"C01234567","ts":record.ts(),"opaque_ack":{"keep":7}}))
        .unwrap();
    (page, record, accepted)
}
fn begin(j: &mut Journal, record: &OwnDeleteRecord, aid: &str) -> DeleteInventory {
    let scope = scope_for(record).unwrap();
    let initial = j.delete_inventory(&scope, record.ts()).unwrap();
    j.delete_begin(&initial, aid, record).unwrap()
}
fn deleted(page: &Page, ts: &str) -> Page {
    let mut raw = page.raw().clone();
    raw["messages"]
        .as_array_mut()
        .unwrap()
        .retain(|m| m["ts"] != ts);
    raw["deleted"] = json!([ts]);
    raw["opaque_readback"] = json!("original");
    Page::parse(page.target(), raw).unwrap()
}

#[test]
fn actual_reopen_retains_pending_without_recreating_delete_authority() {
    let temp = Temp::new();
    let (_, record, _) = fixture();
    let aid = id();
    let expected = {
        let mut j = temp.open();
        begin(&mut j, &record, &aid)
    };
    let mut j = temp.open();
    let restored = j.delete_inventory(expected.scope(), record.ts()).unwrap();
    assert_eq!(restored, expected);
    assert_eq!(restored.attempt().unwrap().phase(), DeletePhase::Pending);
    assert_eq!(restored.attempt().unwrap().record(), &record);
    assert_eq!(restored.attempt().unwrap().id(), aid);
    assert!(restored.attempt().unwrap().phase().locked());
    assert!(j.delete_begin(&restored, &aid, &record).is_err());
    assert!(j.delete_begin(&restored, &id(), &record).is_err());
    let decoded: DeleteInventory =
        serde_json::from_value(serde_json::to_value(&restored).unwrap()).unwrap();
    assert_eq!(decoded, restored);
}

#[test]
fn uncertain_original_receipt_survives_reopen_and_positive_readback_without_ack() {
    let temp = Temp::new();
    let (page, record, _) = fixture();
    let aid = id();
    let scope = scope_for(&record).unwrap();
    let raw = json!({"ok":false,"error":"internal_error","opaque":{"original":9}});
    let saved = {
        let mut j = temp.open();
        begin(&mut j, &record, &aid);
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Uncertain(Some(&raw)),
            77,
        )
        .unwrap()
    };
    let mut j = temp.open();
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), saved);
    assert!(j.delete_begin(&saved, &id(), &record).is_err());
    let reject = json!({"ok":false,"error":"cant_delete_message"});
    for outcome in [
        DeleteResolution::NotRequested,
        DeleteResolution::Rejected(&reject),
        DeleteResolution::RateLimited,
    ] {
        assert!(
            j.delete_resolve(&scope, record.ts(), &aid, outcome, 99)
                .is_err()
        );
    }
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), saved);
    let observed = deleted(&page, record.ts());
    let terminal = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Observed(&observed),
            0,
        )
        .unwrap();
    let a = terminal.attempt().unwrap();
    assert_eq!(a.phase(), DeletePhase::Observed);
    assert_eq!(a.raw(), Some(&raw));
    assert_eq!(a.readback(), Some(observed.raw()));
    assert_eq!(a.retry_at(), 77);
    assert_eq!(terminal.retry_at(), 77);
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Observed(&observed),
            0
        )
        .unwrap(),
        terminal
    );
    assert!(j.delete_begin(&terminal, &id(), &record).is_err());
    drop(j);
    assert_eq!(
        temp.open().delete_inventory(&scope, record.ts()).unwrap(),
        terminal
    );
}

#[test]
fn confirmed_original_ack_is_terminal_exact_idempotent_and_cannot_begin_again() {
    let temp = Temp::new();
    let (_, record, accepted) = fixture();
    let scope = scope_for(&record).unwrap();
    let aid = id();
    let mut j = temp.open();
    begin(&mut j, &record, &aid);
    let terminal = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0,
        )
        .unwrap();
    assert_eq!(terminal.attempt().unwrap().raw(), Some(accepted.raw()));
    assert!(terminal.attempt().unwrap().readback().is_none());
    assert_eq!(terminal.attempt().unwrap().phase(), DeletePhase::Confirmed);
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0
        )
        .unwrap(),
        terminal
    );
    assert!(
        j.delete_resolve(&scope, record.ts(), &aid, DeleteResolution::NotRequested, 0)
            .is_err()
    );
    assert!(j.delete_begin(&terminal, &id(), &record).is_err());
    drop(j);
    assert_eq!(
        temp.open().delete_inventory(&scope, record.ts()).unwrap(),
        terminal
    );
}

#[test]
fn stale_cas_id_reuse_and_old_resolution_cannot_replace_newer_attempt() {
    let temp = Temp::new();
    let (_, record, _) = fixture();
    let scope = scope_for(&record).unwrap();
    let mut j = temp.open();
    let initial = j.delete_inventory(&scope, record.ts()).unwrap();
    let first = id();
    j.delete_begin(&initial, &first, &record).unwrap();
    let done = j
        .delete_resolve(
            &scope,
            record.ts(),
            &first,
            DeleteResolution::NotRequested,
            100,
        )
        .unwrap();
    assert!(j.delete_begin(&initial, &id(), &record).is_err());
    assert!(j.delete_begin(&done, &first, &record).is_err());
    let newer_id = id();
    let newer = j.delete_begin(&done, &newer_id, &record).unwrap();
    assert_eq!(newer.retry_at(), 100);
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &first,
            DeleteResolution::NotRequested,
            0
        )
        .unwrap(),
        newer
    );
    assert!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &first,
            DeleteResolution::NotRequested,
            101
        )
        .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), newer);
    assert_eq!(
        j.delete_attempt(&scope, record.ts(), &first)
            .unwrap()
            .unwrap()
            .phase(),
        DeletePhase::NotSent
    );
}

#[test]
fn explicit_rate_limit_retains_issued_disposition_and_monotonic_deadline() {
    let temp = Temp::new();
    let (_, record, _) = fixture();
    let scope = scope_for(&record).unwrap();
    let aid = id();
    let mut j = temp.open();
    let pending = begin(&mut j, &record, &aid);
    assert!(
        j.delete_resolve(&scope, record.ts(), &aid, DeleteResolution::RateLimited, 0)
            .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), pending);
    let limited = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::RateLimited,
            500,
        )
        .unwrap();
    assert_eq!(limited.attempt().unwrap().phase(), DeletePhase::RateLimited);
    assert!(limited.attempt().unwrap().raw().is_none());
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::RateLimited,
            400
        )
        .unwrap(),
        limited
    );
    let later = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::RateLimited,
            600,
        )
        .unwrap();
    assert_eq!(later.retry_at(), 600);
    let next = j.delete_begin(&later, &id(), &record).unwrap();
    assert_eq!(next.retry_at(), 600);
}

#[test]
fn rejection_whitelist_conflicting_receipts_and_foreign_scope_are_atomic() {
    let temp = Temp::new();
    let (_, record, _) = fixture();
    let scope = scope_for(&record).unwrap();
    let aid = id();
    let mut j = temp.open();
    let initial = begin(&mut j, &record, &aid);
    for error in [
        "internal_error",
        "fatal_error",
        "request_timeout",
        "unknown_error",
    ] {
        assert!(
            j.delete_resolve(
                &scope,
                record.ts(),
                &aid,
                DeleteResolution::Rejected(&json!({"ok":false,"error":error})),
                0
            )
            .is_err()
        );
    }
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), initial);
    let raw = json!({"ok":false,"error":"cant_delete_message","opaque":1});
    let rejected = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Rejected(&raw),
            0,
        )
        .unwrap();
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Rejected(&raw),
            0
        )
        .unwrap(),
        rejected
    );
    assert!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Rejected(
                &json!({"ok":false,"error":"cant_delete_message","opaque":2})
            ),
            0
        )
        .is_err()
    );
    let mut foreign = scope.clone();
    foreign.user = "UOTHER".into();
    assert!(j.delete_attempt(&foreign, record.ts(), &aid).is_err());
    assert!(
        j.delete_resolve(
            &foreign,
            record.ts(),
            &aid,
            DeleteResolution::NotRequested,
            0
        )
        .is_err()
    );
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), rejected);
}

#[test]
fn positive_observation_rejects_absence_live_and_foreign_tombstone_contradictions() {
    let temp = Temp::new();
    let (page, record, _) = fixture();
    let scope = scope_for(&record).unwrap();
    let aid = id();
    let mut j = temp.open();
    let pending = begin(&mut j, &record, &aid);
    let mut absent = page.raw().clone();
    absent["messages"].as_array_mut().unwrap().remove(1);
    let mut live = page.raw().clone();
    live["deleted"] = json!([record.ts()]);
    let mut foreign = live.clone();
    foreign["messages"][1] = json!({"ts":record.ts(),"thread_ts":record.target().root_ts(),"user":"UOTHER","subtype":"tombstone"});
    for raw in [absent, live, foreign] {
        let observed = Page::parse(page.target(), raw).unwrap();
        assert!(
            j.delete_resolve(
                &scope,
                record.ts(),
                &aid,
                DeleteResolution::Observed(&observed),
                0
            )
            .is_err()
        );
        assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), pending);
    }
    let mut raw = page.raw().clone();
    raw["messages"][1] =
        json!({"ts":record.ts(),"thread_ts":record.target().root_ts(),"subtype":"tombstone"});
    let tombstone = Page::parse(page.target(), raw).unwrap();
    assert_eq!(
        j.delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Observed(&tombstone),
            0
        )
        .unwrap()
        .attempt()
        .unwrap()
        .phase(),
        DeletePhase::Observed
    );
}

#[test]
fn later_reply_and_own_edit_buffers_history_survive_every_delete_operation() {
    let temp = Temp::new();
    let (page, record, accepted) = fixture();
    let scope = scope_for(&record).unwrap();
    let mut j = temp.open();
    let initial = j.inventory(&scope).unwrap();
    let reply = j
        .save(&initial, &id(), content::plain("Independent reply A"))
        .unwrap();
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
    let edit_zero = j.edit_inventory(&scope, record.ts()).unwrap();
    let edit = j
        .edit_save(
            &edit_zero,
            &seed,
            &id(),
            content::plain("Independent edit B"),
        )
        .unwrap();
    let edit_intent = OwnEditIntent::capture(
        &seed,
        &page.messages()[1],
        &policy,
        content::plain("Independent edit B"),
        1700000002000,
    )
    .unwrap();
    let edit_id = id();
    let edit = j.edit_begin(&edit, &edit_id, edit_intent.record()).unwrap();
    let aid = id();
    begin(&mut j, &record, &aid);
    let reply = j
        .save(&reply, &id(), content::plain("Newer independent reply A"))
        .unwrap();
    let edit = j
        .edit_save(
            &edit,
            &seed,
            &id(),
            content::plain("Newer independent edit C"),
        )
        .unwrap();
    let history = j.edit_attempt(&scope, record.ts(), &edit_id).unwrap();
    let terminal = j
        .delete_resolve(
            &scope,
            record.ts(),
            &aid,
            DeleteResolution::Confirmed(&accepted),
            0,
        )
        .unwrap();
    assert_eq!(j.inventory(&scope).unwrap().blocks(), reply.blocks());
    assert_eq!(j.edit_inventory(&scope, record.ts()).unwrap(), edit);
    assert_eq!(
        j.edit_attempt(&scope, record.ts(), &edit_id).unwrap(),
        history
    );
    drop(j);
    let j = temp.open();
    assert_eq!(j.inventory(&scope).unwrap().blocks(), reply.blocks());
    assert_eq!(j.edit_inventory(&scope, record.ts()).unwrap(), edit);
    assert_eq!(j.delete_inventory(&scope, record.ts()).unwrap(), terminal);
}

mod corruption;
