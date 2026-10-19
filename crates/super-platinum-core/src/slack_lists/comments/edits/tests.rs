use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::json;

const CHANNEL: &str = "C01234567";
const ROOT: &str = "1700000000.000001";
const TS: &str = "1700000001.000001";

fn page() -> Page {
    let (document, record, root) = crate::slack_lists::comments::tests::seed();
    let target = Target::capture(&document, &record).unwrap();
    Page::parse(
        &target,
        json!({"ok":true,"messages":[root,{
        "type":"message","ts":TS,"thread_ts":ROOT,"user":"U1","text":"Old",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"text","text":"Old","style":{"bold":true}}]}]}],
        "files":[{"id":"F01234567"}],"attachments":[{"text":"Old attachment"}],
        "reactions":[{"name":"wave","count":1,"users":["U2"]}],
        "edited":{"user":"U1","ts":"1699999999.000001"},
        "metadata":{"old":true,"nested":{"retain":true}},
        "opaque_previous":{"retain":true},"parent_user_id":"U_PARENT"
    }],"has_more":true,"response_metadata":{"next_cursor":"opaque+/cursor"},
    "opaque_receipt":{"retain":true}}),
    )
    .unwrap()
}

fn raw(patch: Value) -> Value {
    let mut raw = json!({"ts":TS,"thread_ts":ROOT,"user":"U1","text":"Updated"});
    raw.as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    raw
}
fn update(raw: Value) -> MessageUpdate {
    update_kind(raw, "message_changed", CHANNEL)
}
fn update_kind(raw: Value, kind: &str, channel: &str) -> MessageUpdate {
    match parse_event(
        &json!({"type":"message","subtype":kind,"channel":channel,"message":raw}).to_string(),
    )
    .unwrap()
    {
        RtEvent::MessageChanged {
            original_update: Some(update),
            ..
        } => update,
        _ => panic!("expected parser-owned original update"),
    }
}
fn projected(page: &Page, update: &MessageUpdate) -> ReplyEdit {
    ReplyEdit::project(page.target(), &page.messages()[1], update).unwrap()
}
fn value(message: &Message) -> Value {
    serde_json::to_value(message).unwrap()
}

#[test]
fn omitted_generated_body_fields_clear_but_picked_scalars_and_reactions_preserve() {
    let page = page();
    let before = value(&page.messages()[1]);
    let incoming = raw(json!({"unrecognized_wire_field":{"do_not_adopt":true}}));
    let witness = update(incoming.clone());
    let edit = projected(&page, &witness);
    let next = edit.apply(&page.messages()[1]).unwrap();
    assert_eq!(next.text.as_deref(), Some("Updated"));
    assert!(next.blocks.is_empty() && next.files.is_empty() && next.attachments.is_empty());
    assert_eq!(next.edited, page.messages()[1].edited);
    assert_eq!(next.extra["metadata"], before["metadata"]);
    assert_eq!(next.extra["opaque_previous"], before["opaque_previous"]);
    assert!(!next.extra.contains_key("unrecognized_wire_field"));
    assert_eq!(value(&next)["reactions"], before["reactions"]);
    assert!(!edit.writes_reactions());
    assert_eq!(next.parent_user_id.as_deref(), Some("U_PARENT"));
    assert_eq!(value(&page.messages()[1]), before);
    assert_eq!(witness.raw(), &incoming);
}

#[test]
fn explicit_empty_reactions_and_blocks_are_writes_and_rich_leaves_remain_exact() {
    let page = page();
    let edit = projected(
        &page,
        &update(raw(
            json!({"blocks":[],"reactions":[],"files":[],"attachments":[]}),
        )),
    );
    let next = edit.apply(&page.messages()[1]).unwrap();
    assert!(edit.writes_reactions());
    assert!(next.reactions.is_empty() && next.blocks.is_empty());
    let blocks = json!([{"type":"rich_text","opaque":{"keep":[]},"elements":[{
        "type":"rich_text_section","elements":[{"type":"text","text":"New rich","style":{"italic":true},"extra":null}]}]}]);
    let rich = projected(&page, &update(raw(json!({"blocks":blocks}))));
    assert_eq!(
        value(&rich.apply(&page.messages()[1]).unwrap())["blocks"],
        blocks
    );
}

