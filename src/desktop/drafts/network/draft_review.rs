//! Explicit positive conflict readback. No storage, projection, mutation or retry.
//! Called only by an explicit review action; no automatic request scheduling.
use super::*;
use std::{collections::BTreeMap, sync::OnceLock};
use super_platinum_core::drafts::{
    store::sync::{LocalReview, RemoteReview},
    sync::Workflow,
};

// No credentials in keys. A reopened controller cannot bypass Retry-After.
// Process restart deliberately does not claim a persisted cooldown guarantee.
type Key = (String, String, String);
fn deadlines() -> &'static Mutex<BTreeMap<Key, u64>> {
    static VALUE: OnceLock<Mutex<BTreeMap<Key, u64>>> = OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}

/// This proof retains the authentication that produced the response. The caller
/// must also retain the exact Editor/serial/native visit captured before reading.
/// No Debug implementation: the Binding owns credentials.
#[derive(Clone)]
pub(crate) struct RemoteProof {
    binding: Binding,
    review: RemoteReview,
}
impl RemoteProof {
    pub fn review(&self) -> &RemoteReview {
        &self.review
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
    }
    pub fn into_current_review(self, shell: &ShellState) -> Result<RemoteReview, Block> {
        self.current(shell)
            .then_some(self.review)
            .ok_or(Block::Session)
    }
}
pub(crate) struct RemoteRead {
    pub proof: Option<RemoteProof>,
    pub issue: Option<Issue>,
    pub cooldown_until_ms: Option<u64>,
    pub requested: bool,
    /// Opaque unsuccessful evidence only; never exposed as a successful choice.
    pub raw: Option<Value>,
}
impl RemoteRead {
    fn blocked(block: Block) -> Self {
        Self {
            proof: None,
            issue: Some(Issue::Blocked(block)),
            cooldown_until_ms: match block {
                Block::Cooldown { until_ms } => Some(until_ms),
                _ => None,
            },
            requested: false,
            raw: None,
        }
    }
}
impl Binding {
    fn review_key(&self) -> Key {
        (
            self.account.clone(),
            self.workspace.team_id.clone(),
            self.workspace.user_id.clone(),
        )
    }
    fn prepare_remote_read(
        &self,
        review: &LocalReview,
        now_ms: u64,
    ) -> Result<PreparedRequest, Block> {
        if review.scope.account_id != self.account
            || review.scope.key.team != self.workspace.team_id
        {
            return Err(Block::Scope);
        }
        let row = review.stored.as_ref().ok_or(Block::Unsupported)?;
        if row.state.scope() != &review.scope
            || row.state.pending().is_some()
            || row.state.consumption().is_some()
            || row.state.blocked_observation().is_some()
            || !matches!(
                row.state.workflow(),
                Workflow::Idle | Workflow::Conflict { pending: None, .. }
            )
        {
            return Err(Block::Unsupported);
        }
        let base = row.state.acknowledged().ok_or(Block::Unsupported)?;
        let until_ms = {
            let mut deadlines = deadlines().lock().map_err(|_| Block::Session)?;
            deadlines.retain(|_, until| *until > now_ms);
            deadlines.get(&self.review_key()).copied().unwrap_or(0)
        }
        .max(*self.cooldown_until_ms.lock().map_err(|_| Block::Session)?);
        if until_ms > now_ms {
            return Err(Block::Cooldown { until_ms });
        }
        api::drafts_info(&self.client, &self.workspace, &base.id).map_err(|_| Block::Unsupported)
    }
    /// Caller checks current() immediately before this call and after its await.
    /// The exact inventory was captured BEFORE this request. One attempt only.
    pub async fn read_remote_review(&self, review: LocalReview, now_ms: u64) -> RemoteRead {
        self.read_remote_review_with(review, now_ms, |request| self.transport.execute(request))
            .await
    }
    async fn read_remote_review_with<F, Fut>(
        &self,
        review: LocalReview,
        now_ms: u64,
        read: F,
    ) -> RemoteRead
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let request = match self.prepare_remote_read(&review, now_ms) {
            Ok(request) => request,
            Err(block) => return RemoteRead::blocked(block),
        };
        let id = review
            .stored
            .as_ref()
            .unwrap()
            .state
            .acknowledged()
            .unwrap()
            .id
            .clone();
        let started = Instant::now();
        let result = read(request).await;
        let received_ms =
            now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        let (raw, error) = match result {
            Ok(raw)
                if raw.get("ok").and_then(Value::as_bool) == Some(false)
                    || raw.get("error").is_some_and(|value| !value.is_null()) =>
            {
                let error = Error::Api(
                    raw.get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("invalid_draft_info")
                        .into(),
                );
                (Some(raw), Some(error))
            }
            Ok(raw) => {
                let valid = api::decode_drafts_info(&id, raw.clone()).is_ok();
                let prepared = valid
                    .then(|| RemoteReview::prepare(review, raw.clone(), &self.workspace.user_id))
                    .transpose();
                return match prepared {
                    Ok(Some(review)) => RemoteRead {
                        proof: Some(RemoteProof {
                            binding: self.clone(),
                            review,
                        }),
                        issue: None,
                        cooldown_until_ms: None,
                        requested: true,
                        raw: None,
                    },
                    _ => RemoteRead {
                        proof: None,
                        issue: Some(Issue::UnsupportedResponse),
                        cooldown_until_ms: None,
                        requested: true,
                        raw: Some(raw),
                    },
                };
            }
            Err(error) => (None, Some(error)),
        };
        let attempt = failure(&error.unwrap(), received_ms);
        if let Some(until) = attempt.cooldown_until_ms {
            self.defer_until(until);
            if let Ok(mut deadlines) = deadlines().lock() {
                let saved = deadlines.entry(self.review_key()).or_default();
                *saved = (*saved).max(until);
            }
        }
        RemoteRead {
            proof: None,
            issue: Some(if attempt.cooldown_until_ms.is_some() {
                Issue::RateLimited
            } else {
                Issue::Uncertain
            }),
            cooldown_until_ms: attempt.cooldown_until_ms,
            requested: true,
            raw,
        }
    }
}
#[cfg(test)]
mod tests;
