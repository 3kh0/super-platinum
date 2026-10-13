use serde_json::json;

use super::*;
use crate::model::{ButtonStyle, RichNode};

fn projected(value: Value) -> String {
    let core = crate::fixture::fixture_core();
    let media = crate::media::MediaRegistry::default();
    let nodes = super::super::block_nodes(BlockCtx::new(&core.workspaces["T1"], &media), &value);
    let [
        RichNode::Button {
            label,
            url: None,
            style: ButtonStyle::Default,
            ..
        },
    ] = nodes.as_slice()
    else {
        panic!("Expected an inert control: {nodes:?}");
    };
    label.clone()
}

#[test]
fn selected_options_win_over_placeholder_without_activating_option_urls() {
    for kind in ["static_select", "external_select", "radio_buttons"] {
        assert_eq!(
            projected(json!({
                "type":kind, "placeholder":{"type":"plain_text","text":"Choose"},
                "initial_option":{"text":{"type":"plain_text","text":"Ship :rocket:"},
                                  "value":"opaque", "url":"https://example.test"},
                "url":"https://example.test/also-ignored"
            })),
            "Ship 🚀"
        );
    }
}

#[test]
fn multiple_selected_options_preserve_order_and_skip_malformed_labels() {
    for kind in ["multi_static_select", "multi_external_select", "checkboxes"] {
        assert_eq!(
            projected(json!({"type":kind,"initial_options":[
                {"text":{"type":"plain_text","text":"Design"}},
                {"value":"not a display label"},
                {"text":{"type":"plain_text","text":"   "}},
                {"text":{"type":"plain_text","text":"Engineering"}}
            ]})),
            "Design, Engineering"
        );
    }
}

#[test]
fn selected_users_and_conversations_resolve_cached_names() {
    for (value, expected) in [
        (
            json!({"type":"users_select","initial_user":"U1"}),
            "@Maya Chen",
        ),
        (
            json!({"type":"multi_users_select","initial_users":["U1",null,"","U404"]}),
            "@Maya Chen, @U404",
        ),
        (
            json!({"type":"channels_select","initial_channel":"C2"}),
            "#ship",
        ),
        (
            json!({"type":"multi_channels_select","initial_channels":["C2","C1"]}),
            "#ship, #general",
        ),
        (
            json!({"type":"conversations_select","initial_conversation":"D1"}),
            "Maya Chen",
        ),
        (
            json!({"type":"multi_conversations_select","initial_conversations":["D1","C2","C404"]}),
            "Maya Chen, #ship, C404",
        ),
    ] {
        assert_eq!(projected(value), expected);
    }
}

#[test]
fn current_conversation_defaults_respect_single_and_multi_precedence() {
    assert_eq!(
        projected(
            json!({"type":"conversations_select","initial_conversation":"C1",
        "default_to_current_conversation":true})
        ),
        "#general"
    );
    assert_eq!(
        projected(
            json!({"type":"multi_conversations_select","initial_conversations":["C1"],
        "default_to_current_conversation":true})
        ),
        "Select"
    );
    for kind in ["conversations_select", "multi_conversations_select"] {
        assert_eq!(
            projected(json!({"type":kind,"default_to_current_conversation":true,
                "placeholder":{"type":"plain_text","text":"Choose conversation"}
            })),
            "Choose conversation"
        );
    }
}

#[test]
fn supplied_date_time_and_text_values_remain_readable() {
    for (value, expected) in [
        (
            json!({"type":"datepicker","initial_date":"2026-10-13"}),
            "2026-10-13",
        ),
        (json!({"type":"timepicker","initial_time":"14:30"}), "14:30"),
        (
            json!({"type":"plain_text_input","initial_value":"<script>literal</script>\n:rocket:"}),
            "<script>literal</script>\n:rocket:",
        ),
    ] {
        assert_eq!(projected(value), expected);
    }
    assert_eq!(
        projected(json!({"type":"timepicker","initial_time":"14:30","timezone":"America/Chicago"})),
        "14:30 (America/Chicago)"
    );
    let ts = "1791896400";
    assert_eq!(
        projected(json!({"type":"datetimepicker","initial_date_time":1791896400})),
        format!(
            "{} {}",
            super_platinum_core::state::date_key_for_ts(ts).unwrap(),
            super_platinum_core::state::format_ts_hm(ts)
        )
    );
}

#[test]
fn absent_or_malformed_values_fall_back_without_leaking_machine_values() {
    for value in [
        json!({"type":"static_select","initial_option":{"value":"opaque"}}),
        json!({"type":"multi_external_select","initial_options":[]}),
        json!({"type":"multi_users_select","initial_users":"U1"}),
        json!({"type":"datetimepicker","initial_date_time":-1}),
        json!({"type":"datetimepicker","initial_date_time":u64::MAX}),
        json!({"type":"datepicker","initial_date":42}),
        json!({"type":"timepicker","initial_time":""}),
        json!({"type":"plain_text_input","initial_value":true}),
    ] {
        let mut value = value;
        value["placeholder"] = json!({"type":"plain_text","text":"Choose"});
        assert_eq!(projected(value), "Choose");
    }
    assert_eq!(projected(json!({"type":"multi_external_select"})), "Select");
    assert_eq!(projected(json!({"type":"overflow"})), "More");
}

#[test]
fn composition_labels_decode_slack_escapes_once_without_creating_links() {
    assert_eq!(
        projected(json!({"type":"static_select","initial_option":{
        "text":{"type":"plain_text","text":"R&amp;D &lt;beta&gt; &amp;lt; &quot; &lt;https://example.test|Link&gt;"},
        "value":"opaque"}})),
        "R&D <beta> &lt; &quot; <https://example.test|Link>"
    );
    assert_eq!(
        projected(json!({"type":"multi_external_select","placeholder":{
        "type":"plain_text","text":"R&amp;D"}})),
        "R&D"
    );
}
