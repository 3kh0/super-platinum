//! Closed evidence mapping. No workflow or absent-object success inference.
use super::*;
use crate::drafts::{
    dispatch::Outcome,
    network::{Issue, Readback},
};

pub(super) fn mutation(outcome: &Outcome) -> durable::Outcome {
    match outcome {
        Outcome::Draft(remote) => durable::Outcome::Active(remote.clone()),
        Outcome::Deleted { draft_id } => durable::Outcome::Deleted {
            draft_id: draft_id.clone(),
        },
        Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        } => durable::Outcome::Sent {
            client_msg_id: client_msg_id.clone(),
            draft_id: draft_id.clone(),
            ts: ts.clone(),
        },
        Outcome::Scheduled { draft, post_at } => durable::Outcome::Scheduled {
            remote: draft.clone(),
            post_at: *post_at,
        },
        Outcome::Rejected => durable::Outcome::Rejected,
        Outcome::Uncertain => durable::Outcome::Uncertain,
    }
}
pub(super) fn check(read: &Readback) -> durable::CheckResult {
    if let Some(outcome) = &read.outcome {
        return match outcome {
            Outcome::Draft(_) | Outcome::Sent { .. } | Outcome::Scheduled { .. } => {
                durable::CheckResult::Positive(mutation(outcome))
            }
            Outcome::Deleted { .. } | Outcome::Rejected | Outcome::Uncertain => {
                durable::CheckResult::Nonpositive(durable::KnownReadFailure::Unsupported)
            }
        };
    }
    durable::CheckResult::Nonpositive(if !read.requested {
        durable::KnownReadFailure::NotRequested
    } else {
        match read.issue {
            Some(Issue::RateLimited) => durable::KnownReadFailure::RateLimited,
            Some(Issue::MalformedResponse | Issue::UnsupportedResponse) => {
                durable::KnownReadFailure::Unsupported
            }
            _ if read.raw.is_none() => durable::KnownReadFailure::Unavailable,
            _ if read.raw.as_ref().is_some_and(|raw| {
                raw.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
                    || raw.get("error").is_some_and(|value| !value.is_null())
            }) =>
            {
                durable::KnownReadFailure::ApiError
            }
            Some(_) => durable::KnownReadFailure::Unsupported,
            None => durable::KnownReadFailure::Missing,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lack_of_positive_evidence_never_becomes_rejection() {
        let mut read = Readback {
            outcome: None,
            raw: None,
            issue: None,
            cooldown_until_ms: None,
            requested: false,
            has_more: true,
        };
        assert_eq!(
            check(&read),
            durable::CheckResult::Nonpositive(durable::KnownReadFailure::NotRequested)
        );
        read.requested = true;
        assert_eq!(
            check(&read),
            durable::CheckResult::Nonpositive(durable::KnownReadFailure::Unavailable)
        );
        read.raw = Some(serde_json::json!({"ok":true,"messages":[]}));
        assert_eq!(
            check(&read),
            durable::CheckResult::Nonpositive(durable::KnownReadFailure::Missing)
        );
        read.issue = Some(Issue::UnsupportedResponse);
        assert_eq!(
            check(&read),
            durable::CheckResult::Nonpositive(durable::KnownReadFailure::Unsupported)
        );
    }
}
