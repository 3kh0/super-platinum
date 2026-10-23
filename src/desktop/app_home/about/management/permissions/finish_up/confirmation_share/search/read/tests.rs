use super::*;

#[test]
fn malformed_page_evidence_keeps_exact_raw_and_original_object_order() {
    let raw = json!({"ok":true,"results":{"z":[{"id":"UZ"}],"a":[{"id":"UA"}]},"can_interact":{"UZ":false},"next_marker":42,"opaque":{"original":true}});
    let keys = vec!["z".to_owned(), "a".to_owned()];
    let response = Ok((raw.clone(), Some(keys.clone())));
    let mut reads = vec![];
    record(
        &mut reads,
        Kind::Members,
        "users/search",
        Some(&json!({"query":"Dana"})),
        &response,
    );
    assert_eq!(reads[0]["raw"], raw);
    assert_eq!(reads[0]["orderedKeys"], json!(keys));
    let mut pages = search::Pages::default();
    assert!(pages.accept(raw, Some(&keys)).is_err());
    assert!(!pages.complete());
    assert!(pages.rows().is_err());
    assert_eq!(reads.len(), 1);
}

#[test]
fn transport_failure_evidence_has_no_fabricated_reply_or_sensitive_diagnostic() {
    let mut reads = vec![];
    let response = Err(Error::Offline(
        "synthetic signed URL must not be exposed".into(),
    ));
    record(
        &mut reads,
        Kind::Channels,
        "channels/search",
        Some(&json!({"query":"research"})),
        &response,
    );
    assert_eq!(reads[0]["failure"], json!({"kind":"offline"}));
    assert!(reads[0].get("raw").is_none());
    assert!(reads[0].get("orderedKeys").is_none());
    assert!(!reads[0].to_string().contains("signed URL"));
}

#[test]
fn rate_limit_and_timeout_remain_distinct_from_empty_success() {
    assert_eq!(
        failure(&Error::RateLimited {
            retry_after_secs: Some(12)
        }),
        json!({"kind":"rate-limited","retryAfterSeconds":12})
    );
    assert_eq!(
        failure(&Error::Api("workflow_share_search_timeout".into())),
        json!({"kind":"timeout"})
    );
    assert_eq!(failure(&retired()), json!({"kind":"retired"}));
}
