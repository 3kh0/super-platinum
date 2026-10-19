use super::*;
use serde_json::json;

#[test]
fn effective_file_fields_join_existing_mixed_collation_without_title_substitution() {
    let mut fields = BTreeSet::from(["Existing member".into()]);
    let mut collation = BTreeSet::from(["Existing member".into()]);
    let raw = json!({"id":"FRENDER01","name":"Raw.canvas","title":"@Cafe\u{301}\nNext",
        "external_url":"https://example.com/Case","permalink":null,"facts":{"comparisonName":"spoof"}});
    let original = raw.clone();
    extend(&raw, &mut fields, &mut collation).unwrap();
    assert_eq!(raw, original);
    for text in [
        "Raw.canvas",
        "@Cafe\u{301}\nNext",
        "Cafe\u{301}\nNext",
        "Next",
        "",
        "https://example.com/Case",
    ] {
        assert!(fields.contains(text), "{text}");
    }
    assert_eq!(
        collation,
        BTreeSet::from(["Existing member".into(), "@Cafe\u{301}\nNext".into()])
    );
    assert!(!fields.contains("spoof"));
    extend(
        &json!({"name":"Fallback","title":""}),
        &mut fields,
        &mut collation,
    )
    .unwrap();
    assert!(collation.contains("Fallback"));
    assert!(extend(&json!({"title":42}), &mut fields, &mut collation).is_err());
}

#[test]
fn link_reply_requires_original_query_ticket_and_locale_including_false_result() {
    let make = || Reply {
        ticket: "ticket".into(),
        query: "example.com".into(),
        locale: "en-US".into(),
        has_link: false,
    };
    assert!(!decode("ticket", "example.com", "en-US", make()).unwrap());
    assert!(decode("successor", "example.com", "en-US", make()).is_err());
    assert!(decode("ticket", "changed.com", "en-US", make()).is_err());
    assert!(decode("ticket", "example.com", "tr-TR", make()).is_err());
}
