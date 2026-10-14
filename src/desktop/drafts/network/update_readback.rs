//! Positive evidence for one frozen uncertain Update, never a conflict choice.
use super::*;
use super_platinum_core::{
    drafts::sync::{DraftSync, OperationKind},
    state::cmp_ts,
};

impl Binding {
    pub async fn check_update(&self, operation: &Operation, now_ms: u64) -> Readback {
        self.check_update_with(operation, now_ms, |request| self.transport.execute(request))
            .await
    }
    pub(super) async fn check_update_with<F, Fut>(
        &self,
        operation: &Operation,
        now_ms: u64,
        read: F,
    ) -> Readback
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let request = match self.prepare_update_read(operation, now_ms) {
            Ok(request) => request,
            Err(block) => return Readback::blocked(block),
        };
        let started = Instant::now();
        let result = read(request).await;
        let received_ms =
            now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        match result {
            Ok(raw) => {
                let mut result = decode_update(operation, &self.workspace, raw.clone());
                if raw.get("ok") == Some(&Value::Bool(false))
                    && let Some(code) = raw.get("error").and_then(Value::as_str)
                {
                    let retry = failure(&Error::Api(code.into()), received_ms).cooldown_until_ms;
                    if let Some(until) = retry {
                        self.defer_until(until);
                        result.cooldown_until_ms = Some(until);
                        result.issue = Some(Issue::RateLimited);
                    }
                }
                result
            }
            Err(error) => {
                let failure = failure(&error, received_ms);
                if let Some(until) = failure.cooldown_until_ms {
                    self.defer_until(until);
                }
                Readback {
                    outcome: None,
                    raw: None,
                    issue: Some(if failure.cooldown_until_ms.is_some() {
                        Issue::RateLimited
                    } else {
                        Issue::Uncertain
                    }),
                    cooldown_until_ms: failure.cooldown_until_ms,
                    requested: true,
                    has_more: false,
                }
            }
        }
    }
    fn prepare_update_read(
        &self,
        operation: &Operation,
        now_ms: u64,
    ) -> Result<PreparedRequest, Block> {
        if operation.kind != OperationKind::Update
            || operation.snapshot.is_none()
            || operation.consume_id.is_some()
        {
            return Err(Block::Unsupported);
        }
        // Preparation validates scope/cooldown and the immutable operation. Its
        // mutation request is never dispatched by this read-only path.
        self.prepare(operation, now_ms)?;
        let base = operation.base.as_ref().ok_or(Block::Unsupported)?;
        if base.scope != operation.scope || base.client_msg_id != operation.client_msg_id {
            return Err(Block::Unsupported);
        }
        api::drafts_info(&self.client, &self.workspace, &base.id).map_err(|_| Block::Unsupported)
    }
}
pub(crate) fn decode_update(
    operation: &Operation,
    workspace: &WorkspaceSession,
    raw: Value,
) -> Readback {
    let mut result = Readback {
        outcome: None,
        raw: Some(raw.clone()),
        issue: Some(Issue::UnsupportedResponse),
        cooldown_until_ms: None,
        requested: true,
        has_more: false,
    };
    let Some(base) = operation.base.as_ref() else {
        return result;
    };
    if operation.kind != OperationKind::Update
        || operation.scope.key.team != workspace.team_id
        || operation.consume_id.is_some()
        || base.scope != operation.scope
        || base.client_msg_id != operation.client_msg_id
        || api::decode_drafts_info(&base.id, raw.clone()).is_err()
    {
        return result;
    }
    let (outcome, issue) = decode::response(operation, workspace, &raw);
    let Outcome::Draft(remote) = outcome else {
        result.issue = issue.or(Some(Issue::Uncertain));
        return result;
    };
    if remote.id != base.id
        || remote.client_msg_id != operation.client_msg_id
        || remote.scope != operation.scope
        || operation.snapshot.as_ref() != Some(&remote.snapshot)
        || !cmp_ts(Some(&remote.last_updated_ts), Some(&base.last_updated_ts)).is_gt()
        || DraftSync::adopt_remote(&operation.scope, remote.clone()).is_err()
    {
        return result;
    }
    result.outcome = Some(Outcome::Draft(remote));
    result.issue = None;
    result
}
#[cfg(test)]
mod tests;
