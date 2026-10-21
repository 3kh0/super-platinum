use super::*;
use crate::slack_lists::{Document, Record};
use serde_json::json;
const TS: &str = "1700000001.000001";
fn page() -> Page {
    let doc = Document::parse(
        json!({"id":"F01234567","mode":"list","list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let record =
        Record::parse(json!({"id":"Rec1","fields":[],"created_by":"UOWNER","date_created":1}))
            .unwrap();
    Page::parse(&Target::capture(&doc,&record).unwrap(),json!({"ok":true,"has_more":true,
        "response_metadata":{"next_cursor":"original-cursor"},"opaque":{"retained":true},"messages":[
        {"ts":"1700000000.000001","type":"message","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"}},
        {"ts":TS,"thread_ts":"1700000000.000001","type":"message","user":"UOWNER","text":"Original plain comment",
        "reactions":[{"name":"wave","count":1,"users":["UOTHER"]}],"opaque":{"body":1}}]})).unwrap()
}
fn permission(allowed: bool) -> OwnDeletePermission {
    OwnDeletePermissionQuery::capture("T1","UOWNER","C01234567",false).unwrap()
        .accept(json!({"ok":true,"allowed":if allowed {vec!["DELETE_OWN_MESSAGE"]}else{vec![]},"opaque":"original permission"})).unwrap()
}
fn policy() -> OwnDeletePolicy {
    OwnDeletePolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567"}),
        true,
        false,
        false,
        &permission(true),
    )
    .unwrap()
}
fn capture(p: &Page) -> Result<OwnDeleteIntent, ListError> {
    OwnDeleteIntent::capture("T1", "UOWNER", p, &p.messages()[1], &policy())
}
fn intent() -> OwnDeleteIntent {
    capture(&page()).unwrap()
}
fn ack() -> Value {
    json!({"ok":true,"channel":"C01234567","ts":TS,"opaque":{"original":true}})
}

#[test]
fn permission_query_preserves_exact_scoped_array_and_raw_grant() {
    for admin in [false, true] {
        let q = OwnDeletePermissionQuery::capture("T1", "UOWNER", "C01234567", admin).unwrap();
        let mut expected = json!({"user_id":"UOWNER","channel_id":"C01234567","permissions":["DELETE_OWN_MESSAGE"]});
        if admin {
            expected["as_admin"] = json!(true);
        }
        assert_eq!(q.body(), expected);
        q.validate().unwrap();
        assert_eq!(q.workspace(), "T1");
        let raw = json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"],"opaque":[1,2]});
        let p = q.clone().accept(raw.clone()).unwrap();
        assert_eq!(p.query(), &q);
        assert_eq!(p.raw(), &raw);
        assert!(p.allowed());
    }
    assert!(!permission(false).allowed());
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"allowed":null}),
        json!({"ok":true,"allowed":[true]}),
        json!({"ok":"true","allowed":["DELETE_OWN_MESSAGE"]}),
        json!({"ok":false,"allowed":["DELETE_OWN_MESSAGE"]}),
        json!({"ok":true,"error":"failed","allowed":["DELETE_OWN_MESSAGE"]}),
    ] {
        assert!(
            OwnDeletePermissionQuery::capture("T1", "UOWNER", "C01234567", false)
                .unwrap()
                .accept(raw)
                .is_err()
        );
    }
    assert!(
        OwnDeletePermissionQuery::capture("T1", "UOWNER", "C01234567", false)
            .unwrap()
            .accept(json!({"ok":true,"allowed":["delete_own_message","DELETE_ANY_MESSAGE"]}))
            .is_ok_and(|p| !p.allowed())
    );
}

#[test]
fn ordinary_plain_rich_file_and_attachment_bodies_do_not_need_edit_mode_or_window() {
    for extra in [
        json!({}),
        json!({"subtype":"me_message"}),
        json!({"subtype":"bot_message"}),
        json!({"blocks":[{"type":"rich_text","elements":[]}],"markdown_text":"portable"}),
        json!({"subtype":"file_share","files":[{"id":"FOTHER","title":"Original file"}],"attachments":[{"text":"Original attachment"}]}),
    ] {
        let p = page();
        let mut raw = p.raw().clone();
        raw["messages"][1]
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let p = Page::parse(p.target(), raw).unwrap();
        let i = capture(&p).unwrap();
        i.record().validate().unwrap();
        assert_eq!(i.record().http_raw(), p.raw());
        assert_eq!(i.record().original_raw(), &p.raw()["messages"][1]);
    }
    let p = page();
    let channel = json!({"id":"C01234567","is_archived":true,"is_frozen":true});
    let policy = OwnDeletePolicy::observed(
        "T1",
        "UOWNER",
        channel,
        true,
        false,
        false,
        &permission(true),
    )
    .unwrap();
    assert!(OwnDeleteIntent::capture("T1", "UOWNER", &p, &p.messages()[1], &policy).is_ok());
}

