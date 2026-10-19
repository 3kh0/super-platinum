use super::*;
use crate::slack_lists::comments::{Target, projection::Projection};
use serde_json::json;

const CHANNEL: &str = "C01234567";
const ROOT: &str = "1700000000.000001";
const REPLY: &str = "1700000001.000001";

fn page(patch: Value) -> Page {
    let (document, record, root) = crate::slack_lists::comments::tests::seed();
    let target = Target::capture(&document, &record).unwrap();
    let mut reply = json!({"type":"message","ts":REPLY,"thread_ts":ROOT,
        "user":"U_ORIGINAL","text":"Original content","opaque":{"keep":[1,2]},
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"text","text":"Original content","style":{"bold":true}}]}]}],
        "reactions":[{"name":"wave","count":1,"users":["U_FIRST"],"opaque":"keep"}]});
    reply
        .as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    Page::parse(
        &target,
        json!({"ok":true,"channel":CHANNEL,"messages":[root,reply,
        {"ts":"1700000002.000001","thread_ts":ROOT,"text":"Sibling"}],
        "has_more":true,"response_metadata":{"next_cursor":"opaque+/=cursor"},
        "opaque_receipt":{"keep":true}}),
    )
    .unwrap()
}
fn typed(page: &Page) -> Value {
    serde_json::to_value(page.messages()).unwrap()
}

#[test]
fn add_duplicate_and_remove_reuse_shared_reaction_reducer() {
    let mut page = page(json!({}));
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
    let reaction = &page.messages()[1].reactions[0];
    assert_eq!(reaction.count, 2);
    assert_eq!(reaction.users, ["U_FIRST", "U_SECOND"]);
    assert_eq!(reaction.extra["opaque"], "keep");
    let before = typed(&page);
    assert!(!page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
    assert_eq!(typed(&page), before);
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", false));
    assert_eq!(page.messages()[1].reactions[0].count, 1);
    assert_eq!(page.messages()[1].reactions[0].users, ["U_FIRST"]);
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_FIRST", "wave", false));
    assert!(page.messages()[1].reactions.is_empty());
    assert!(!page.apply_reaction(CHANNEL, REPLY, "U_FIRST", "wave", false));
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_FIRST", "custom_party", true));
    assert_eq!(page.messages()[1].reactions[0].name, "custom_party");
}

#[test]
fn delta_preserves_raw_target_pagination_and_all_nonreaction_content() {
    let mut page = page(json!({"channel":CHANNEL}));
    let raw = page.raw().clone();
    let target = page.target().clone();
    let before = typed(&page);
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
    let mut after = typed(&page);
    after[1]["reactions"] = before[1]["reactions"].clone();
    assert_eq!(after, before);
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.target(), &target);
    assert_eq!(page.cursor(), Some("opaque+/=cursor"));
    assert!(page.has_more());
    assert_eq!(
        serde_json::to_value(page.root().unwrap()).unwrap(),
        before[0]
    );
    // Re-parsing the unchanged source receipt intentionally has the older pill.
    assert_eq!(
        Page::parse(&target, raw).unwrap().messages()[1].reactions[0].count,
        1
    );
}

#[test]
fn foreign_unknown_root_and_empty_actor_or_name_never_mutate() {
    let mut page = page(json!({}));
    let before = typed(&page);
    for (channel, ts, user, name) in [
        ("C_OTHER", REPLY, "U_FIRST", "wave"),
        (CHANNEL, "1700000999.000001", "U_FIRST", "wave"),
        (CHANNEL, ROOT, "U_FIRST", "wave"),
        (CHANNEL, REPLY, "", "wave"),
        (CHANNEL, REPLY, "U_FIRST", ""),
    ] {
        for added in [false, true] {
            assert!(!page.apply_reaction(channel, ts, user, name, added));
            assert_eq!(typed(&page), before);
        }
    }
}

#[test]
fn hidden_and_deleted_rows_are_not_reaction_targets() {
    let mut patches = vec![
        json!({"subtype":"tombstone"}),
        json!({"subtype":"message_deleted"}),
    ];
    for key in [
        "isNonExistent",
        "no_display",
        "_hidden_stub",
        "is_deleted",
        "is_tombstoned",
    ] {
        for value in [json!(true), json!("yes"), json!(1), json!([]), json!({})] {
            patches.push(json!({key:value}));
        }
    }
    for patch in patches {
        let mut page = page(patch);
        let before = typed(&page);
        for added in [true, false] {
            assert!(!page.apply_reaction(CHANNEL, REPLY, "U_FIRST", "wave", added));
            assert_eq!(typed(&page), before);
        }
    }
}

#[test]
fn explicit_deleted_timestamp_blocks_row_even_if_retained_in_response() {
    let original = page(json!({}));
    let mut raw = original.raw().clone();
    raw["deleted"] = json!([REPLY]);
    let mut deleted = Page::parse(original.target(), raw.clone()).unwrap();
    let before = typed(&deleted);
    assert!(!deleted.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
    assert!(!deleted.apply_reaction(CHANNEL, REPLY, "U_FIRST", "wave", false));
    assert_eq!(typed(&deleted), before);
    assert_eq!(deleted.raw(), &raw);
}

#[test]
fn falsey_flags_and_hidden_reply_follow_projection_instead_of_guessed_hiding() {
    for value in [Value::Null, json!(false), json!(0), json!("")] {
        let mut page = page(json!({"no_display":value,"_hidden_reply":true}));
        assert!(page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
    }
    let mut page = page(json!({"subtype":"list_edit","list_edit":{}}));
    let on = Projection::observed(json!({"ok":true,"assignments":{
        "lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}}))
    .unwrap();
    assert!(
        !on.visible(page.messages(), ROOT)
            .iter()
            .any(|m| m.ts.as_deref() == Some(REPLY))
    );
    // Page owns unconditional guards only. The actual projected caller refuses
    // this target under `on`; a Page never fabricates an assignment observation.
    assert!(page.apply_reaction(CHANNEL, REPLY, "U_SECOND", "wave", true));
}
