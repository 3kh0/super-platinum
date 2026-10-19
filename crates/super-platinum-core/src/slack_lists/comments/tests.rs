use super::*;
use serde_json::json;

pub(super) fn seed() -> (Document, Record, Value) {
    let document = Document::parse(json!({"id":"F01234567","mode":"list","access":"read",
        "opaque":{"keep":[1,"二"]},"list_metadata":{"schema":[],"views":[]}}))
    .unwrap();
    let row =
        Record::parse(json!({"id":"Rec1","list_id":"F01234567","fields":[],"opaque":"preserve"}))
            .unwrap();
    let root = json!({"type":"message","subtype":"list_record_comment","ts":"1700000000.000001",
        "slack_list":{"list_id":"F01234567","list_record_id":"Rec1","opaque":9},"unknown":[1,2]});
    (document, row, root)
}
#[test]
fn alias_acquires_real_root_without_rewriting_list_or_record() {
    let (doc, row, root) = seed();
    let target = Target::capture(&doc, &row).unwrap();
    assert_eq!(target.channel_id(), "C01234567");
    assert_eq!(target.request_ts(), "F01234567-Rec1");
    let raw = json!({"ok":true,"messages":[root],"has_more":true,"response_metadata":{"next_cursor":"opaque+cursor"},"unknown":{"keep":true}});
    let page = Page::parse(&target, raw.clone()).unwrap();
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.target().root_ts(), Some("1700000000.000001"));
    assert_eq!(page.target().document_raw(), &doc.raw);
    assert_eq!(page.target().record_raw(), &row.raw);
    assert_eq!(page.root().unwrap().extra["unknown"], json!([1, 2]));
    assert_eq!(page.cursor(), Some("opaque+cursor"));
    assert!(page.has_more());
    assert_eq!(target.root_ts(), None);
}
#[test]
fn existing_root_and_continuation_are_distinct_from_alias_resolution() {
    let (doc, mut row, root) = seed();
    row.raw["thread_ts"] = root["ts"].clone();
    let target = Target::capture(&doc, &row).unwrap();
    assert_eq!(target.request_ts(), "1700000000.000001");
    let page = Page::parse(&target,json!({"ok":true,"messages":[{"ts":"1700000000.000002","thread_ts":"1700000000.000001","text":"reply","unknown":true}]})).unwrap();
    assert!(page.root().is_none());
    assert_eq!(page.messages().len(), 1);
    let (doc, row, _) = seed();
    let alias = Target::capture(&doc, &row).unwrap();
    assert!(Page::parse(&alias, page.raw().clone()).is_err());
    assert!(
        ReplyQuery {
            cursor: Some("next".into()),
            ..Default::default()
        }
        .validate(&alias)
        .is_err()
    );
}
#[test]
fn foreign_missing_duplicate_or_wrong_item_roots_are_refused() {
    let (doc, row, root) = seed();
    let target = Target::capture(&doc, &row).unwrap();
    let mut foreign = root.clone();
    foreign["channel"] = json!("C76543210");
    let mut wrong = root.clone();
    wrong["slack_list"]["list_record_id"] = json!("Rec2");
    let mut missing = root.clone();
    missing.as_object_mut().unwrap().remove("slack_list");
    let mut sibling = root.clone();
    sibling["ts"] = json!("1700000000.000002");
    for messages in [
        json!([]),
        json!([foreign]),
        json!([wrong]),
        json!([missing]),
        json!([root.clone(), root.clone()]),
        json!([root, sibling]),
    ] {
        assert!(Page::parse(&target, json!({"ok":true,"messages":messages})).is_err());
    }
}
#[test]
fn page_refuses_foreign_reply_and_malformed_pagination_without_losing_unknowns() {
    let (doc, row, root) = seed();
    let target = Target::capture(&doc, &row).unwrap();
    let base = json!({"ok":true,"messages":[root]});
    for (key, value) in [
        ("ok", json!(false)),
        ("error", json!("denied")),
        ("has_more", json!("false")),
        ("response_metadata", json!({"next_cursor":2})),
        ("unchanged_messages", json!(["1700000000.000002"])),
        ("deleted", json!([2])),
    ] {
        let mut raw = base.clone();
        raw[key] = value;
        assert!(Page::parse(&target, raw).is_err());
    }
    let mut raw = base;
    raw["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"ts":"1700000000.000002","thread_ts":"1700000000.999999"}));
    assert!(Page::parse(&target, raw).is_err());
}
#[test]
fn capture_and_revalidation_bind_exact_raw_owners_without_editor_acl() {
    let (doc, row, _) = seed();
    let target = Target::capture(&doc, &row).unwrap();
    assert!(!doc.editable(Some("full")));
    let mut changed = row.clone();
    changed.raw["opaque"] = json!("changed");
    assert!(target.revalidate(&doc, &changed).is_err());
    let mut changed = row.clone();
    changed.raw["list_id"] = json!("F76543210");
    assert!(Target::capture(&doc, &changed).is_err());
    let mut changed = row.clone();
    changed.raw["archived"] = json!(true);
    assert!(Target::capture(&doc, &changed).is_ok());
    for id in [
        "F1",
        "C01234567",
        "F0123456é",
        "F0123456a",
        "F012345678901234",
    ] {
        assert!(channel(id).is_err());
    }
}
#[test]
fn tombstone_policy_is_exact_connected_and_topic_desync_truth_table() {
    for connected in [false, true] {
        for channel in [false, true] {
            for user in [false, true] {
                assert_eq!(
                    Tombstones::from_realtime(connected, channel, user),
                    if !connected || channel || user {
                        Tombstones::Include
                    } else {
                        Tombstones::Exclude
                    }
                );
            }
        }
    }
}

#[test]
fn refresh_retains_only_same_item_and_continuation_root() {
    let (doc, row, root) = seed();
    let alias = Target::capture(&doc, &row).unwrap();
    let page = Page::parse(&alias, json!({"ok":true,"messages":[root]})).unwrap();
    let resolved = page.target();
    let file = json!({"ok":true,"file":doc.raw});
    let record = json!({"ok":true,"record":row.raw});
    assert_eq!(
        resolved.refresh(&file, &record, true).unwrap().root_ts(),
        resolved.root_ts()
    );
    assert!(alias.refresh(&file, &record, true).is_err());
    let mut changed = record.clone();
    changed["record"]["opaque"] = json!("new");
    assert!(resolved.refresh(&file, &changed, true).is_err());
    assert!(resolved.refresh(&file, &changed, false).is_ok());
    changed["record"]["thread_ts"] = json!("1700000000.999999");
    assert!(resolved.refresh(&file, &changed, true).is_err());
    assert_eq!(
        resolved.refresh(&file, &changed, false).unwrap().root_ts(),
        Some("1700000000.999999")
    );
    for (key, value) in [
        ("id", json!("other")),
        ("created_by", json!("other")),
        ("date_created", json!(3)),
        ("is_deleted", json!(true)),
    ] {
        let mut changed = record.clone();
        changed["record"][key] = value;
        assert!(resolved.refresh(&file, &changed, false).is_err());
    }
    let mut failed = file.clone();
    failed["error"] = json!("failed");
    assert!(resolved.refresh(&failed, &record, false).is_err());
}
#[test]
fn known_empty_is_valid_but_unknown_empty_cannot_invent_root() {
    let (doc, mut row, _) = seed();
    let empty = json!({"ok":true,"messages":[],"has_more":false});
    assert!(Page::parse(&Target::capture(&doc, &row).unwrap(), empty.clone()).is_err());
    row.raw["thread_ts"] = json!("1700000000.000001");
    let page = Page::parse(&Target::capture(&doc, &row).unwrap(), empty).unwrap();
    assert!(page.messages().is_empty());
    assert!(!page.has_more());
}
#[test]
fn synthetic_page_projects_using_acquired_root_not_request_alias() {
    let (doc, row, root) = seed();
    let alias = Target::capture(&doc, &row).unwrap();
    let page = Page::parse(
        &alias,
        json!({"ok":true,"messages":[root,
        {"ts":"1700000000.000002","thread_ts":"1700000000.000001","text":"Visible reply"}]}),
    )
    .unwrap();
    let projection = projection::Projection::observed(json!({"ok":true,"assignments":{}})).unwrap();
    let visible = projection.visible(page.messages(), page.target().root_ts().unwrap());
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].ts.as_deref(), Some("1700000000.000002"));
    assert!(alias.root_ts().is_none());
    assert_eq!(page.root().unwrap().ts.as_deref(), page.target().root_ts());
}
#[test]
fn query_does_not_collapse_two_source_reads_into_one() {
    let (doc, mut row, _) = seed();
    row.raw["thread_ts"] = json!("1700000000.000001");
    let target = Target::capture(&doc, &row).unwrap();
    let both = ReplyQuery {
        oldest: Some("1700000000.000000".into()),
        latest: Some("1700000000.000002".into()),
        cursor: None,
    };
    assert!(both.validate(&target).is_err());
    let equal = ReplyQuery {
        oldest: target.root_ts().map(str::to_owned),
        latest: target.root_ts().map(str::to_owned),
        cursor: None,
    };
    assert!(equal.validate(&target).is_ok());
}