#[test]
fn original_http_and_newer_typed_body_have_independent_custody() {
    let mut p = page();
    let original = p.raw().clone();
    p.messages[1].text = Some("Current accepted or socket body".into());
    let i = capture(&p).unwrap();
    assert_eq!(i.record().http_raw(), &original);
    assert_eq!(
        i.record().message_raw()["text"],
        "Current accepted or socket body"
    );
    assert!(i.revalidate(&p, &p.messages()[1], &policy()).is_err());
    let mut fresh = original.clone();
    fresh["messages"][1]["text"] = json!("Current accepted or socket body");
    let fresh = Page::parse(p.target(), fresh).unwrap();
    i.revalidate(&fresh, &p.messages()[1], &policy()).unwrap();
    assert_eq!(i.record().http_raw(), &original);
    let mut reacted = p.messages()[1].clone();
    reacted.reactions.clear();
    reacted.reply_count = Some(88);
    reacted.channel = Some("C01234567".into());
    assert!(i.record().current(&reacted));
    reacted.text = Some("Successor body".into());
    assert!(!i.record().current(&reacted));
    assert!(i.revalidate(&fresh, &reacted, &policy()).is_err());
}

#[test]
fn roots_foreign_retired_special_and_removed_rows_refuse() {
    let p = page();
    assert!(OwnDeleteIntent::capture("T1", "UOWNER", &p, p.root().unwrap(), &policy()).is_err());
    for (key, value) in [
        ("user", json!("UOTHER")),
        ("channel", json!("COTHER")),
        ("thread_ts", json!("1700000099.000001")),
        ("is_deleted", json!(true)),
        ("isNonExistent", json!(true)),
        ("_hidden_stub", json!(true)),
        ("subtype", json!("moderated")),
        ("subtype", json!("thread_hidden")),
        ("subtype", json!("retention")),
        ("subtype", json!("thread_broadcast")),
        ("subtype", json!("reply_broadcast")),
        ("subtype", json!("channel_topic")),
    ] {
        let mut raw = p.raw().clone();
        raw["messages"][1][key] = value;
        if let Ok(p) = Page::parse(p.target(), raw) {
            assert!(capture(&p).is_err(), "{key}");
        }
    }
    let mut retired = p.clone();
    retired.messages.remove(1);
    assert!(
        OwnDeleteIntent::capture("T1", "UOWNER", &retired, &p.messages()[1], &policy()).is_err()
    );
    let mut locked = p.clone();
    locked.messages[0]
        .extra
        .insert("is_locked".into(), json!(true));
    assert!(capture(&locked).is_err());
    let mut raw = p.raw().clone();
    raw["deleted"] = json!([TS]);
    assert!(capture(&Page::parse(p.target(), raw).unwrap()).is_err());
}

#[test]
fn scoped_permission_and_context_denials_are_not_default_grants() {
    let p = page();
    for (can_thread, readonly, external, channel, permission) in [
        (
            false,
            false,
            false,
            json!({"id":"C01234567"}),
            permission(true),
        ),
        (
            true,
            true,
            false,
            json!({"id":"C01234567"}),
            permission(true),
        ),
        (
            true,
            false,
            false,
            json!({"id":"C01234567"}),
            permission(false),
        ),
        (
            true,
            false,
            true,
            json!({"id":"C01234567","is_ext_shared":true}),
            permission(true),
        ),
        (
            true,
            false,
            false,
            json!({"id":"C01234567","is_non_threadable":true}),
            permission(true),
        ),
    ] {
        let q = OwnDeletePolicy::observed(
            "T1",
            "UOWNER",
            channel,
            can_thread,
            readonly,
            external,
            &permission,
        )
        .unwrap();
        assert!(OwnDeleteIntent::capture("T1", "UOWNER", &p, &p.messages()[1], &q).is_err());
    }
    for (team, user, channel) in [
        ("TOTHER", "UOWNER", "C01234567"),
        ("T1", "UOTHER", "C01234567"),
        ("T1", "UOWNER", "COTHER"),
    ] {
        assert!(
            OwnDeletePolicy::observed(
                team,
                user,
                json!({"id":channel}),
                true,
                false,
                false,
                &permission(true)
            )
            .is_err()
        );
    }
    let local = OwnDeletePolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567","is_shared":true}),
        true,
        false,
        true,
        &permission(true),
    )
    .unwrap();
    assert!(OwnDeleteIntent::capture("T1", "UOWNER", &p, &p.messages()[1], &local).is_ok());
    assert!(OwnDeleteIntent::capture("TOTHER", "UOWNER", &p, &p.messages()[1], &policy()).is_err());
}