#[test]
fn scalar_null_empty_and_nested_objects_follow_field_specific_rules() {
    let page = page();
    let edit = projected(
        &page,
        &update(raw(json!({"text":null,"edited":null,
        "metadata":{"only":"new"},"parent_user_id":null}))),
    );
    let next = edit.apply(&page.messages()[1]).unwrap();
    assert_eq!(next.text, None);
    assert_eq!(next.edited, None);
    assert_eq!(next.extra["metadata"], json!({"only":"new"}));
    assert_eq!(next.parent_user_id.as_deref(), Some("U_PARENT"));
    let empty = projected(
        &page,
        &update(raw(
            json!({"text":"","edited":{"ts":"1700000002.000001"},"metadata":null}),
        )),
    );
    let next = empty.apply(&page.messages()[1]).unwrap();
    assert_eq!(next.text.as_deref(), Some(""));
    assert_eq!(next.edited, Some(json!({"ts":"1700000002.000001"})));
    assert_eq!(next.extra["metadata"], Value::Null);
    let mut absent = raw(json!({}));
    absent.as_object_mut().unwrap().remove("text");
    assert_eq!(
        projected(&page, &update(absent))
            .apply(&page.messages()[1])
            .unwrap()
            .text,
        None
    );
}

#[test]
fn original_identity_kind_and_known_root_guards_refuse_before_projection() {
    let page = page();
    for incoming in [
        raw(json!({"channel":"C_OTHER"})),
        raw(json!({"channel":null})),
        raw(json!({"thread_ts":null})),
        raw(json!({"thread_ts":"1700000009.000001"})),
        raw(json!({"ts":ROOT})),
        raw(json!({"ts":"1700000009.000001"})),
    ] {
        assert_eq!(
            ReplyEdit::project(page.target(), &page.messages()[1], &update(incoming)).unwrap_err(),
            ReplyEditError::Refused
        );
    }
    let mut missing = raw(json!({}));
    missing.as_object_mut().unwrap().remove("thread_ts");
    for witness in [
        update(missing),
        update_kind(raw(json!({})), "message_replied", CHANNEL),
        update_kind(
            raw(json!({"files":[{"id":"F01234567"}]})),
            "message_changed",
            "C_OTHER",
        ),
    ] {
        assert_eq!(
            ReplyEdit::project(page.target(), &page.messages()[1], &witness).unwrap_err(),
            ReplyEditError::Refused
        );
    }
    let witness = update(raw(json!({})));
    assert_eq!(
        ReplyEdit::project(page.target(), &page.messages()[0], &witness).unwrap_err(),
        ReplyEditError::Refused
    );
    let mut hidden = page.messages()[1].clone();
    hidden.extra.insert("no_display".into(), json!(true));
    assert_eq!(
        ReplyEdit::project(page.target(), &hidden, &witness).unwrap_err(),
        ReplyEditError::Refused
    );
}

#[test]
fn generated_thread_fields_and_truthy_auxiliary_fields_have_distinct_omission_rules() {
    let page = page();
    let mut known = page.messages()[1].clone();
    known.reply_count = Some(9);
    known.reply_users = vec!["U_OLD".into()];
    known.extra.insert("language".into(), json!("en"));
    known.extra.insert("_unread_count".into(), json!(5));
    known
        .extra
        .insert("reply_user_icons".into(), json!(["old"]));
    let mut incoming = raw(json!({"unread_count":0,"reply_user_icons":null}));
    incoming.as_object_mut().unwrap().remove("user");
    let edit = ReplyEdit::project(page.target(), &known, &update(incoming)).unwrap();
    let next = edit.apply(&known).unwrap();
    assert_eq!(next.user, None);
    assert_eq!(next.reply_count, Some(0));
    assert!(next.reply_users.is_empty());
    assert!(!next.extra.contains_key("language"));
    assert_eq!(next.extra["_unread_count"], json!(5));
    assert_eq!(next.extra["reply_user_icons"], Value::Null);
    assert_eq!(next.channel.as_deref(), Some(CHANNEL));
    assert_eq!(next.thread_ts.as_deref(), Some(ROOT));
}

