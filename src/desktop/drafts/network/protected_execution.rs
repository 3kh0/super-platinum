//! Protected-only bounded HTTP execution. Ordinary Binding methods are unchanged.
//! Existing preparation/decoders classify the same complete Value; the typed
//! transport disposition additionally distinguishes a never-dispatched call.
use super::*;
use std::cell::Cell;
use super_platinum_core::{drafts::sync::OperationKind, slack::transport::ProtectedDraftFailure};

pub(super) struct Evidence {
    started: Instant,
    now: u64,
    not_requested: Cell<bool>,
    deadline: Cell<Option<u64>>,
}
impl Evidence {
    pub fn new(now: u64) -> Self {
        Self {
            started: Instant::now(),
            now,
            not_requested: Cell::new(false),
            deadline: Cell::new(None),
        }
    }
    pub fn failure(&self, failure: ProtectedDraftFailure) -> Error {
        self.not_requested.set(!failure.requested());
        let seconds = match failure.error() {
            Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(30).max(1)),
            Error::HttpStatus {
                status,
                retry_after_secs,
            } => retry_after_secs
                .or((*status == 429).then_some(30))
                .map(|s| s.max(1)),
            _ => None,
        };
        if let Some(seconds) = seconds {
            let received = self.now.saturating_add(
                self.started
                    .elapsed()
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX),
            );
            self.deadline
                .set(Some(received.saturating_add(seconds.saturating_mul(1000))));
        }
        failure.into_error()
    }
    pub fn not_requested(&self) -> bool {
        self.not_requested.get()
    }
    pub fn deadline(&self) -> Option<u64> {
        self.deadline.get()
    }
}
impl Binding {
    pub(crate) async fn execute_protected<F: Fn() -> bool>(
        &self,
        operation: &Operation,
        now: u64,
        current: F,
    ) -> Attempt {
        self.execute_protected_with(operation, now, |request| {
            self.transport
                .execute_protected_draft_mutation(request, &current)
        })
        .await
    }
    // Consumes the actual issued claim; no Operation/receipt constructor can
    // enter this adapter. The old Owner path remains unchanged.
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(crate) async fn execute_claimed<F: Fn() -> bool>(
        &self,
        claim: super_platinum_core::drafts::store::protected::ClaimedDispatch,
        now: u64,
        current: F,
    ) -> Attempt {
        self.execute_claimed_with(claim, now, |request| {
            self.transport
                .execute_protected_draft_mutation(request, &current)
        })
        .await
    }
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(super) async fn execute_claimed_with<F, Fut>(
        &self,
        claim: super_platinum_core::drafts::store::protected::ClaimedDispatch,
        now: u64,
        send: F,
    ) -> Attempt
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, ProtectedDraftFailure>>,
    {
        self.execute_protected_with(claim.operation(), now, send)
            .await
    }
    async fn execute_protected_with<F, Fut>(
        &self,
        operation: &Operation,
        now: u64,
        send: F,
    ) -> Attempt
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, ProtectedDraftFailure>>,
    {
        let evidence = Evidence::new(now);
        let evidence_ref = &evidence;
        let mut result = self
            .execute_with(operation, now, |request| async move {
                send(request)
                    .await
                    .map_err(|error| evidence_ref.failure(error))
            })
            .await;
        if evidence.not_requested() {
            result.dispatched = false;
            result.outcome = Outcome::Uncertain;
            result.issue = Some(Issue::Blocked(Block::Session));
        }
        if let Some(deadline) = evidence.deadline() {
            self.defer_until(deadline);
            result.cooldown_until_ms = Some(result.cooldown_until_ms.unwrap_or(0).max(deadline));
        }
        result
    }
    pub(crate) async fn check_protected_operation<F: Fn() -> bool>(
        &self,
        operation: &Operation,
        now: u64,
        current: F,
    ) -> Readback {
        let evidence = Evidence::new(now);
        let evidence_ref = &evidence;
        let current_ref = &current;
        let read = |request| async move {
            self.transport
                .execute_protected_draft_read(request, current_ref)
                .await
                .map_err(|error| evidence_ref.failure(error))
        };
        let mut result = match operation.kind {
            OperationKind::Send => self.check_send_with(operation, now, read).await,
            OperationKind::Update => self.check_update_with(operation, now, read).await,
            OperationKind::Schedule { .. } => {
                schedule_readback::check_schedule_with(
                    &self.account,
                    &self.client,
                    &self.workspace,
                    &self.cooldown_until_ms,
                    operation,
                    now,
                    read,
                )
                .await
            }
            _ => Readback::blocked(Block::Unsupported),
        };
        if evidence.not_requested() {
            result.requested = false;
            result.outcome = None;
            result.issue = Some(Issue::Blocked(Block::Session));
        }
        if let Some(deadline) = evidence.deadline() {
            self.defer_until(deadline);
            result.cooldown_until_ms = Some(result.cooldown_until_ms.unwrap_or(0).max(deadline));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_retry_after_on_unavailable_response_is_retained() {
        let evidence = Evidence::new(10_000);
        evidence.failure(ProtectedDraftFailure::Response(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(7),
        }));
        assert!(!evidence.not_requested());
        assert!(evidence.deadline().unwrap() >= 17_000);
    }
    #[test]
    fn retired_before_any_dispatch_is_distinct_from_a_lost_response() {
        let evidence = Evidence::new(10);
        evidence.failure(ProtectedDraftFailure::NotRequested(Error::Transport(
            "retired".into(),
        )));
        assert!(evidence.not_requested());
        assert_eq!(evidence.deadline(), None);
    }
}
