//! Closed named Slash fixture. Ordinary boot and all live transports are excluded.
//! The native driver must separately exercise Clear/edit/revocation before acceptance.
use super::owned_route::o;
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    rc::Rc,
    sync::{Mutex, OnceLock},
};
mod isolation;
mod seed;
static RUN: OnceLock<Arc<Run>> = OnceLock::new();
pub(crate) const INITIAL: &str = "/example mounted \u{65e5}\u{672c}\u{8a9e} \u{1f680}";
pub(crate) const SCENARIOS: &[&str] = &[
    "clear-channel",
    "edit-channel",
    "revoke-channel",
    "clear-thread",
    "edit-thread",
    "revoke-thread",
];
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-owned-slash" | "protected-owned-slash-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SLASH_CHECK").as_deref() == Ok("1")
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
    pub account: Arc<Account>,
    pub registry: Arc<registry::Registry>,
    pub editor: Arc<editor::Editor>,
    pub fixture: Arc<o::SlashFixture>,
    pub stream: durable::Stream,
    pub target: ComposerTarget,
    pub epoch: u64,
    pub view_id: u64,
    pub scenario: String,
    pub nonce: String,
    pub root: std::path::PathBuf,
    pub captures: std::path::PathBuf,
    pub parent: Mutex<Option<operation::Operation>>,
    pub native: Mutex<Vec<Value>>,
    pub actions: Mutex<Vec<Value>>,
}
impl Run {
    pub fn current(&self, shell: &ShellState) -> bool {
        enabled()
            && shell.core.session.is_none()
            && shell.core.cache.is_none()
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U1")
            && shell.core.active_account.as_deref() == Some(self.account.id())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view_id
            && shell.composer_key(self.target).as_ref() == Some(&self.fixture.scope().key)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.fixture.transport()))
            && self.registry.current(
                self.account.id(),
                self.epoch,
                &self.fixture.scope().key,
                &self.editor,
            )
    }
    pub async fn wait_dispatched(&self) -> Result<(), String> {
        self.fixture
            .wait_dispatched()
            .await
            .map_err(|e| format!("{e:?}"))
    }
    pub fn release_result(&self) -> Result<(), String> {
        self.fixture.release_result().map_err(|e| format!("{e:?}"))
    }
}
pub(crate) fn binding(shell: &ShellState) -> Option<Arc<Run>> {
    RUN.get().filter(|r| r.current(shell)).cloned()
}
pub(crate) fn start(state: Signal<ShellState>) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = seed::install(state).await;
        match result {
            Ok(run) => eprintln!(
                "protected-owned-slash-ready: {}",
                json!({"ok":true,"nonce":run.nonce,"scenario":run.scenario,"account":run.account.id(),"pid":std::process::id(),"executionStarted":false})
            ),
            Err(error) => eprintln!(
                "protected-owned-slash-ready: {}",
                json!({"ok":false,"error":error})
            ),
        }
    });
}
pub(crate) fn route(shell: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(run) = binding(shell) else {
        return;
    };
    for (key, local) in changes {
        if key == run.fixture.scope().key {
            let result = run.editor.submit(&local);
            run.actions
                .lock()
                .unwrap()
                .push(json!({"kind":"input","serial":result.ok(),"text":local.composer.text}));
        }
    }
}
pub(crate) fn intercept(mut state: Signal<ShellState>, target: ComposerTarget) -> bool {
    if !named() {
        return false;
    }
    let Some(run) = binding(&state.peek()) else {
        return true;
    };
    if target != run.target || run.parent.lock().unwrap().is_some() {
        return true;
    }
    let result = (|| {
        let root = dioxus::core::try_consume_context::<
            Rc<crate::view::composer::protected_operations::Root>,
        >()
        .ok_or(durable::Error::Stale)?;
        let choice = run
            .fixture
            .catalog()
            .choice(0)
            .ok_or(durable::Error::Unsupported)?;
        let action = crate::app_modal::register_viewed_action(
            &mut state.write(),
            &choice.command().app,
            Some(run.fixture.scope().key.channel.clone()),
        )
        .ok_or(durable::Error::Stale)?;
        let request = crate::view::composer::protected_actions::StartSlash {
            state,
            target,
            registry: run.registry.clone(),
            editor: run.editor.clone(),
            visit: Visit::new(),
            context: owned_slash::Context::fixture(run.fixture.clone()),
            stream: run.stream.clone(),
            choice,
            action,
        };
        crate::view::composer::protected_actions::start_slash(&root, request).map_err(|refused| {
            crate::app_modal::reject_action(&mut state.write(), &refused.request.action);
            refused.error
        })
    })();
    match result {
        Ok(parent) => *run.parent.lock().unwrap() = Some(parent),
        Err(error) => run
            .actions
            .lock()
            .unwrap()
            .push(json!({"kind":"refused","error":format!("{error:?}")})),
    }
    true
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if enabled() {
        if let Some(run) = RUN.get() {
            run.native
                .lock()
                .unwrap()
                .push(json!({"kind":kind,"evidence":evidence}));
        }
    }
}
