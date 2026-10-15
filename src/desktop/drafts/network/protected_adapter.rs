//! Proposed network child: raw baseline read for an opaque claimed admission.
//! No LocalReview/Stored conversion and no proof constructor lives here.
use super::*;
use super_platinum_core::drafts::store::protected as durable;

#[derive(Clone)]
pub(crate) struct BaselineRead {
    pub response: durable::BaselineResponse,
    pub raw: Option<Value>,
    pub cooldown_until_ms: Option<u64>,
    pub requested: bool,
}
impl BaselineRead {
    pub(crate) fn not_requested(until: Option<u64>) -> Self {
        Self {
            response: durable::BaselineResponse::Nonpositive(
                durable::KnownReadFailure::NotRequested,
            ),
            raw: None,
            cooldown_until_ms: until,
            requested: false,
        }
    }
}
impl Binding {
    pub(crate) async fn read_protected_baseline<F: Fn() -> bool>(
        &self,
        request: durable::BaselineRequest,
        now_ms: u64,
        current: F,
    ) -> BaselineRead {
        if request.scope().account_id != self.account
            || request.scope().key.team != self.workspace.team_id
        {
            return BaselineRead::not_requested(None);
        }
        let until = match self.cooldown_until_ms.lock() {
            Ok(until) => *until,
            Err(_) => return BaselineRead::not_requested(None),
        };
        if until > now_ms {
            return BaselineRead::not_requested(Some(until));
        }
        let prepared = match api::drafts_info(&self.client, &self.workspace, request.draft_id()) {
            Ok(prepared) => prepared,
            Err(_) => return BaselineRead::not_requested(None),
        };
        let start = Instant::now();
        let evidence = super::protected_execution::Evidence::new(now_ms);
        let response = self
            .transport
            .execute_protected_draft_read(prepared, current)
            .await
            .map_err(|error| evidence.failure(error));
        let received =
            now_ms.saturating_add(start.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        match response {
            Ok(raw) => {
                // This variant carries a candidate raw response, not proof of
                // positive evidence. Core classifies it against the original
                // full review and durably retains unsupported/malformed raw
                // input as Nonpositive without granting a verification basis.
                let response = durable::BaselineResponse::Positive {
                    raw: raw.clone(),
                    user: self.workspace.user_id.clone(),
                };
                BaselineRead {
                    response,
                    raw: Some(raw),
                    cooldown_until_ms: None,
                    requested: true,
                }
            }
            Err(error) => {
                let mut attempt = failure(&error, received);
                if let Some(until) = evidence.deadline() {
                    attempt.cooldown_until_ms =
                        Some(attempt.cooldown_until_ms.unwrap_or(0).max(until));
                }
                if let Some(until) = attempt.cooldown_until_ms {
                    self.defer_until(until);
                }
                BaselineRead {
                    response: durable::BaselineResponse::Nonpositive(if evidence.not_requested() {
                        durable::KnownReadFailure::NotRequested
                    } else if attempt.cooldown_until_ms.is_some() {
                        durable::KnownReadFailure::RateLimited
                    } else {
                        durable::KnownReadFailure::Unavailable
                    }),
                    raw: None,
                    cooldown_until_ms: attempt.cooldown_until_ms,
                    requested: !evidence.not_requested(),
                }
            }
        }
    }
}
