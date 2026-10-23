use super::*;
use serde_json::json;

#[test]
fn page_keeps_original_order_separate_from_mutable_typed_rows() {
    use crate::slack_lists::comments::{Page, Target};
    let (document, record, _) = crate::slack_lists::comments::tests::seed();
    let target = Target::capture(&document, &record).unwrap();
    let bytes = br#"{"ok":true,"messages":[{"type":"message","subtype":"list_record_comment","ts":"1700000000.000001","slack_list":{"list_id":"F01234567","list_record_id":"Rec1"},"text":"original","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"original","style":{"italic":true,"bold":true}}]}]}]}]}"#;
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert!(
        Page::parse(&target, value.clone())
            .unwrap()
            .ordered_response()
            .is_none()
    );
    let mut page =
        Page::parse_ordered(&target, ResponseWitness::decode(bytes, &value).unwrap()).unwrap();
    let cloned = page.clone();
    page.messages[0].text = Some("changed by current reconciliation".into());
    assert_ne!(page.messages()[0].text, cloned.messages()[0].text);
    assert_eq!(page.raw(), &value);
    assert_eq!(page.ordered_response().unwrap().response(), &value);
    let original = page.ordered_response().unwrap().message(0).unwrap();
    assert_eq!(
        original.style_keys(&[0, 0, 0]),
        Some(vec!["italic", "bold"])
    );
    assert!(!original.matches_seed(&serde_json::to_value(&page.messages()[0]).unwrap()));
    assert!(original.same_seed(&cloned.ordered_response().unwrap().message(0).unwrap()));
}

#[test]
fn ordered_page_still_requires_original_target_validation() {
    use crate::slack_lists::comments::{Page, Target};
    let (document, record, _) = crate::slack_lists::comments::tests::seed();
    let target = Target::capture(&document, &record).unwrap();
    let bytes = br#"{"ok":true,"messages":[{"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{"list_id":"F01234567","list_record_id":"OTHER"}}]}"#;
    let value = serde_json::from_slice(bytes).unwrap();
    assert!(Page::parse_ordered(&target, ResponseWitness::decode(bytes, &value).unwrap()).is_err());
}

fn raw(style: &str) -> Vec<u8> {
    format!(r#"{{"ok":true,"messages":[{{"ts":"1.000001","channel":"C1","thread_ts":"1.000000","user":"U1","text":"body","blocks":[{{"type":"rich_text","elements":[{{"type":"rich_text_section","elements":[{{"type":"text","text":"word","style":{style}}}]}}]}}],"attachments":[{{"opaque":{{"z":[true,1,null]}}}}],"extra":{{"x":1}}}}]}}"#).into_bytes()
}
fn witness(bytes: &[u8]) -> MessageWitness {
    let value = serde_json::from_slice(bytes).unwrap();
    ResponseWitness::decode(bytes, &value)
        .unwrap()
        .message(0)
        .unwrap()
}

#[test]
fn exact_raw_style_order_survives_sorted_value_equality() {
    let a = witness(&raw(r#"{"bold":true,"italic":true}"#));
    let b = witness(&raw(r#"{"italic":true,"bold":true}"#));
    assert_eq!(a.raw_message(), b.raw_message());
    assert_eq!(a.style_keys(&[0, 0, 0]), Some(vec!["bold", "italic"]));
    assert_eq!(b.style_keys(&[0, 0, 0]), Some(vec!["italic", "bold"]));
    assert!(!a.same_seed(&b));
    assert!(a.same_seed(&a.clone()));
}

#[test]
fn false_unknown_and_empty_style_keys_remain_positive_order_facts() {
    let a = witness(&raw(r#"{"italic":false,"future":null,"bold":true}"#));
    assert_eq!(
        a.style_keys(&[0, 0, 0]),
        Some(vec!["italic", "future", "bold"])
    );
    assert_eq!(witness(&raw("{}")).style_keys(&[0, 0, 0]), Some(vec![]));
    assert_eq!(a.style_keys(&[0, 0]), None);
    assert_eq!(a.style_keys(&[1]), None);
}

#[test]
fn javascript_array_index_keys_precede_insertion_order_string_keys() {
    let a = witness(&raw(
        r#"{"italic":true,"10":1,"2":2,"01":3,"bold":true,"4294967295":4,"0":0}"#,
    ));
    assert_eq!(
        a.style_keys(&[0, 0, 0]),
        Some(vec!["0", "2", "10", "italic", "01", "bold", "4294967295"])
    );
    let b = witness(&raw(
        r#"{"0":0,"2":2,"10":1,"italic":true,"01":3,"bold":true,"4294967295":4}"#,
    ));
    assert!(a.same_seed(&b));
}

#[test]
fn response_and_message_index_are_bound_to_the_same_original_bytes() {
    let bytes = raw("{}");
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    let original = value.clone();
    value["messages"][0]["extra"]["x"] = json!(2);
    assert!(ResponseWitness::decode(&bytes, &value).is_err());
    let r = ResponseWitness::decode(&bytes, &original).unwrap();
    assert_eq!(r.response(), &original);
    assert_eq!(
        r.message(0).unwrap().raw_message(),
        &original["messages"][0]
    );
    assert!(r.message(1).is_err());
}

#[test]
fn entire_seed_and_identity_changes_refuse_but_reaction_receipts_do_not_reseed() {
    let a = witness(&raw(r#"{"bold":true}"#));
    let mut reaction = a.raw_message().clone();
    reaction["reactions"] = json!([{"name":"eyes","count":1}]);
    assert!(a.matches_seed(&reaction));
    for key in SEED_FIELDS {
        let mut changed = a.raw_message().clone();
        changed[key] = json!({"changed":true});
        assert!(!a.matches_seed(&changed), "{key}");
    }
    let mut changed = a.raw_message().clone();
    changed["attachments"][0]["opaque"]["z"][1] = json!(2);
    assert!(!a.matches_seed(&changed));
    let mut changed = a.raw_message().clone();
    changed["blocks"][0]["elements"][0]["elements"][0]["text"] = json!("new");
    assert!(!a.matches_seed(&changed));
}

#[test]
fn duplicate_keys_failed_responses_and_wrong_shapes_never_produce_witnesses() {
    for text in [
        r#"{"ok":true,"messages":[],"ok":true}"#,
        r#"{"ok":true,"messages":[{"ts":"1","style":{"a":1,"a":2}}]}"#,
        r#"{"ok":false,"messages":[]}"#,
        r#"{"ok":true,"messages":{}}"#,
        r#"[]"#,
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        assert!(
            ResponseWitness::decode(text.as_bytes(), &value).is_err(),
            "{text}"
        );
    }
    assert!(ResponseWitness::decode(&vec![b' '; LIMIT + 1], &Value::Null).is_err());
    let text = br#"{"ok":true,"messages":[{}]}"#;
    assert!(
        ResponseWitness::decode(text, &serde_json::from_slice(text).unwrap())
            .unwrap()
            .message(0)
            .is_err()
    );
}

#[test]
fn positive_reply_envelope_refuses_every_nonnull_error_without_fabricating_success() {
    for error in [
        json!("failed"),
        json!(""),
        json!(false),
        json!({}),
        json!([]),
    ] {
        let value = json!({"ok":true,"error":error,"messages":[]});
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(ResponseWitness::decode(&bytes, &value).is_err());
    }
    for value in [
        json!({"ok":true,"messages":[]}),
        json!({"ok":true,"error":null,"messages":[]}),
    ] {
        assert!(ResponseWitness::decode(&serde_json::to_vec(&value).unwrap(), &value).is_ok());
    }
}
