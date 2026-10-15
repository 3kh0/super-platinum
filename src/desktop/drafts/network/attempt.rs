//! Shared prepared request execution and strict acknowledgement classification.
//! Production and isolated fixtures differ only in the execution closure.
use super::*;
use std::time::Instant;

pub(crate) async fn execute_prepared<F, Fut>(
    operation: &Operation,
    workspace: &WorkspaceSession,
    request: PreparedRequest,
    now_ms: u64,
    send: F,
) -> Attempt
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let started = Instant::now();
    let result = send(request).await;
    let received_ms =
        now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    match result {
        Ok(value) => {
            if value.get("ok").and_then(Value::as_bool) == Some(false) {
                let attempt = acknowledgement::denial(value, received_ms);
                return attempt;
            }
            let (outcome, issue) = decode::response(operation, workspace, &value);
            Attempt {
                outcome,
                issue,
                dispatched: true,
                raw: Some(value),
                cooldown_until_ms: None,
            }
        }
        Err(error) => failure(&error, received_ms),
    }
}