#[test]
fn exact_target_and_fresh_current_body_are_revalidated() {
    let i = intent();
    let p = page();
    i.revalidate(&p, &p.messages()[1], &policy()).unwrap();
    let mut changed = p.clone();
    changed.target.record["date_created"] = json!(2);
    assert!(
        i.revalidate(&changed, &changed.messages()[1], &policy())
            .is_err()
    );
    let mut changed = p.clone();
    changed.messages[1].text = Some("Changed".into());
    assert!(
        i.revalidate(&changed, &changed.messages()[1], &policy())
            .is_err()
    );
    let mut reaction = p.clone();
    reaction.messages[1].reactions.clear();
    i.revalidate(&reaction, &reaction.messages()[1], &policy())
        .unwrap();
}

#[test]
fn consumed_ack_requires_exact_identity_and_retains_original_raw() {
    for (key, value) in [
        ("ok", json!(false)),
        ("ok", json!(1)),
        ("channel", json!("COTHER")),
        ("ts", json!("1700000099.000001")),
        ("ts", Value::Null),
        ("error", json!("error")),
    ] {
        let mut raw = ack();
        raw[key] = value;
        assert!(intent().accept(raw).is_err());
    }
    for key in ["channel", "ts", "ok"] {
        let mut raw = ack();
        raw.as_object_mut().unwrap().remove(key);
        assert!(intent().accept(raw).is_err());
    }
    let raw = ack();
    let accepted = intent().accept(raw.clone()).unwrap();
    assert_eq!(accepted.raw(), &raw);
    assert_eq!(
        accepted.record().original_raw()["text"],
        "Original plain comment"
    );
}

#[test]
fn readback_requires_positive_scoped_deletion_not_absence_or_typed_removal() {
    let r = intent().record().clone();
    let p = page();
    assert!(!r.observed(&p).unwrap());
    let mut removed = p.clone();
    assert!(removed.remove_reply("C01234567", TS));
    assert!(!r.observed(&removed).unwrap());
    for more in [false, true] {
        let mut raw = p.raw().clone();
        raw["messages"].as_array_mut().unwrap().remove(1);
        raw["has_more"] = json!(more);
        assert!(
            !r.observed(&Page::parse(p.target(), raw.clone()).unwrap())
                .unwrap()
        );
        raw["deleted"] = json!([TS]);
        assert!(r.observed(&Page::parse(p.target(), raw).unwrap()).unwrap());
    }
    let mut raw = p.raw().clone();
    raw["messages"][1] = json!({"ts":TS,"thread_ts":p.target().root_ts(),"subtype":"tombstone"});
    assert!(
        r.observed(&Page::parse(p.target(), raw.clone()).unwrap())
            .unwrap()
    );
    raw["messages"][1]["user"] = json!("UOTHER");
    assert!(!r.observed(&Page::parse(p.target(), raw).unwrap()).unwrap());
    let mut foreign = p.clone();
    foreign.target.record["date_created"] = json!(9);
    assert!(r.observed(&foreign).is_err());
    assert_eq!(r.http_raw(), p.raw());
}

