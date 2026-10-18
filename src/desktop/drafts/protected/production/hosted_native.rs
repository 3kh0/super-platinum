//! Closed disconnected composer workflow through the actual Root/Editor/Send.
use super::super::owned_route::{o, ui};
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, OnceLock};
mod driver;
mod isolation;
mod response_gate;
mod seed;
static RUN: OnceLock<Arc<Run>> = OnceLock::new();
pub(crate) const SCENARIOS: &[&str] = &[
    "caption",
    "file-only",
    "unknown",
    "storage",
    "changed-current",
];
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-hosted-composer" | "protected-hosted-composer-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_HOSTED_CHECK").as_deref() == Ok("1")
}
pub(crate) fn apply(shell: &mut ShellState) {
    if let Some(index) = shell.channels.iter().position(|c| c.id == "C1") {
        shell.select_channel(index, crate::state::ChannelOpen::Global);
    }
    shell.core.active_account = None;
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.cache = None;
    shell.core.drafts = Default::default();
    shell.core.composer = Default::default();
    shell.core.thread_composer = Default::default();
    shell.core.composer_attachments.clear();
    shell.core.thread_composer_attachments.clear();
    shell.thread_root = None;
    shell.sync_composer_drafts();
}
pub(crate) struct Run {
    account: Arc<Account>,
    fixture: Arc<o::HostedFixture>,
    transport: Arc<super_platinum_core::slack::Transport>,
    epoch: u64,
    view: u64,
    config: isolation::Config,
    native: Mutex<Vec<Value>>,
    response: response_gate::Gate,
}
impl Run {
    fn current(&self, shell: &ShellState) -> bool {
        enabled()
            && shell.core.cache.is_none()
            && shell.core.active_account.as_deref() == Some(self.account.id())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view
            && shell.composer_key(ComposerTarget::Channel).as_ref()
                == Some(&self.fixture.scope().key)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && shell
                .core
                .session
                .as_ref()
                .is_some_and(seed::synthetic_session)
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U1")
            && account(self.account.id()).is_ok_and(|a| Arc::ptr_eq(&a, &self.account))
    }
    pub(crate) fn scope(&self) -> &super_platinum_core::drafts::sync::Scope {
        self.fixture.scope()
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(crate) fn session(&self) -> super_platinum_core::config::WorkspaceSession {
        self.fixture.session()
    }
    pub(crate) fn read(
        &self,
        request: super_platinum_core::slack::PreparedRequest,
    ) -> Result<Value, super_platinum_core::slack::Error> {
        self.fixture.read(request)
    }
    pub(crate) async fn complete(
        &self,
        attempt: o::HostedAttempt,
    ) -> Result<(), o::HostedExecuteError> {
        let fail = || o::HostedExecuteError::BeforeDispatch(o::AdmissionError::Stale);
        // Independently bounded. Failure binds no response; the actual armed
        // claim stays uncertain and the original Send task can unwind.
        self.response.wait().await.map_err(|_| fail())?;
        if self.config.scenario == "storage" {
            let route = self.account.owned.as_ref().ok_or_else(fail)?;
            let fault = route
                .operations
                .fixture_fault(
                    self.fixture.scope().clone(),
                    durable::test_support::FaultPoint::MutationReceipt,
                )
                .map_err(|_| fail())?;
            fault.submit().map_err(|_| fail())?;
            tokio::task::spawn_blocking(move || fault.wait())
                .await
                .map_err(|_| fail())?
                .map_err(|_| fail())?;
        }
        self.fixture.complete(attempt)
    }
}
pub(crate) fn capture(shell: &ShellState) -> Option<Arc<Run>> {
    RUN.get().filter(|run| run.current(shell)).cloned()
}
pub(crate) fn current(shell: &ShellState) -> bool {
    capture(shell).is_some()
}
pub(crate) fn binding(
    account: &str,
    session: &super_platinum_core::config::WorkspaceSession,
    transport: &Arc<super_platinum_core::slack::Transport>,
) -> Result<Option<Arc<Run>>, o::HostedExecuteError> {
    if !named() {
        return Ok(None);
    }
    let run = RUN
        .get()
        .filter(|run| {
            enabled()
                && run.account.id() == account
                && Arc::ptr_eq(transport, &run.transport)
                && session.team_id == "T1"
                && session.user_id == "U1"
                && session.token == run.fixture.session().token
                && session.url == "https://fixture.slack.com"
                && session.enterprise_id.is_none()
        })
        .ok_or(o::HostedExecuteError::BeforeDispatch(
            o::AdmissionError::Stale,
        ))?;
    Ok(Some(run.clone()))
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if let Some(run) = RUN.get() {
        if let Ok(mut records) = run.native.lock() {
            if records.len() < 128 {
                records.push(json!({"kind":kind,"evidence":evidence}));
            }
        }
    }
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result=async {
            isolation::wait_ready().await?;
            let(run,mut worker)=seed::install(state).await?;
            let result=tokio::time::timeout(std::time::Duration::from_secs(150),driver::run(state,&desktop,&run,&mut worker)).await.map_err(|_|"Hosted composer driver timed out".to_owned()).and_then(|v|v);
            // RUN is static: always drop a held Sender on driver error/timeout.
            run.response.cancel();
            let close=worker.close(durable::GapReason::Disconnect).await;
            let mut output=result?;close.map_err(|e|format!("Source cleanup: {e:?}"))?;
            output["cleanup"]=json!({"sourceClosed":true,"connectionCurrent":socket::capture(&state.peek(),"T1").is_ok_and(|c|c.current(&state.peek()))});Ok::<_,String>(output)
        }.await;
        eprintln!(
            "protected-hosted-composer-result: {}",
            match result {
                Ok(value) => value,
                Err(error) => json!({"ok":false,"error":error}),
            }
        );
    });
}
