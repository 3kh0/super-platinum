//! One captured authenticated session and one non-retrying draft HTTP attempt.
//! The owner must persist preparation first and persist this outcome afterward.
//! No account lookup, UI mutation, enrollment or automatic replay occurs here.
use super::dispatch::Outcome;
use crate::state::ShellState;
use serde_json::Value;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Instant,
};
use super_platinum_core::{
    config::WorkspaceSession,
    drafts::sync::Operation,
    slack::{Error, PreparedRequest, SlackClient, Transport, api},
};
mod acknowledgement;
mod decode;
mod draft_review;
pub(crate) use draft_review::RemoteProof;
mod readback;
pub(crate) use readback::Readback;
mod update_readback;
pub(crate) use update_readback::decode_update;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Session,
    Offline,
    Scope,
    Unsupported,
    Cooldown { until_ms: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Issue {
    Blocked(Block),
    RateLimited,
    Rejected,
    Uncertain,
    MalformedResponse,
    UnsupportedResponse,
}
pub(crate) struct Attempt {
    pub outcome: Outcome,
    pub cooldown_until_ms: Option<u64>,
    pub issue: Option<Issue>,
    pub dispatched: bool,
    /// Full response evidence, never logged. Do not reconstruct authoritative
    /// content from display text or throw away unsupported returned fields.
    pub raw: Option<Value>,
}
impl Attempt {
    fn blocked(block: Block) -> Self {
        Self {
            outcome: if matches!(block, Block::Cooldown { .. }) {
                Outcome::Uncertain
            } else {
                Outcome::Rejected
            },
            cooldown_until_ms: match block {
                Block::Cooldown { until_ms } => Some(until_ms),
                _ => None,
            },
            issue: Some(Issue::Blocked(block)),
            dispatched: false,
            raw: None,
        }
    }
}

// Intentionally no Debug implementation: this owns authenticated credentials.
#[derive(Clone)]
pub(crate) struct Binding {
    account: String,
    epoch: u64,
    cookie: String,
    transport: Arc<Transport>,
    client: SlackClient,
    workspace: WorkspaceSession,
    cooldown_until_ms: Arc<Mutex<u64>>,
}
impl Binding {
    pub fn capture(shell: &ShellState, team: &str) -> Result<Self, Block> {
        let account = shell.core.active_account.clone().ok_or(Block::Session)?;
        if !super_platinum_core::drafts::sync::valid_account_id(&account) {
            return Err(Block::Session);
        }
        let session = shell.core.session.as_ref().ok_or(Block::Session)?;
        let workspace = session
            .workspaces
            .get(team)
            .cloned()
            .ok_or(Block::Session)?;
        if session.d_cookie.is_empty()
            || workspace.team_id != team
            || workspace.token.is_empty()
            || workspace.user_id.is_empty()
            || shell.core.workspaces.get(team).is_none_or(|state| {
                state.team_id != team || state.self_user_id != workspace.user_id
            })
        {
            return Err(Block::Session);
        }
        Ok(Self {
            account,
            epoch: shell.core.account_epoch,
            cookie: session.d_cookie.clone(),
            transport: shell.core.transport.clone().ok_or(Block::Session)?,
            client: shell.core.client.clone(),
            workspace,
            cooldown_until_ms: Arc::new(Mutex::new(0)),
        })
    }
    /// Check before preparing a new operation. Once dispatched, its immutable
    /// receipt still belongs to this account even if the visible visit changes.
    pub fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(self.account.as_str())
            && shell.core.account_epoch == self.epoch
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|transport| Arc::ptr_eq(transport, &self.transport))
            && shell
                .core
                .session
                .as_ref()
                .filter(|session| session.d_cookie == self.cookie)
                .and_then(|session| session.workspaces.get(&self.workspace.team_id))
                .is_some_and(|session| {
                    session.team_id == self.workspace.team_id
                        && session.user_id == self.workspace.user_id
                        && session.token == self.workspace.token
                        && session.url == self.workspace.url
                        && session.enterprise_id == self.workspace.enterprise_id
                })
            && shell
                .core
                .workspaces
                .get(&self.workspace.team_id)
                .is_some_and(|state| {
                    state.team_id == self.workspace.team_id
                        && state.self_user_id == self.workspace.user_id
                })
    }
    /// Merge a persisted cooldown before dispatch. The owner must store 429
    /// deadlines durably before production autosave can be enabled.
    pub fn defer_until(&self, until_ms: u64) {
        if let Ok(mut deadline) = self.cooldown_until_ms.lock() {
            *deadline = (*deadline).max(until_ms);
        }
    }
    pub fn prepare(
        &self,
        operation: &Operation,
        now_ms: u64,
    ) -> Result<api::DraftOperationRequest, Block> {
        if operation.scope.account_id != self.account
            || operation.scope.key.team != self.workspace.team_id
        {
            return Err(Block::Scope);
        }
        let deadline = *self.cooldown_until_ms.lock().map_err(|_| Block::Session)?;
        if deadline > now_ms {
            return Err(Block::Cooldown { until_ms: deadline });
        }
        api::prepare_draft_operation(&self.client, &self.workspace, operation, now_ms)
            .map_err(|_| Block::Unsupported)
    }
    pub async fn execute(&self, operation: &Operation, now_ms: u64) -> Attempt {
        self.execute_with(operation, now_ms, |request| {
            self.transport.execute_draft_mutation(request)
        })
        .await
    }
    async fn execute_with<F, Fut>(&self, operation: &Operation, now_ms: u64, send: F) -> Attempt
    where
        F: FnOnce(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let prepared = match self.prepare(operation, now_ms) {
            Ok(prepared) => prepared,
            Err(block) => return Attempt::blocked(block),
        };
        let started = Instant::now();
        let result = send(prepared.request).await;
        let received_ms =
            now_ms.saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        let attempt = match result {
            Ok(value) => {
                if value.get("ok").and_then(Value::as_bool) == Some(false) {
                    let attempt = acknowledgement::denial(value, received_ms);
                    if let Some(deadline) = attempt.cooldown_until_ms {
                        self.defer_until(deadline);
                    }
                    return attempt;
                }
                let (outcome, issue) = decode::response(operation, &self.workspace, &value);
                Attempt {
                    outcome,
                    issue,
                    dispatched: true,
                    raw: Some(value),
                    cooldown_until_ms: None,
                }
            }
            Err(error) => failure(&error, received_ms),
        };
        if let Some(deadline) = attempt.cooldown_until_ms {
            self.defer_until(deadline);
        }
        attempt
    }
}

