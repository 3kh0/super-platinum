//! A screenshot may cross a positive countdown tick, never deadline retirement.
use serde_json::Value;

pub(super) fn unchanged(
    stage: &str,
    before: &Value,
    after: &Value,
    journal: &Value,
    journal_after: &Value,
) -> bool {
    if journal != journal_after {
        return false;
    }
    if before == after {
        return true;
    }
    let (Some(deadline), Some(remaining), Some(next_remaining)) = (
        before["comments"]["retryAt"].as_i64(),
        before["comments"]["retryAfter"].as_i64(),
        after["comments"]["retryAfter"].as_i64(),
    ) else {
        return false;
    };
    if stage != "rate-limited"
        || deadline <= 0
        || after["comments"]["retryAt"] != deadline
        || remaining <= 0
        || next_remaining <= 0
        || next_remaining > remaining
        || before["actionProbe"]["available"] != false
        || after["actionProbe"]["available"] != false
    {
        return false;
    }
    let mut comparable = after.clone();
    comparable["comments"]["retryAfter"] = before["comments"]["retryAfter"].clone();
    comparable == *before
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn initial() -> Value {
        json!({"comments":{"retryAt":1792386246_i64,"retryAfter":2,"read":17,"page":{"messages":[{"text":"Original"}]}},"actionProbe":{"available":false,"own":false,"count":2},"draftStore":{"blocks":[{"text":"Rich draft"}],"revision":4},"ledger":{"requests":16}})
    }

    #[test]
    fn positive_countdown_tick_preserves_exact_source_and_journal() {
        let before = initial();
        let mut after = before.clone();
        after["comments"]["retryAfter"] = json!(1);
        let journal = json!({"blocks":[{"text":"Rich draft"}],"revision":4});
        assert!(unchanged(
            "rate-limited",
            &before,
            &after,
            &journal,
            &journal
        ));
        assert!(!unchanged(
            "rate-expired",
            &before,
            &after,
            &journal,
            &journal
        ));
        assert!(!unchanged(
            "rate-limited",
            &before,
            &after,
            &journal,
            &json!({"revision":5})
        ));
    }

    #[test]
    fn countdown_never_masks_expiry_deadline_or_authority_changes() {
        let before = initial();
        for (path, value) in [
            ("/comments/retryAfter", json!(0)),
            ("/comments/retryAfter", json!(3)),
            ("/comments/retryAfter", json!("1")),
            ("/comments/retryAt", json!(0)),
            ("/comments/retryAt", json!(1792386247_i64)),
            ("/comments/read", json!(18)),
            ("/comments/page/messages/0/text", json!("Changed")),
            ("/actionProbe/available", json!(true)),
            ("/actionProbe/own", json!(true)),
            ("/actionProbe/count", json!(3)),
            ("/draftStore/blocks/0/text", json!("Changed")),
            ("/draftStore/revision", json!(5)),
            ("/ledger/requests", json!(17)),
        ] {
            let mut after = before.clone();
            after["comments"]["retryAfter"] = json!(1);
            *after.pointer_mut(path).unwrap() = value;
            assert!(
                !unchanged("rate-limited", &before, &after, &Value::Null, &Value::Null),
                "{path}"
            );
        }
    }

    #[test]
    fn unchanged_expired_stage_is_valid_but_expiry_transition_is_not() {
        let before = initial();
        let mut expired = before.clone();
        expired["comments"]["retryAfter"] = json!(0);
        expired["comments"]["retryAt"] = json!(0);
        expired["actionProbe"]["available"] = json!(true);
        assert!(!unchanged(
            "rate-limited",
            &before,
            &expired,
            &Value::Null,
            &Value::Null
        ));
        assert!(unchanged(
            "rate-expired",
            &expired,
            &expired,
            &Value::Null,
            &Value::Null
        ));
    }
}
