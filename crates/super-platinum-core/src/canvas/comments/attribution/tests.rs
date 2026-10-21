use super::*;
use crate::block_actions::name_policy::{Membership, OrdinaryClientV2, Snapshot, UserPrefs};
use serde_json::json;
use std::collections::BTreeMap;

fn raw() -> Value {
    json!({"type":"message","subtype":"document_comment_root","channel":"C123",
        "ts":"10.000001","thread_ts":"10.000001","user":"UAUTHOR","text":"original",
        "document_comment":{"thread_id":"original-control","most_recent_editor":"UEDITOR",
            "authors":["UOTHER"],"opaque":{"retained":true}}})
}
fn message(raw: Value) -> Message {
    serde_json::from_value(raw).unwrap()
}
fn policy(prefs: Value) -> Snapshot {
    let membership = Membership::observe(
        None,
        &["T1".into()],
        &BTreeMap::from([("T1".into(), json!({"id":"T1"}))]),
    )
    .unwrap();
    Snapshot::new(
        OrdinaryClientV2,
        membership,
        UserPrefs::decode(&json!({"ok":true,"prefs":prefs})).unwrap(),
        None,
    )
    .unwrap()
}
fn member() -> Value {
    json!({"id":"UEDITOR","name":"username-must-not-be-used","real_name":"Top Real",
        "profile":{"display_name":" Display ","real_name":"Profile Real"}})
}

#[test]
fn acquired_root_borrows_editor_and_preserves_original_page_identity() {
    let page = super::super::ReplyPage::parse(
        "F123",
        "10.000001",
        true,
        &json!({"ok":true,"messages":[raw()],"has_more":true,
            "response_metadata":{"next_cursor":"next"}}),
    )
    .unwrap();
    let original = &page.messages[0];
    let before = serde_json::to_value(original).unwrap();
    let selected = editor(original, Context::Thread, false).unwrap().unwrap();
    assert_eq!(selected.id(), "UEDITOR");
    assert!(std::ptr::eq(selected.message(), original));
    assert!(std::ptr::eq(
        selected.id(),
        original.extra["document_comment"]["most_recent_editor"]
            .as_str()
            .unwrap()
    ));
    assert_eq!(selected.message().user.as_deref(), Some("UAUTHOR"));
    assert_eq!(
        selected.message().extra["document_comment"]["thread_id"],
        "original-control"
    );
    assert_eq!(serde_json::to_value(original).unwrap(), before);
    assert_eq!(page.cursor.as_deref(), Some("next"));
}

