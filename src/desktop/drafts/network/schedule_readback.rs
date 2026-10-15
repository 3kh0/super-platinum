//! Exact positive recovery for one frozen, known-ID Schedule. No write or replay.
//! B1681ecc... Id6n6810246 freezes the draft/time and updates an existing ID;
//! B6818245 -> V8904e661... zZt+9756496 -> Sf085df09... uGgu1594178 proves
//! drafts.info with draft_id only. Retained provenance is documented in
//! docs/draft-observation-integration.md (managed Schedule source evidence).
use super::*;
use super_platinum_core::{drafts::sync::OperationKind, state::cmp_ts};

impl Binding {
    pub async fn check_schedule(&self, operation: &Operation, now_ms: u64) -> Readback {
        ScheduleRead {
            account: &self.account,
            client: &self.client,
            workspace: &self.workspace,
            cooldown: &self.cooldown_until_ms,
        }
        .check(operation, now_ms, |request| self.transport.execute(request))
        .await
    }
}

// The native fixture substitutes only HTTP, reusing preparation/clock/decoder.
pub(crate) async fn check_schedule_with<F, Fut>(
    account: &str,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    cooldown: &Mutex<u64>,
    operation: &Operation,
    now_ms: u64,
    read: F,
) -> Readback
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    ScheduleRead {
        account,
        client,
        workspace,
        cooldown,
    }
    .check(operation, now_ms, read)
    .await
}

// Only execution is injected; tests do not create Session, Transport or storage.
struct ScheduleRead<'a> {
    account: &'a str,
    client: &'a SlackClient,
    workspace: &'a WorkspaceSession,
    cooldown: &'a Mutex<u64>,
}
impl ScheduleRead<'_> {
    fn prepare(&self, operation: &Operation, now_ms: u64) -> Result<PreparedRequest, Block> {
        if operation.scope.account_id != self.account
            || operation.scope.key.team != self.workspace.team_id
        {
            return Err(Block::Scope);
        }
        let until_ms = *self.cooldown.lock().map_err(|_| Block::Session)?;
        if now_ms < until_ms {
            return Err(Block::Cooldown { until_ms });
        }
        if !operation.known_schedule_check() || self.workspace.user_id.is_empty() {
            return Err(Block::Unsupported);
        }
        // This is a historical frozen operation read. Do not prepare a mutation
        // or require its requested delivery time to remain in the future.
        api::drafts_info(
            self.client,
            self.workspace,
            &operation.base.as_ref().unwrap().id,
        )
        .map_err(|_| Block::Unsupported)
    }

    async fn check<F, Fut>(&self, operation: &Operation, now_ms: u64, read: F) -> Readback
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let request = match self.prepare(operation, now_ms) {
            Ok(request) => request,
            Err(block) => return Readback::blocked(block),
        };
        let started = Instant::now();
        let result = read(request).await;
        let received_ms =
            now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        let mut result = match result {
            Ok(raw) => {
                let mut result = decode_schedule(operation, self.workspace, raw.clone());
                if raw.get("ok") == Some(&Value::Bool(false))
                    && let Some(code) = raw.get("error").and_then(Value::as_str)
                {
                    result.cooldown_until_ms =
                        failure(&Error::Api(code.into()), received_ms).cooldown_until_ms;
                }
                result
            }
            Err(error) => Readback {
                outcome: None,
                raw: None,
                issue: Some(Issue::Uncertain),
                cooldown_until_ms: failure(&error, received_ms).cooldown_until_ms,
                requested: true,
                has_more: false,
            },
        };
        if let Some(until) = result.cooldown_until_ms {
            if let Ok(mut deadline) = self.cooldown.lock() {
                *deadline = (*deadline).max(until);
                result.cooldown_until_ms = Some(*deadline);
            }
            result.issue = Some(Issue::RateLimited);
        }
        result
    }
}

pub(crate) fn decode_schedule(
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
    if !operation.known_schedule_check() || operation.scope.key.team != workspace.team_id {
        return result;
    }
    let base = operation.base.as_ref().unwrap();
    if api::decode_drafts_info(&base.id, raw.clone()).is_err() {
        return result;
    }
    // Existing decoder retains its conservative envelope/lifecycle/routing
    // policy, including optional is_from_composer semantics. Do not loosen it.
    let (outcome, _) = decode::response(operation, workspace, &raw);
    let Outcome::Scheduled { draft, post_at } = outcome else {
        return result;
    };
    if operation.kind != (OperationKind::Schedule { post_at })
        || draft.id != base.id
        || draft.client_msg_id != operation.client_msg_id
        || draft.scope != operation.scope
        || operation.snapshot.as_ref() != Some(&draft.snapshot)
        || !cmp_ts(Some(&draft.last_updated_ts), Some(&base.last_updated_ts)).is_gt()
    {
        return result;
    }
    result.outcome = Some(Outcome::Scheduled { draft, post_at });
    result.issue = None;
    result
}

#[cfg(test)]
mod tests;
