//! One discovered catalog/owner generation and at most one prepared command.
//! The host retains this context and revokes it on editor/native/visit, session,
//! transport, account or catalog replacement. Revoke never grants replay.
use super::*;
use crate::slash_commands::{
    Owner,
    catalog::{Catalog, Choice},
    controller::{DiscoveryFailure, ProductionContext},
};
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};

pub enum SlashDiscoveryError {
    Stale,
    Unsupported,
    Transport(crate::slack::Error),
}
impl From<DiscoveryFailure> for SlashDiscoveryError {
    fn from(error: DiscoveryFailure) -> Self {
        match error {
            DiscoveryFailure::Transport(error) => Self::Transport(error),
            DiscoveryFailure::Admission(crate::slash_commands::Failure::Unsupported) => {
                Self::Unsupported
            }
            _ => Self::Stale,
        }
    }
}
impl std::fmt::Debug for SlashDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stale => "SlashDiscoveryError::Stale",
            Self::Unsupported => "SlashDiscoveryError::Unsupported",
            Self::Transport(_) => "SlashDiscoveryError::Transport(..)",
        })
    }
}
pub struct SlashContext {
    queue: Arc<Queue>,
    stream: p::Stream,
    context: ProductionContext,
    prepared: Mutex<Option<SlashInput>>,
}
impl Operations {
    /// Read-only actual discovery; no catalog/raw response injection API.
    /// The caller must recheck its captured desktop Binding after this await.
    pub async fn discover_slash(
        &self,
        stream: p::Stream,
        scope: crate::drafts::sync::Scope,
        session: WorkspaceSession,
        client: SlackClient,
        transport: Arc<Transport>,
        emoji: Arc<serde_json::Value>,
    ) -> Result<SlashContext, SlashDiscoveryError> {
        if scope.account_id != self.account()
            || stream.account() != self.account()
            || stream.team() != scope.key.team
            || session.team_id != scope.key.team
            || !stream.is_live()
        {
            return Err(SlashDiscoveryError::Stale);
        }
        let owner = Owner {
            account: scope.account_id,
            team: scope.key.team,
            channel: scope.key.channel,
            thread: scope.key.thread,
            user: session.user_id.clone(),
        };
        let context = ProductionContext::discover(owner, session, client, transport, emoji)
            .await
            .map_err(SlashDiscoveryError::from)?;
        if !stream.is_live() {
            return Err(SlashDiscoveryError::Stale);
        }
        Ok(SlashContext {
            queue: self.queue.clone(),
            stream,
            context,
            prepared: Mutex::new(None),
        })
    }
}

#[cfg(test)]
#[path = "owned_slash_context_tests.rs"]
mod tests;
impl SlashContext {
    pub fn catalog(&self) -> &Catalog {
        self.context.catalog()
    }
    pub fn is_live(&self) -> bool {
        self.context.is_live() && self.stream.is_live()
    }
    pub fn revoke(&self) {
        self.context.revoke();
    }
    pub fn prepare(
        &self,
        ops: &Operations,
        review: p::Review,
        writer: p::Writer,
        serial: u64,
        choice: &Choice,
        client_token: &str,
    ) -> Result<SlashInput, AdmissionError> {
        let owner = self.catalog().owner();
        let scope = review.scope();
        if !Arc::ptr_eq(&self.queue, &ops.queue)
            || !self.is_live()
            || serial == 0
            || scope.account_id != owner.account
            || scope.key.team != owner.team
            || scope.key.channel != owner.channel
            || scope.key.thread != owner.thread
            || writer.scope() != scope
            || writer.enrollment() != review.row().lineage().enrollment()
        {
            return Err(AdmissionError::Stale);
        }
        let mut prepared = self.prepared.lock().map_err(|_| AdmissionError::Dead)?;
        if prepared.is_some() {
            return Err(AdmissionError::Stale);
        }
        let plan = self
            .context
            .plan(&review, choice, client_token)
            .map_err(|e| match e {
                crate::slash_commands::Failure::Unsupported => AdmissionError::Unsupported,
                _ => AdmissionError::Stale,
            })?;
        let input =
            SlashInput::from_prepared(self.stream.clone(), review, writer, serial, Arc::new(plan));
        *prepared = Some(input.clone());
        Ok(input)
    }
    /// Same exact prepared holder; existing worker consumes its Permit once.
    /// Retain the complete future even if the visible chooser disappears.
    pub async fn execute(&self, completion: &SlashCompletion) -> Result<(), SlashExecuteError> {
        let same = Arc::ptr_eq(&self.queue, &completion.queue)
            && self
                .prepared
                .lock()
                .map_err(|_| SlashExecuteError::BeforeDispatch(AdmissionError::Dead))?
                .as_ref()
                .is_some_and(|input| Arc::ptr_eq(&input.inner, &completion.held.input()));
        if !same {
            return Err(SlashExecuteError::BeforeDispatch(AdmissionError::Stale));
        }
        completion.execute().await
    }
}