#[test]
fn only_thread_visible_roots_admit_attribution() {
    let original = message(raw());
    assert!(editor(&original, Context::Stream, false).unwrap().is_none());
    assert!(editor(&original, Context::Thread, true).unwrap().is_none());
    for (key, value) in [
        ("subtype", json!("document_comment")),
        ("subtype", Value::Null),
        ("thread_ts", json!("9.000001")),
        ("document_comment", json!({"most_recent_editor":"UEDITOR"})),
    ] {
        let mut value_raw = raw();
        value_raw[key] = value;
        assert!(
            editor(&message(value_raw), Context::Thread, false)
                .unwrap()
                .is_none()
        );
    }
    for visible in [
        Value::Null,
        json!(true),
        json!(0),
        json!("false"),
        json!({}),
    ] {
        let mut value = raw();
        value["document_comment"]["is_visible"] = visible;
        assert!(
            editor(&message(value), Context::Thread, false)
                .unwrap()
                .is_some()
        );
    }
    let mut hidden = raw();
    hidden["document_comment"]["is_visible"] = json!(false);
    // Visibility also wins over an unsupported editor shape.
    hidden["document_comment"]["most_recent_editor"] = json!({});
    assert!(
        editor(&message(hidden), Context::Thread, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn falsey_editor_never_falls_back_to_authors_or_original_author() {
    for value in [Value::Null, json!(false), json!(0), json!("")] {
        let mut original = raw();
        original["document_comment"]["most_recent_editor"] = value;
        assert!(
            editor(&message(original), Context::Thread, false)
                .unwrap()
                .is_none()
        );
    }
    let mut original = raw();
    original["document_comment"]
        .as_object_mut()
        .unwrap()
        .remove("most_recent_editor");
    assert!(
        editor(&message(original), Context::Thread, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn malformed_truthy_editor_is_not_a_send_or_profile_identity() {
    for value in [
        json!(true),
        json!(1),
        json!([]),
        json!({}),
        json!(" UEDITOR"),
        json!("U\nEDITOR"),
        json!("U".repeat(129)),
    ] {
        let mut original = raw();
        original["document_comment"]["most_recent_editor"] = value;
        assert_eq!(
            editor(&message(original), Context::Thread, false).unwrap_err(),
            Error::Editor
        );
    }
}

#[test]
fn observed_display_real_and_hidden_names_use_shared_policy_without_trimming() {
    let original = message(raw());
    let selected = editor(&original, Context::Thread, false).unwrap().unwrap();
    let display = policy(json!({"display_real_names_override":-1}));
    let real = policy(json!({"display_real_names_override":1}));
    assert_eq!(
        selected.name(&member(), &display, "Name hidden"),
        Ok(Name::Known(" Display ".into()))
    );
    assert_eq!(
        selected.name(&member(), &real, "Name hidden"),
        Ok(Name::Known("Profile Real".into()))
    );
    let mut top = member();
    top["profile"]["real_name"] = json!("");
    assert_eq!(
        selected.name(&top, &real, "Name hidden"),
        Ok(Name::Known("Top Real".into()))
    );
    let hidden = policy(json!({"hidden_users":"UEDITOR"}));
    assert_eq!(
        selected.name(&member(), &hidden, "Observed hidden label"),
        Ok(Name::Known("Observed hidden label".into()))
    );
}

#[test]
fn unknown_nonexistent_and_known_empty_are_distinct_render_branches() {
    let original = message(raw());
    let selected = editor(&original, Context::Thread, false).unwrap().unwrap();
    let observed = policy(json!({"display_real_names_override":-1}));
    for marker in [json!(true), json!(1), json!("yes"), json!({})] {
        assert_eq!(
            selected.name(
                &json!({"id":"UEDITOR","isUnknown":marker,"isNonExistent":true}),
                &observed,
                "Name hidden"
            ),
            Ok(Name::Unknown)
        );
    }
    let absent = json!({"id":"UEDITOR","isNonExistent":true});
    assert_eq!(
        selected.name(&absent, &observed, "Name hidden"),
        Ok(Name::Nonexistent)
    );
    let hidden = policy(json!({"hidden_users":"UEDITOR"}));
    assert_eq!(
        selected.name(&absent, &hidden, "Name hidden"),
        Ok(Name::Known("Name hidden".into()))
    );
    assert_eq!(
        selected.name(
            &json!({"id":"UEDITOR","name":"not-a-fallback"}),
            &observed,
            "Name hidden"
        ),
        Ok(Name::Known(String::new()))
    );
    let mut named_nonexistent = member();
    named_nonexistent["isNonExistent"] = json!(true);
    assert_eq!(
        selected.name(&named_nonexistent, &observed, "Name hidden"),
        Ok(Name::Known(" Display ".into()))
    );
}

#[test]
fn foreign_missing_or_malformed_member_and_unobserved_policy_are_not_defaults() {
    let original = message(raw());
    let selected = editor(&original, Context::Thread, false).unwrap().unwrap();
    let observed = policy(json!({"display_real_names_override":-1}));
    for member in [
        Value::Null,
        json!({}),
        json!({"id":"UOTHER","profile":{"display_name":"Foreign"}}),
    ] {
        assert_eq!(
            selected.name(&member, &observed, "Name hidden"),
            Err(Error::Member)
        );
    }
    let mut malformed = member();
    malformed["profile"]["display_name"] = json!([]);
    assert_eq!(
        selected.name(&malformed, &observed, "Name hidden"),
        Err(Error::Policy(name_policy::Error::Shape))
    );
    assert_eq!(
        selected.name(&member(), &policy(json!({})), "Name hidden"),
        Err(Error::Policy(name_policy::Error::MissingPreference))
    );
    // A changed root cannot reuse the former editor's member observation.
    let mut updated = raw();
    updated["document_comment"]["most_recent_editor"] = json!("UNEW");
    let updated = message(updated);
    let new_editor = editor(&updated, Context::Thread, false).unwrap().unwrap();
    assert_eq!(
        new_editor.name(&member(), &observed, "Name hidden"),
        Err(Error::Member)
    );
    assert_eq!(selected.id(), "UEDITOR");
}
