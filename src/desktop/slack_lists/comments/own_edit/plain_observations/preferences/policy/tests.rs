use super::*;
use serde_json::json;

fn decode(prefs: Value, assignments: Value) -> NativePolicy {
    native_policy(
        &json!({"ok":true,"prefs":prefs}),
        &json!({"ok":true,"assignments":assignments}),
    )
    .unwrap()
}

#[test]
fn successful_complete_absence_differs_from_unavailable_carrier() {
    let empty = decode(json!({}), json!({}));
    assert_eq!(
        serde_json::to_value(&empty).unwrap(),
        json!({
            "plainTextMode":false,"markdownMode":false,"supportsPortableContent":false,
            "slugsEnabled":true,"emojiAsText":false,"convertEmoticons":false,
            "composerSmartSymbols":false,"disableSmartSymbols":false,"detectGoLinks":false,
            "enterCreatesNewline":false,"usePreNewlines":false,"spellcheck":false,
            "underlineLinks":false,"flattenTreeListBeforePaste":false
        })
    );
    for raw in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":false,"prefs":{}}),
        json!({"ok":true,"prefs":null}),
        json!({"ok":true,"prefs":[]}),
        json!({"ok":true,"prefs":{},"error":"missing_scope"}),
    ] {
        assert!(native_policy(&raw, &json!({"ok":true,"assignments":{}})).is_err());
    }
    for raw in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true,"assignments":null}),
        json!({"ok":true,"assignments":[]}),
        json!({"ok":true,"assignments":{},"error":"not_authed"}),
    ] {
        assert!(native_policy(&json!({"ok":true,"prefs":{}}), &raw).is_err());
    }
}

#[test]
fn preference_truthiness_is_source_javascript_not_boolean_only_or_string_parsing() {
    for (value, expected) in [
        (Value::Null, false),
        (json!(false), false),
        (json!(0), false),
        (json!(-0.0), false),
        (json!(""), false),
        (json!(true), true),
        (json!(1), true),
        (json!(-2), true),
        (json!(0.25), true),
        (json!("false"), true),
        (json!([]), true),
        (json!({}), true),
    ] {
        let policy = decode(
            json!({
                "plain_text_mode":value,"convert_emoticons":value,"disable_smart_symbols":value,
                "msg_input_send_btn":value,"enter_is_special_in_tbt":value,
                "webapp_spellcheck":value,"underline_links":value
            }),
            json!({"portable_content_composer":{"group":"treatment"}}),
        );
        assert_eq!(policy.plain_text_mode, expected, "{value}");
        assert_eq!(policy.convert_emoticons, expected);
        assert_eq!(policy.disable_smart_symbols, expected);
        assert_eq!(policy.enter_creates_newline, expected);
        assert_eq!(policy.use_pre_newlines, expected);
        assert_eq!(policy.spellcheck, expected);
        assert_eq!(policy.underline_links, expected);
        assert_eq!(policy.supports_portable_content, !expected);
        assert!(!policy.markdown_mode); // Plain pref is not local Markdown state.
    }
}

#[test]
fn exact_assignment_and_emoji_equalities_preserve_observed_nonmatching_values() {
    for value in [
        Value::Null,
        json!(false),
        json!(true),
        json!(1),
        json!("on"),
        json!([]),
        json!({}),
        json!({"group":null}),
        json!({"group":true}),
        json!({"group":"ON"}),
        json!({"group":"treatment"}),
    ] {
        let policy = decode(
            json!({}),
            json!({"composer_smart_symbols":value,
            "detect_go_links":value,"fe_flatten_tree_list_before_paste":value}),
        );
        assert!(
            !policy.composer_smart_symbols
                && !policy.detect_go_links
                && !policy.flatten_tree_list_before_paste,
            "{value}"
        );
    }
    for value in [
        Value::Null,
        json!(false),
        json!(true),
        json!("As_Text"),
        json!("native"),
        json!([]),
        json!({}),
        json!(0),
    ] {
        let prefs = json!({"emoji_mode":value});
        assert!(!decode(prefs.clone(), json!({})).emoji_as_text);
        assert!(!super::super::emoji_as_text(&json!({"ok":true,"prefs":prefs})).unwrap());
    }
    assert!(decode(json!({"emoji_mode":"as_text"}), json!({})).emoji_as_text);
}

#[test]
fn enabled_unsupported_native_branches_are_retained_without_mount_overrides() {
    let prefs = json!({"plain_text_mode":true,"emoji_mode":"native","convert_emoticons":true,
        "disable_smart_symbols":false,"msg_input_send_btn":true});
    let assignments = json!({"portable_content_composer":{"group":"treatment"},
        "composer_smart_symbols":{"group":"on"},"detect_go_links":{"group":"on"},
        "fe_flatten_tree_list_before_paste":{"group":"on"}});
    let before = (prefs.clone(), assignments.clone());
    let policy = native_policy(
        &json!({"ok":true,"prefs":prefs}),
        &json!({"ok":true,"assignments":assignments}),
    )
    .unwrap();
    assert!(policy.plain_text_mode && policy.slugs_enabled && policy.convert_emoticons);
    assert!(!policy.emoji_as_text && !policy.disable_smart_symbols);
    assert!(
        policy.composer_smart_symbols
            && policy.detect_go_links
            && policy.flatten_tree_list_before_paste
    );
    assert!(!policy.supports_portable_content && !policy.markdown_mode);
    assert_eq!((prefs, assignments), before);
}
