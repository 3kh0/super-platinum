//! Opt-in isolated local-write/consume/native-clear acceptance, never delivery.
//! This entire module is compiled only with protected-native-fixture.
use super::Owner;
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::{core::Task, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
pub(crate) mod check;
mod control;
mod inventory;
mod isolation;
mod seed;
pub(crate) use control::{arm_failure, retry_input, return_owner, rotate_away};
pub(crate) use inventory::{durable, local_json, projection, row_json};
pub(crate) use isolation::wait_runner;
pub(crate) use seed::install;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000191";
pub(crate) const OTHER_ACCOUNT: &str = "00000000-0000-4000-8000-000000000192";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const INITIAL: &str = "Protected local draft";
pub(crate) const REQUESTED: &str =
    "Frozen local intent \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nsecond line";
pub(crate) const NEWER: &str =
    "Newer local typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "consume-channel",
    "consume-thread",
    "expired-clear-thread",
    "canceled-clear-channel",
    "failed-write-return-channel",
];
static BINDING: OnceLock<Arc<Binding>> = OnceLock::new();
thread_local! { static WAITER: RefCell<Option<Task>> = const { RefCell::new(None) }; }
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-drafts" | "protected-drafts-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_DRAFTS_CHECK").as_deref() == Ok("1")
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: (target == ComposerTarget::Thread).then(|| ROOT.into()),
    }
}
pub(crate) fn local(text: &str, target: ComposerTarget) -> LocalDraft {
    let mut composer = super_platinum_core::ComposerState::with_text(text);
    composer.reply_broadcast = target == ComposerTarget::Thread;
    LocalDraft {
        composer,
        attachments: vec![],
    }
}
pub(crate) fn apply(shell: &mut ShellState) {
    crate::drafts::projection_fixture::apply(shell);
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.cache = None;
    shell.core.drafts = Default::default();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        shell
            .core
            .drafts
            .items
            .insert(key(target), local(INITIAL, target));
    }
    shell.core.composer = local(INITIAL, ComposerTarget::Channel).composer;
    shell.core.thread_composer = local(INITIAL, ComposerTarget::Thread).composer;
    shell.sync_composer_drafts();
}
pub(crate) struct Binding {
    pub account: Arc<Account>,
    pub registry: Arc<registry::Registry>,
    pub root: PathBuf,
    pub nonce: String,
    pub scenario: String,
    pub epoch: AtomicU64,
    pub view_id: u64,
    isolated: durable::test_support::Isolated,
    stream: Option<durable::Stream>,
    writer_ids: BTreeMap<DraftKey, String>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    pub actions: Mutex<Vec<Value>>,
    pub native: Mutex<Vec<Value>>,
    held: AtomicBool,
    released: AtomicBool,
    inflight: AtomicBool,
    pub result: Mutex<Option<Value>>,
    changed: tokio::sync::Notify,
}
impl Binding {
    pub(crate) fn complete_operation(&self, operation: &operation::Operation, applied: &Applied) {
        *self.result.lock().unwrap() = Some(
            json!({"kind":"completed", "operationId":operation.id(),
            "operationStatus":"ConsumedAwaitingDelivery", "nativeClear":format!("{:?}",applied.native_clear),
            "before":row_json(&applied.before),"after":row_json(&applied.after),"serial":applied.serial}),
        );
        self.inflight.store(false, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub(crate) fn fail_operation(&self, error: durable::Error) {
        *self.result.lock().unwrap() = Some(json!({"kind":"error","error":err(error)}));
        self.inflight.store(false, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub fn target(&self) -> ComposerTarget {
        if self.scenario.ends_with("thread") {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        }
    }
    pub fn owner(&self, target: ComposerTarget) -> Result<Owner, String> {
        self.account.owner(key(target)).map_err(err)
    }
    pub fn editor(&self, target: ComposerTarget) -> Result<Arc<editor::Editor>, String> {
        self.registry
            .get(ACCOUNT, self.epoch.load(Ordering::Acquire), &key(target))
            .ok_or_else(|| "Protected fixture writer is not mounted".into())
    }
    pub fn isolated_shell(&self, s: &ShellState) -> bool {
        enabled()
            && s.core.session.is_none()
            && s.core.transport.is_none()
            && s.core.cache.is_none()
            && s.draft_writes.view_id == self.view_id
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.isolated_shell(s)
            && s.core.active_account.as_deref() == Some(ACCOUNT)
            && s.core.active_team.as_deref() == Some("T1")
            && s.core.account_epoch == self.epoch.load(Ordering::Acquire)
    }
    /// No socket/Binding conversion. This is an actual isolated opaque Stream.
    pub fn current_stream(
        &self,
        s: &ShellState,
        target: ComposerTarget,
        editor: &Arc<editor::Editor>,
    ) -> Result<Option<durable::Stream>, durable::Error> {
        let stream = self.stream.as_ref().ok_or(durable::Error::Gap)?;
        if !self.current(s)
            || target != self.target()
            || editor.scope().account_id != ACCOUNT
            || editor.scope().key != key(target)
            || !self
                .registry
                .current(ACCOUNT, s.core.account_epoch, &key(target), editor)
            || stream.account() != ACCOUNT
            || stream.team() != "T1"
            || !stream.is_live()
        {
            return Err(durable::Error::Stale);
        }
        Ok(Some(stream.clone()))
    }
    pub async fn after_consumed(&self, applied: &Applied) {
        self.actions.lock().unwrap().push(json!({"kind":"consumed","before":row_json(&applied.before),"after":row_json(&applied.after),"serial":applied.serial}));
        self.held.store(true, Ordering::Release);
        self.changed.notify_waiters();
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.released.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
    pub async fn wait_held(&self) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.held.load(Ordering::Acquire) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Protected admission gate timed out".into())
    }
    pub fn release_gate(&self) {
        self.released.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub async fn wait_result(&self) -> Result<Value, String> {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if let Some(v) = self.result.lock().unwrap().clone() {
                    return v;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Protected renderer action timed out".into())
    }
    pub async fn wait_completed(&self) -> Result<Value, String> {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if let Some(value) = self
                    .result
                    .lock()
                    .unwrap()
                    .clone()
                    .filter(|v| v["kind"] != "canceled")
                {
                    return value;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Retained operation did not complete".into())
    }
    pub fn cancel_waiter(&self) -> Result<(), String> {
        if self.scenario != "canceled-clear-channel" || !self.held.load(Ordering::Acquire) {
            return Err("Wrong cancellation phase".into());
        }
        let task = WAITER
            .with_borrow_mut(Option::take)
            .ok_or("Action waiter missing")?;
        task.cancel();
        self.actions
            .lock()
            .unwrap()
            .push(json!({"kind":"waiter-canceled"}));
        *self.result.lock().unwrap() = Some(json!({"kind":"canceled"}));
        self.changed.notify_waiters();
        Ok(())
    }
}
pub(crate) fn binding(shell: &ShellState) -> Option<Arc<Binding>> {
    BINDING.get().filter(|b| b.current(shell)).cloned()
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if super::adoption_fixture::enabled() {
        super::adoption_fixture::record_native(kind, evidence);
        return;
    }
    if super::enrollment_fixture::enabled() {
        super::enrollment_fixture::record_native(kind, evidence);
        return;
    }
    if enabled() {
        if let Some(b) = BINDING.get() {
            b.native
                .lock()
                .unwrap()
                .push(json!({"kind":kind,"evidence":evidence}));
        }
    }
}
pub(crate) async fn checkpoint(applied: &Applied) {
    if enabled() {
        if let Some(b) = BINDING.get() {
            if applied.after.scope().account_id == ACCOUNT
                && applied.after.scope().key == key(b.target())
            {
                b.after_consumed(applied).await;
            }
        }
    }
}
pub(crate) fn route(shell: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(shell) else {
        return;
    };
    for (key, local) in changes {
        let result = b
            .registry
            .get(ACCOUNT, shell.core.account_epoch, &key)
            .ok_or(durable::Error::Stale)
            .and_then(|e| e.submit(&local));
        match result {
            Ok(serial) => {
                b.routed.fetch_add(1, Ordering::Relaxed);
                b.actions
                    .lock()
                    .unwrap()
                    .push(json!({"kind":"input","key":key,"serial":serial,"local":local}));
            }
            Err(e) => {
                b.refused.fetch_add(1, Ordering::Relaxed);
                b.actions
                    .lock()
                    .unwrap()
                    .push(json!({"kind":"input-refused","key":key,"error":format!("{e:?}")}));
            }
        }
    }
}
/// The actual existing composer Send callback reaches this feature-only route.
/// This acceptance route consumes locally and never prepares a Slack request.
pub(crate) fn intercept(state: Signal<ShellState>, target: ComposerTarget) -> bool {
    if !named() {
        return false;
    }
    let Some(b) = binding(&state.peek()) else {
        return true;
    };
    if target != b.target()
        || b.scenario == "failed-write-return-channel"
        || b.result.lock().unwrap().is_some()
        || b.inflight.swap(true, Ordering::AcqRel)
    {
        return true;
    }
    let opened = (|| {
        let root = dioxus::core::try_consume_context::<
            std::rc::Rc<crate::view::composer::protected_operations::Root>,
        >()
        .ok_or(durable::Error::Stale)?;
        let editor = b.editor(target).map_err(|_| durable::Error::Stale)?;
        crate::view::composer::protected_actions::start(
            &root,
            crate::view::composer::protected_actions::Start {
                state,
                target,
                registry: b.registry.clone(),
                editor,
                visit: Visit::new(),
                request: operation::Request::Send,
                route: crate::view::composer::protected_actions::Route::Fixture(b.clone()),
            },
        )
        .map_err(|refused| refused.error)
    })();
    let operation = match opened {
        Ok(operation) => operation,
        Err(error) => {
            b.fail_operation(error);
            return true;
        }
    };
    // This is ONLY a waiter. The actual future remains in the renderer root.
    let task = spawn(async move {
        let _ = operation.ready().await;
    });
    WAITER.with_borrow_mut(|slot| *slot = Some(task));
    true
}
fn err(error: durable::Error) -> String {
    format!("{error:?}")
}