fn failure(error: &Error, now_ms: u64) -> Attempt {
    if matches!(error, Error::Api(code) if code == "draft_mutation_not_sent_offline") {
        return Attempt::blocked(Block::Offline);
    }
    let retry = match error {
        Error::RateLimited { retry_after_secs }
        | Error::HttpStatus {
            status: 429,
            retry_after_secs,
        } => Some(retry_after_secs.unwrap_or(30).max(1)),
        Error::Api(code) if matches!(code.as_str(), "ratelimited" | "rate_limited") => Some(30),
        _ => None,
    };
    let (outcome, issue) = if retry.is_some() {
        (Outcome::Uncertain, Issue::RateLimited)
    } else if matches!(error, Error::Api(code) if definite_rejection(code)) {
        (Outcome::Rejected, Issue::Rejected)
    } else {
        (Outcome::Uncertain, Issue::Uncertain)
    };
    Attempt {
        outcome,
        issue: Some(issue),
        dispatched: true,
        raw: None,
        cooldown_until_ms: retry.map(|seconds| now_ms.saturating_add(seconds.saturating_mul(1000))),
    }
}
fn definite_rejection(code: &str) -> bool {
    matches!(
        code,
        "invalid_auth"
            | "not_authed"
            | "token_revoked"
            | "token_expired"
            | "account_inactive"
            | "channel_not_found"
            | "not_in_channel"
            | "is_archived"
            | "restricted_action"
            | "no_permission"
            | "missing_scope"
            | "invalid_arguments"
            | "invalid_arg_name"
            | "invalid_blocks"
            | "invalid_channel"
            | "invalid_thread_ts"
            | "invalid_client_msg_id"
            | "invalid_draft"
            | "msg_too_long"
            | "no_text"
            | "time_in_past"
            | "time_too_far"
            | "too_many_attachments"
            | "cannot_reply_to_message"
            | "posting_to_channel_denied"
    )
}

#[cfg(test)]
mod tests;