#[test]
fn deleted_list_cannot_override_live_or_foreign_message_evidence() {
    let record = intent().record().clone();
    let p = page();
    let mut raw = p.raw().clone();
    raw["deleted"] = json!([TS]);
    assert!(
        !record
            .observed(&Page::parse(p.target(), raw.clone()).unwrap())
            .unwrap()
    );
    raw["messages"][1] =
        json!({"ts":TS,"thread_ts":p.target().root_ts(),"subtype":"tombstone","user":"UOTHER"});
    assert!(
        !record
            .observed(&Page::parse(p.target(), raw.clone()).unwrap())
            .unwrap()
    );
    raw["messages"][1]["user"] = json!("UOWNER");
    raw["messages"][1]["type"] = json!("message_changed");
    assert!(
        !record
            .observed(&Page::parse(p.target(), raw.clone()).unwrap())
            .unwrap()
    );
    raw["messages"][1]["type"] = json!("message");
    assert!(
        record
            .observed(&Page::parse(p.target(), raw).unwrap())
            .unwrap()
    );
    assert_eq!(record.http_raw(), p.raw());
}

#[test]
fn inert_roundtrip_validates_both_sources_without_recreating_intent() {
    let r = intent().record().clone();
    let raw = serde_json::to_value(&r).unwrap();
    let restored: OwnDeleteRecord = serde_json::from_value(raw.clone()).unwrap();
    restored.validate().unwrap();
    assert_eq!(r, restored);
    for path in ["message", "http"] {
        let mut broken = raw.clone();
        if path == "message" {
            broken[path]["user"] = json!("UOTHER");
        } else {
            broken[path]["messages"][1]["user"] = json!("UOTHER");
        }
        assert!(
            serde_json::from_value::<OwnDeleteRecord>(broken)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut broken = raw.clone();
    broken["version"] = json!(2);
    assert!(
        serde_json::from_value::<OwnDeleteRecord>(broken)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut broken = raw;
    broken["sendable"] = json!(true);
    assert!(serde_json::from_value::<OwnDeleteRecord>(broken).is_err());
}

#[test]
fn changed_file_body_policy_revocation_and_false_tombstones_refuse() {
    let p = page();
    let mut raw = p.raw().clone();
    raw["messages"][1]["files"] = json!([{"id":"F_FILE1","title":"Original"}]);
    let p = Page::parse(p.target(), raw).unwrap();
    let i = capture(&p).unwrap();
    let mut known = serde_json::to_value(&p.messages()[1]).unwrap();
    known["files"][0]["title"] = json!("Changed file");
    let known: Message = serde_json::from_value(known).unwrap();
    assert!(!i.record().current(&known));
    let denied = OwnDeletePolicy::observed(
        "T1",
        "UOWNER",
        json!({"id":"C01234567"}),
        true,
        false,
        false,
        &permission(false),
    )
    .unwrap();
    assert!(i.revalidate(&p, &p.messages()[1], &denied).is_err());
    for change in [
        json!({"is_deleted":false}),
        json!({"is_deleted":"true"}),
        json!({"_hidden_stub":true}),
    ] {
        let mut raw = p.raw().clone();
        raw["messages"][1]
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        assert!(
            !i.record()
                .observed(&Page::parse(p.target(), raw).unwrap())
                .unwrap()
        );
    }
    let mut raw = p.raw().clone();
    raw["deleted"] = json!(["1700000099.000001"]);
    assert!(
        !i.record()
            .observed(&Page::parse(p.target(), raw).unwrap())
            .unwrap()
    );
}

#[test]
fn permission_query_enterprise_route_is_separate_from_body_and_retained() {
    let ordinary = OwnDeletePermissionQuery::capture("T1", "UOWNER", "C01234567", false).unwrap();
    assert_eq!(ordinary.enterprise_scope(), None);
    assert_eq!(ordinary.edge_scope(), "T1");
    let enterprise = ordinary
        .clone()
        .with_enterprise_scope(Some("EENTERPRISE1"))
        .unwrap();
    assert_eq!(enterprise.enterprise_scope(), Some("EENTERPRISE1"));
    assert_eq!(enterprise.edge_scope(), "EENTERPRISE1");
    assert_eq!(enterprise.workspace(), "T1");
    assert_eq!(enterprise.body(), ordinary.body());
    let permission = enterprise
        .clone()
        .accept(json!({"ok":true,"allowed":["DELETE_OWN_MESSAGE"]}))
        .unwrap();
    assert_eq!(permission.query(), &enterprise);
    assert_eq!(enterprise.with_enterprise_scope(None).unwrap(), ordinary);
    for malformed in ["", "E BAD", "E\nBAD"] {
        assert!(
            ordinary
                .clone()
                .with_enterprise_scope(Some(malformed))
                .is_err()
        );
    }
}
