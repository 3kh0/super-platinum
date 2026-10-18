//! Closed disconnected native workflow through configured Root/Host/Source.
//! The fixture supplies only the exact owned completion's synthetic raw read.
use super::super::owned_route::{o, ui};
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, OnceLock};
mod driver;
mod isolation;
mod seed;
static RUN: OnceLock<Arc<Run>> = OnceLock::new();
pub(crate) const SCENARIOS: &[&str] = &[
    "local-channel",
    "remote-channel",
    "local-thread",
    "remote-thread",
    "attachment-channel",
    "cooldown-channel",
    "edit-channel",
    "retire-channel",
    "finish-channel",
];
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-owned-baseline" | "protected-owned-baseline-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_BASELINE_CHECK").as_deref() == Ok("1")
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
pub(super) struct Run {
    account: Arc<Account>,
    fixture: Arc<o::BaselineFixture>,
    transport: Arc<super_platinum_core::slack::Transport>,
    target: ComposerTarget,
    epoch: u64,
    view: u64,
    config: isolation::Config,
    native: Mutex<Vec<Value>>,
    checkpoint: Mutex<Option<(operation::Operation, String)>>,
}
impl Run {
    fn current(&self, shell: &ShellState) -> bool {
        enabled()
            && shell.core.cache.is_none()
            && shell.core.active_account.as_deref() == Some(self.account.id())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view
            && shell.composer_key(self.target).as_ref() == Some(&self.fixture.scope().key)
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
}
pub(crate) fn current(shell: &ShellState) -> bool {
    RUN.get().is_some_and(|run| run.current(shell))
}
pub(crate) fn read(
    shell: &ShellState,
    completion: &o::BaselineCompletion,
    now: u64,
) -> Result<crate::drafts::network::BaselineRead, durable::Error> {
    let run = RUN
        .get()
        .filter(|run| run.current(shell))
        .ok_or(durable::Error::Stale)?;
    let read = run.fixture.read(completion, now).map_err(ui)?;
    let raw = match &read.response {
        durable::BaselineResponse::Positive { raw, .. } => Some(raw.clone()),
        durable::BaselineResponse::Nonpositive(_) => None,
    };
    Ok(crate::drafts::network::BaselineRead {
        response: read.response,
        raw,
        cooldown_until_ms: (read.known_until != 0).then_some(read.known_until),
        requested: true,
        error: None,
    })
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
/// Closed synthetic-composition checkpoint, causally after the actual choice
/// commits and before native projection. No receipt or outcome is fabricated.
pub(crate) async fn projection_checkpoint(
    state: Signal<ShellState>,
    operation: &operation::Operation,
    applied: &Applied,
) -> Result<(), durable::Error> {
    let Some(run) = RUN.get().filter(|r| r.config.scenario == "finish-channel") else {
        return Ok(());
    };
    if !run.current(&state.peek())
        || !operation.current()
        || applied.after.scope() != run.fixture.scope()
    {
        return Err(durable::Error::Stale);
    }
    let barrier = applied.barrier.as_ref().ok_or(durable::Error::Stale)?;
    let actual = run
        .account
        .owner(run.fixture.scope().key.clone())?
        .review()
        .await?;
    if !actual.same(&applied.after) || !run.current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    {
        let mut held = run.checkpoint.lock().map_err(|_| durable::Error::Storage)?;
        if held.is_some() {
            return Err(durable::Error::Stale);
        }
        *held = Some((operation.clone(), barrier.id().into()));
    }
    let proof = driver::native(run, "composition-start", None, false)
        .await
        .map_err(|_| durable::Error::Stale)?;
    if proof["composing"] != true {
        return Err(durable::Error::Stale);
    }
    record_native("synthetic-composition-after-actual-commit", proof);
    Ok(())
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = async {
            isolation::wait_ready().await?;
            let (run, mut worker) = seed::install(state).await?;
            // Keep the actual accepted WorkerSource alive throughout the driver.
            let result = tokio::time::timeout(std::time::Duration::from_secs(150), driver::run(state, &desktop, &run, &mut worker)).await
                .map_err(|_| "Configured baseline driver timed out".to_owned()).and_then(|v|v);
            if result.is_err() && isolation::validate(&run.config).is_ok() {
                let path=run.config.captures.join("failure.png");
                let _=crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), &desktop).await;
            }
            let close = worker.close(durable::GapReason::Disconnect).await;
            let mut output = result?;
            close.map_err(|e| format!("Source cleanup: {e:?}"))?;
            output["cleanup"]=json!({"sourceClosed":true,"connectionCurrent":socket::capture(&state.peek(),"T1").is_ok_and(|c|c.current(&state.peek()))});
            Ok::<_, String>(output)
        }
        .await;
        eprintln!(
            "protected-owned-baseline-result: {}",
            match result {
                Ok(value) => value,
                Err(error) => json!({"ok":false,"error":error}),
            }
        );
    });
}
