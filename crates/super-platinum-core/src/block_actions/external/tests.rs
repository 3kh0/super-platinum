use super::*;
use serde_json::json;
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":"Exact &amp; text","emoji":false},"value":value,"description":{"type":"plain_text","text":"Description"}})
}
#[test]
fn raw_option_preserves_metadata_and_empty_value_is_not_null() {
    let raw = option("");
    let parsed = ExternalOption::parse(&raw).unwrap();
    assert_eq!(parsed.raw(), &raw);
    assert_eq!(parsed.value(), "");
    assert_eq!(parsed.description(), Some("Description"));
    for key in ["text", "description"] {
        let mut raw = raw.clone();
        raw[key]["text"] = json!("");
        assert!(ExternalOption::parse(&raw).is_err());
    }
    for (key, value) in [
        ("url", json!("https://example.test")),
        ("unknown", json!(true)),
    ] {
        let mut raw = raw.clone();
        raw[key] = value;
        assert_eq!(ExternalOption::parse(&raw).unwrap().raw(), &raw);
    }
    let mut raw = raw.clone();
    raw["text"]["type"] = json!("mrkdwn");
    assert!(ExternalOption::parse(&raw).is_err());
}
#[test]
fn utf16_constraints_and_group_precedence_are_bounded() {
    let mut raw = option("v");
    raw["text"]["text"] = json!("\u{1F600}".repeat(37) + "a");
    assert!(ExternalOption::parse(&raw).is_ok());
    raw["text"]["text"] = json!("\u{1F600}".repeat(38));
    assert!(ExternalOption::parse(&raw).is_err());
    assert_eq!(
        ExternalSuggestions::parse(&json!({"option_groups":[],"options":"ignored"})).unwrap(),
        ExternalSuggestions::Groups(vec![])
    );
    for bad in [json!(false), json!(0), json!(""), json!({})] {
        assert!(ExternalSuggestions::parse(&json!({"option_groups":bad,"options":[]})).is_err());
    }
    assert!(ExternalSuggestions::parse(&json!({"options":vec![option("v");101]})).is_err());
    assert!(
        ExternalSuggestions::parse(
            &json!({"option_groups":[{"label":{"type":"plain_text","text":""},"options":[]}]})
        )
        .is_err()
    );
}
#[test]
fn ordered_groups_retain_raw_and_reject_conflicting_values() {
    let a = option("a");
    let b = option("b");
    let response =
        json!({"option_groups":[{"label":{"type":"plain_text","text":"First"},"options":[b,a]}]});
    let ExternalSuggestions::Groups(groups) = ExternalSuggestions::parse(&response).unwrap() else {
        panic!()
    };
    assert_eq!(groups[0].options[0].raw(), &b);
    assert_eq!(groups[0].options[1].raw(), &a);
    let mut conflicting = a.clone();
    conflicting["text"]["text"] = json!("Changed");
    assert!(ExternalSuggestions::parse(&json!({"options":[a,conflicting]})).is_err());
}

#[test]
fn opaque_metadata_survives_query_groups_without_becoming_display_or_identity() {
    let mut raw = option("v");
    raw["opaque"] = json!({"kept":[null,false,7,{"nested":"exact"}]});
    raw["url"] = json!("javascript:must-not-execute");
    raw["html"] = json!("<img src=untrusted>");
    let ExternalSuggestions::Options(options) =
        ExternalSuggestions::parse(&json!({"options":[raw.clone()]})).unwrap()
    else {
        panic!()
    };
    assert_eq!(options[0].raw(), &raw);
    assert_eq!(options[0].text(), "Exact &amp; text");
    assert_eq!(options[0].description(), Some("Description"));
    assert_eq!(options[0].value(), "v");
    let ExternalSuggestions::Groups(groups) = ExternalSuggestions::parse(&json!({"option_groups":[{"label":{"type":"plain_text","text":"Group"},"options":[raw.clone()]}]})).unwrap() else {panic!()};
    assert_eq!(groups[0].options[0].raw(), &raw);
    let mut changed = raw.clone();
    changed["opaque"]["kept"][2] = json!(8);
    assert_ne!(
        ExternalOption::parse(&raw).unwrap(),
        ExternalOption::parse(&changed).unwrap()
    );
    assert!(ExternalSuggestions::parse(&json!({"options":[raw,changed]})).is_err());
}

#[test]
fn opaque_metadata_cannot_replace_or_relax_known_field_validation() {
    for (key, value) in [
        ("text", json!({"type":"mrkdwn","text":"invalid"})),
        (
            "text",
            json!({"type":"plain_text","text":"valid","emoji":"true"}),
        ),
        ("text", json!({"type":"plain_text","text":"x".repeat(76)})),
        ("value", json!(null)),
        ("value", json!(true)),
        ("value", json!("x".repeat(151))),
        ("description", json!({"type":"plain_text","text":""})),
        (
            "description",
            json!({"type":"plain_text","text":"x".repeat(76)}),
        ),
    ] {
        let mut raw = option("v");
        raw["opaque"] = json!({"valid":true});
        raw[key] = value;
        assert!(ExternalOption::parse(&raw).is_err(), "{key}");
    }
}
