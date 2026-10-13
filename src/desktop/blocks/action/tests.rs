use serde_json::{Value, json};
use super_platinum_core::block_actions::{
    BlockConfirmationState, BlockElementMetadata, BlockElementSource, BlockTextFormat,
};

use crate::blocks::{BlockCtx, block_nodes};
use crate::model::RichNode;

fn project(value: Value) -> Vec<RichNode> {
    let core = crate::fixture::fixture_core();
    let media = crate::media::MediaRegistry::default();
    block_nodes(BlockCtx::new(&core.workspaces["T1"], &media), &value)
}

fn metadata(node: &RichNode) -> &BlockElementMetadata {
    let RichNode::Button { metadata, .. } = node else {
        panic!("Expected button metadata: {node:?}");
    };
    metadata
}

fn first_action(nodes: &[RichNode]) -> &RichNode {
    let [RichNode::Actions(elements)] = nodes else {
        panic!("Expected actions: {nodes:?}");
    };
    &elements[0]
}

fn button() -> Value {
    json!({"type":"button","text":{"type":"plain_text","text":"Approve"},
        "action_id":"approve","value":"release-42"})
}

#[test]
fn containing_block_disambiguates_repeated_action_ids() {
    let a = project(json!({"type":"actions","block_id":"release-a","elements":[button()]}));
    let b = project(json!({"type":"actions","block_id":"release-b","elements":[button()]}));
    let a = metadata(first_action(&a));
    let b = metadata(first_action(&b));
    assert_eq!(a.action_id.as_deref(), Some("approve"));
    assert_eq!(a.action_id, b.action_id);
    assert_eq!(a.block_id.as_deref(), Some("release-a"));
    assert_eq!(b.block_id.as_deref(), Some("release-b"));
    assert_ne!(a, b);
    assert_eq!(a.value.as_deref(), Some("release-42"));
    assert_eq!(a.source, BlockElementSource::Button);
}

#[test]
fn section_accessory_keeps_its_own_block_identity() {
    let mut button = button();
    button["block_id"] = json!("spoofed-element-block");
    let nodes = project(json!({"type":"section","block_id":"section-a",
        "text":{"type":"plain_text","text":"Release review"},"accessory":button}));
    let [
        RichNode::Section {
            accessory: Some(accessory),
            ..
        },
    ] = nodes.as_slice()
    else {
        panic!("Expected section: {nodes:?}");
    };
    assert_eq!(metadata(accessory).block_id.as_deref(), Some("section-a"));
}

#[test]
fn missing_and_duplicate_identities_are_not_repaired_with_synthetic_ids() {
    let mut standalone = button();
    standalone["block_id"] = json!("not-a-parent");
    standalone["action_id"] = json!("");
    standalone["value"] = json!("");
    let nodes = project(standalone);
    let meta = metadata(&nodes[0]);
    assert_eq!(meta.block_id, None);
    assert_eq!(meta.action_id.as_deref(), Some(""));
    assert_eq!(meta.value.as_deref(), Some(""));

    let missing = project(json!({"type":"actions","block_id":123,"elements":[
        {"type":"button","text":{"type":"plain_text","text":"Unknown"},"action_id":false}
    ]}));
    let meta = metadata(first_action(&missing));
    assert_eq!(meta.block_id, None);
    assert_eq!(meta.action_id, None);

    let duplicate =
        project(json!({"type":"actions","block_id":"same","elements":[button(),button()]}));
    let [RichNode::Actions(elements)] = duplicate.as_slice() else {
        unreachable!()
    };
    assert_eq!(metadata(&elements[0]), metadata(&elements[1]));
}

#[test]
fn nested_layout_blocks_reset_parent_identity() {
    let nodes = project(json!({"type":"actions","block_id":"outer","elements":[
        {"type":"actions","elements":[button()]}
    ]}));
    let nested = first_action(&nodes);
    let RichNode::Actions(elements) = nested else {
        panic!("Expected nested actions")
    };
    assert_eq!(metadata(&elements[0]).block_id, None);
}

