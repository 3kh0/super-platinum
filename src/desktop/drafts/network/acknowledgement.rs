//! Raw server bodies cannot establish transport predispatch provenance.
//! Unknown fields may describe another mutation outcome: retain uncertainty.
use super::*;

fn strings(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_string))
}

pub(super) fn envelope(value: &Value, payload_fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.iter().all(|(key, value)| match key.as_str() {
            "ok" | "error" => true, // Outcome-specific validation belongs to the caller.
            "needed" | "provided" | "warning" => value.is_string(),
            "warnings" => strings(value),
            "response_metadata" => value.as_object().is_some_and(|metadata| {
                metadata.iter().all(|(key, value)| {
                    matches!(key.as_str(), "messages" | "warnings") && strings(value)
                })
            }),
            field => payload_fields.contains(&field),
        })
    })
}

pub(super) fn denial(raw: Value, now_ms: u64) -> Attempt {
    let code = raw.get("error").and_then(Value::as_str);
    let clean = raw.get("ok").and_then(Value::as_bool) == Some(false)
        && code.is_some_and(|code| !code.is_empty())
        && envelope(&raw, &[]);
    let rate_limited = matches!(code, Some("ratelimited" | "rate_limited"));
    let rejected = clean && code.is_some_and(definite_rejection);
    Attempt {
        outcome: if rejected {
            Outcome::Rejected
        } else {
            Outcome::Uncertain
        },
        issue: Some(if !clean {
            Issue::MalformedResponse
        } else if rejected {
            Issue::Rejected
        } else if rate_limited {
            Issue::RateLimited
        } else {
            Issue::Uncertain
        }),
        // Even a contradictory rate-limit body cannot shorten throttling.
        cooldown_until_ms: rate_limited.then(|| now_ms.saturating_add(30_000)),
        dispatched: true,
        raw: Some(raw),
    }
}
