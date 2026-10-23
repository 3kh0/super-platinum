use super::*;
use serde_json::{Value, json};

fn options(raw: &Value) -> MessageLinkOptions {
    let flag = |key| raw.get(key).and_then(Value::as_bool).unwrap_or(false);
    MessageLinkOptions {
        ignore_phone_matches: flag("ignorePhoneMatches"),
        ignore_email_matches: flag("ignoreEmailMatches"),
        ignore_links_without_http: flag("ignoreLinksWithoutHttp"),
        include_phone_matches_with_tel_prefix: flag("includePhoneMatchesWithTelPrefix"),
        ignore_unbalanced_parentheses: flag("ignoreUnbalancedParentheses"),
    }
}
fn serialized(hits: &[MessageLink]) -> Value {
    Value::Array(hits.iter().map(|h|json!({"type":match h.kind {MessageLinkKind::Url=>"url",MessageLinkKind::Email=>"email",MessageLinkKind::Phone=>"phone"},"offset":h.offset_utf16,"text":h.text,"url":h.url})).collect())
}

#[test]
fn actual_vtg_flag_vectors_match_complete_untrimmed_utf16_results() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let rows = vectors["results"].as_array().unwrap();
    assert_eq!(rows.len(), 126);
    for row in rows {
        let result = detect(row["input"].as_str().unwrap(), options(&row["options"])).unwrap();
        assert_eq!(
            serialized(&result),
            row["matches"],
            "{} {}",
            row["profile"],
            row["input"]
        );
    }
}

#[test]
fn complete_scan_keeps_later_urls_and_offsets_without_changing_default_first_link() {
    let input = "  \u{1f600} https://example.com/a https://second.com/b  ";
    let hits = detect(input, MessageLinkOptions::default()).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].offset_utf16, 5);
    assert_eq!(hits[1].url, "https://second.com/b");
    let original = super::super::super::LinkValue::from_input(input, None)
        .unwrap()
        .unwrap();
    assert_eq!(original.original_url, hits[0].url);
}

#[test]
fn source_http_named_flag_requires_a_scheme_not_only_http_and_preserves_tel_capture_quirk() {
    let hits = detect(
        "example.com ftp://example.com",
        MessageLinkOptions {
            ignore_links_without_http: true,
            ignore_phone_matches: true,
            ignore_email_matches: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url, "ftp://example.com");
    let hits = detect(
        "tel:+1-212-555-0123 +1-212-555-0123",
        MessageLinkOptions {
            include_phone_matches_with_tel_prefix: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        hits.iter().map(|h| h.url.as_str()).collect::<Vec<_>>(),
        vec!["tel:+1-212-555-0123", "tel:12125550123"]
    );
}

#[test]
fn unbalanced_parenthesis_truncation_is_opt_in_and_input_bound_is_explicit() {
    let input = "https://example.com/a)b/c";
    assert_eq!(
        detect(input, MessageLinkOptions::default()).unwrap()[0].text,
        input
    );
    assert_eq!(
        detect(
            input,
            MessageLinkOptions {
                ignore_unbalanced_parentheses: true,
                ..Default::default()
            }
        )
        .unwrap()[0]
            .text,
        "https://example.com/a"
    );
    assert!(detect(&"x".repeat(262_145), MessageLinkOptions::default()).is_err());
    assert!(
        detect("", MessageLinkOptions::default())
            .unwrap()
            .is_empty()
    );
}