#[test]
fn confirmation_keeps_raw_text_format_style_and_accessibility_label() {
    let mut button = button();
    button["accessibility_label"] = json!("Approve release forty-two");
    button["confirm"] = json!({
        "title":{"type":"plain_text","text":"Ship &amp; notify?","emoji":false},
        "text":{"type":"mrkdwn","text":"*Approve* <https://example.test|release>?","verbatim":true},
        "confirm":{"type":"plain_text","text":"Yes"},
        "deny":{"type":"plain_text","text":"Wait"},"style":"danger"
    });
    let nodes = project(button);
    let meta = metadata(&nodes[0]);
    assert_eq!(
        meta.accessibility_label.as_deref(),
        Some("Approve release forty-two")
    );
    let BlockConfirmationState::Parsed(confirm) = &meta.confirm else {
        panic!("Expected confirmation")
    };
    let title = confirm.title.as_ref().unwrap();
    assert_eq!(title.text, "Ship &amp; notify?");
    assert_eq!(title.format, BlockTextFormat::PlainText);
    assert_eq!(title.emoji, Some(false));
    let text = confirm.text.as_ref().unwrap();
    assert_eq!(text.format, BlockTextFormat::Mrkdwn);
    assert_eq!(text.verbatim, Some(true));
    assert_eq!(text.text, "*Approve* <https://example.test|release>?");
    assert_eq!(confirm.confirm.as_ref().unwrap().text, "Yes");
    assert_eq!(confirm.deny.as_ref().unwrap().text, "Wait");
    assert_eq!(confirm.style.as_deref(), Some("danger"));
}

#[test]
fn malformed_confirmation_never_becomes_missing_confirmation() {
    assert_eq!(
        metadata(&project(button())[0]).confirm,
        BlockConfirmationState::Missing
    );
    for invalid in [Value::Null, json!(true), json!("confirm"), json!([])] {
        let mut button = button();
        button["confirm"] = invalid;
        assert_eq!(
            metadata(&project(button)[0]).confirm,
            BlockConfirmationState::Invalid
        );
    }
    let mut button = button();
    button["confirm"] = json!({"title":{"text":"Missing type"},
        "text":{"type":"future_text","text":"Keep me"},
        "confirm":{"type":"plain_text","text":12},"style":"future_style"});
    let nodes = project(button);
    let BlockConfirmationState::Parsed(confirm) = &metadata(&nodes[0]).confirm else {
        panic!("Expected partial confirmation")
    };
    assert_eq!(confirm.title, None);
    assert_eq!(confirm.confirm, None);
    assert_eq!(confirm.deny, None);
    assert_eq!(confirm.style.as_deref(), Some("future_style"));
    assert_eq!(
        confirm.text.as_ref().unwrap().format,
        BlockTextFormat::Unknown("future_text".into())
    );
}

#[test]
fn inert_controls_cannot_masquerade_as_buttons_and_urls_remain_unchanged() {
    let nodes = project(json!({"type":"actions","block_id":"actions","elements":[
        {"type":"static_select","action_id":"approve","value":"retained",
         "url":"https://example.test/must-stay-inert","placeholder":{"type":"plain_text","text":"Choose"}},
        {"type":"button","action_id":"approve","url":"https://example.test/open",
         "text":{"type":"plain_text","text":"Open"}}
    ]}));
    let [RichNode::Actions(elements)] = nodes.as_slice() else {
        unreachable!()
    };
    assert_eq!(
        metadata(&elements[0]).source,
        BlockElementSource::UnsupportedControl {
            kind: "static_select".into()
        }
    );
    assert_eq!(metadata(&elements[0]).value.as_deref(), Some("retained"));
    assert!(matches!(&elements[0], RichNode::Button { url: None, .. }));
    assert_eq!(metadata(&elements[1]).source, BlockElementSource::Button);
    assert!(
        matches!(&elements[1], RichNode::Button {url:Some(url),..} if url=="https://example.test/open")
    );
}

#[test]
fn restricted_buttons_are_visible_only_to_the_named_current_user() {
    for (restriction, expected_visible) in [
        (json!(["U0"]), true),
        (json!(["U1", "U0"]), true),
        (json!(["U1"]), false),
        (json!([]), false),
        (Value::Null, false),
        (json!("U0"), false),
        (json!(["U0", false]), false),
        (json!(["U0", ""]), false),
        (json!(["U0", "   "]), false),
    ] {
        let mut button = button();
        button["visible_to_user_ids"] = restriction.clone();
        assert_eq!(
            !project(button).is_empty(),
            expected_visible,
            "{restriction}"
        );
    }
    assert!(!project(button()).is_empty());
}

#[test]
fn hidden_accessory_and_action_rows_do_not_leave_interactive_elements() {
    let mut hidden = button();
    hidden["visible_to_user_ids"] = json!(["U1"]);
    let section = project(json!({"type":"section","block_id":"private", "accessory":hidden}));
    assert!(matches!(
        &section[0],
        RichNode::Section {
            accessory: None,
            ..
        }
    ));
    let actions = project(json!({"type":"actions","block_id":"private", "elements":[hidden]}));
    assert!(matches!(&actions[0], RichNode::Actions(elements) if elements.is_empty()));
}