#[test]
fn unsupported_known_shapes_are_atomic_and_distinct_from_foreign_refusal() {
    let page = page();
    let before = value(&page.messages()[1]);
    for patch in [
        json!({"files":[{"id":"F01234567"}]}),
        json!({"attachments":[{"text":"new"}]}),
        json!({"subtype":"dlp_tombstone"}),
        json!({"subtype":"bot_message"}),
        json!({"hidden":true}),
        json!({"no_display":"yes"}),
        json!({"room":{"id":"R1"}}),
        json!({"schedule":{"id":"S1"}}),
        json!({"type":"channel_topic"}),
        json!({"type":"channel_purpose"}),
        json!({"type":"channel_join"}),
        json!({"type":"channel_leave"}),
        json!({"list_edit":{}}),
    ] {
        assert_eq!(
            ReplyEdit::project(page.target(), &page.messages()[1], &update(raw(patch)))
                .unwrap_err(),
            ReplyEditError::Unsupported
        );
        assert_eq!(value(&page.messages()[1]), before);
    }
}

#[test]
fn equal_current_projection_still_overrides_different_old_ack_fields() {
    let page = page();
    let edit = projected(
        &page,
        &update(raw(
            json!({"edited":null,"metadata":{"new":true},"reactions":[]}),
        )),
    );
    let current = edit.apply(&page.messages()[1]).unwrap();
    let equal = ReplyEdit::project(
        page.target(),
        &current,
        &update(raw(
            json!({"edited":null,"metadata":{"new":true},"reactions":[]}),
        )),
    )
    .unwrap();
    assert_eq!(value(&equal.apply(&current).unwrap()), value(&current));
    let mut old_ack = page.messages()[1].clone();
    old_ack.client_msg_id = Some("original-ack-uuid".into());
    old_ack
        .extra
        .insert("ack_only".into(), json!({"keep":true}));
    let promoted = equal.apply(&old_ack).unwrap();
    assert_eq!(promoted.text.as_deref(), Some("Updated"));
    assert!(promoted.blocks.is_empty() && promoted.reactions.is_empty());
    assert_eq!(promoted.client_msg_id.as_deref(), Some("original-ack-uuid"));
    assert_eq!(promoted.extra["ack_only"], json!({"keep":true}));
    assert_eq!(promoted.extra["metadata"], json!({"new":true}));
}

#[test]
fn union_retains_earlier_picked_writes_and_only_newer_fields_win() {
    let page = page();
    let first = projected(
        &page,
        &update(raw(
            json!({"edited":null,"metadata":{"first":true},"reactions":[]}),
        )),
    );
    let next = first.apply(&page.messages()[1]).unwrap();
    let newer =
        ReplyEdit::project(page.target(), &next, &update(raw(json!({"text":"Newest"})))).unwrap();
    let union = first.union(&newer).unwrap();
    let after = union.apply(&page.messages()[1]).unwrap();
    assert_eq!(after.text.as_deref(), Some("Newest"));
    assert_eq!(after.edited, None);
    assert_eq!(after.extra["metadata"], json!({"first":true}));
    assert!(after.reactions.is_empty() && union.writes_reactions());
    assert_eq!(value(&after), value(&newer.apply(&next).unwrap()));
    let mut another = next.clone();
    another.ts = Some("1700000002.000001".into());
    let other = ReplyEdit::project(
        page.target(),
        &another,
        &update(raw(json!({"ts":"1700000002.000001"}))),
    )
    .unwrap();
    assert!(union.union(&other).is_none());
    assert!(union.apply(&another).is_none());
}

#[test]
fn page_mutation_preserves_original_receipt_target_root_and_pagination() {
    let mut page = page();
    let raw = page.raw().clone();
    let target = page.target().clone();
    let root = value(page.root().unwrap());
    let edit = projected(&page, &update(self::raw(json!({}))));
    assert!(page.apply_reply_edit(&edit));
    assert!(!page.apply_reply_edit(&edit));
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.target(), &target);
    assert_eq!(value(page.root().unwrap()), root);
    assert_eq!(page.cursor(), Some("opaque+/cursor"));
    assert!(page.has_more());
    let mut deleted_raw = raw;
    deleted_raw["deleted"] = json!([TS]);
    let mut deleted = Page::parse(&target, deleted_raw).unwrap();
    assert!(!deleted.apply_reply_edit(&edit));
}
