use super::*;
use crate::slack_lists::comments::Target;
use serde_json::{Value, json};

const CHANNEL: &str = "C01234567";
const ROOT: &str = "1700000000.000001";
const REPLY: &str = "1700000001.000001";
const SIBLING: &str = "1700000002.000001";

fn page(root_position: Option<usize>) -> Page {
    let (document, mut record, root) = crate::slack_lists::comments::tests::seed();
    record.raw["thread_ts"] = json!(ROOT);
    let target = Target::capture(&document, &record).unwrap();
    let mut rows = vec![
        json!({"type":"message","ts":REPLY,"thread_ts":ROOT,"channel":CHANNEL,
            "user":"U1","text":"Reply to remove","opaque":{"keep":[1,2]}}),
        json!({"type":"message","ts":SIBLING,"thread_ts":ROOT,"user":"U2",
            "text":"Sibling retained","edited":{"ts":"1700000003.000001","user":"U2"},
            "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section",
                "elements":[{"type":"text","text":"Sibling retained","style":{"bold":true}}]}]}],
            "reactions":[{"name":"wave","count":1,"users":["U1"],"opaque":"retained"}],
            "opaque":{"order":[2,1]}}),
    ];
    if let Some(index) = root_position {
        rows.insert(index, root);
    }
    Page::parse(
        &target,
        json!({"ok":true,"channel":CHANNEL,"messages":rows,
        "has_more":true,"response_metadata":{"next_cursor":"opaque+/=cursor"},
        "receipt_opaque":{"keep":true}}),
    )
    .unwrap()
}

fn typed(page: &Page) -> Value {
    serde_json::to_value(page.messages()).unwrap()
}

#[test]
fn deletion_preserves_receipt_target_paging_and_exact_surviving_content() {
    let mut page = page(Some(0));
    // A prior socket delta must also survive deletion of a different reply.
    assert!(page.apply_reaction(CHANNEL, SIBLING, "U3", "wave", true));
    let raw = page.raw().clone();
    let target = page.target().clone();
    let before = typed(&page);
    assert!(page.remove_reply(CHANNEL, REPLY));
    assert_eq!(typed(&page), json!([before[0], before[2]]));
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.target(), &target);
    assert_eq!(page.cursor(), Some("opaque+/=cursor"));
    assert!(page.has_more());
    assert_eq!(
        serde_json::to_value(page.root().unwrap()).unwrap(),
        before[0]
    );
    let after = typed(&page);
    assert!(!page.remove_reply(CHANNEL, REPLY));
    assert_eq!(typed(&page), after);
    assert_eq!(page.raw(), &raw);
    // Source receipt is deliberately historical; reader-owned tombstones fence
    // a later response instead of rewriting this HTTP evidence in place.
    assert!(
        Page::parse(&target, raw)
            .unwrap()
            .messages()
            .iter()
            .any(|message| message.ts.as_deref() == Some(REPLY))
    );
}

#[test]
fn root_lookup_survives_removal_before_or_after_root_and_without_root_row() {
    for position in [Some(0), Some(1), Some(2), None] {
        let mut page = page(position);
        let root = page
            .root()
            .map(|message| serde_json::to_value(message).unwrap());
        let raw = page.raw().clone();
        assert!(page.remove_reply(CHANNEL, REPLY));
        assert_eq!(
            page.root()
                .map(|message| serde_json::to_value(message).unwrap()),
            root
        );
        assert!(page.remove_reply(CHANNEL, SIBLING));
        assert_eq!(
            page.root()
                .map(|message| serde_json::to_value(message).unwrap()),
            root
        );
        assert_eq!(page.messages().len(), usize::from(position.is_some()));
        assert_eq!(page.target().root_ts(), Some(ROOT));
        assert_eq!(page.raw(), &raw);
        assert_eq!(page.cursor(), Some("opaque+/=cursor"));
        assert!(page.has_more());
    }
}

#[test]
fn root_foreign_empty_and_unknown_targets_do_not_mutate() {
    let mut page = page(Some(0));
    let before = typed(&page);
    let raw = page.raw().clone();
    for (channel, ts) in [
        (CHANNEL, ROOT),
        ("COTHER", REPLY),
        ("", REPLY),
        (CHANNEL, ""),
        (CHANNEL, "1700000999.000001"),
    ] {
        assert!(!page.remove_reply(channel, ts));
        assert_eq!(typed(&page), before);
        assert_eq!(page.raw(), &raw);
    }
}

#[test]
fn original_reply_membership_is_required_even_if_typed_identity_diverged() {
    for (field, value) in [("channel", "COTHER"), ("thread_ts", "1700000999.000001")] {
        let mut page = page(Some(0));
        // Page::parse already refuses these wire shapes. Exercise the method's
        // own defensive guard without weakening the public parser.
        let reply = page
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(REPLY))
            .unwrap();
        if field == "channel" {
            reply.channel = Some(value.into());
        } else {
            reply.thread_ts = Some(value.into());
        }
        let before = typed(&page);
        assert!(!page.remove_reply(CHANNEL, REPLY));
        assert_eq!(typed(&page), before);
    }
    let mut page = page(Some(0));
    page.messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(REPLY))
        .unwrap()
        .channel = None;
    assert!(page.remove_reply(CHANNEL, REPLY));
}
