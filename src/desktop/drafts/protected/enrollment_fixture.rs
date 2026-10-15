//! Feature-only isolated consent fixture. No selector, legacy store or network.
use super::{Owner, *};
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
mod controller;
mod inventory;
mod isolation;
mod seed;
mod ui;
pub(crate) use inventory::{durable, projection};
pub(crate) use isolation::wait_runner;
pub(crate) use seed::install;
pub(crate) use ui::view;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000251";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const INITIAL: &str = "Local draft ready for recovery";
pub(crate) const NEWER: &str =
    "Later local input \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "enroll-channel",
    "enroll-thread",
    "late-typing-channel",
    "owner-change-channel",
    "failed-editor-channel",
    "canceled-waiter-channel",
];
static BINDING: OnceLock<Arc<Binding>> = OnceLock::new();
thread_local! { static WAITER: RefCell<Option<Task>> = const { RefCell::new(None) }; }
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-enrollment" | "protected-enrollment-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_ENROLLMENT_CHECK").as_deref() == Ok("1")
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
    composer.set_selection(text.len(), text.len());
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
    pub root: PathBuf,
    capture_dir: PathBuf,
    pub nonce: String,
    pub scenario: String,
    isolated: durable::test_support::Isolated,
    selected: Mutex<(u64, u64)>,
    discovery: Mutex<discovery::Discovery>,
    writers: BTreeMap<DraftKey, Arc<local_writer::LocalWriter>>,
    failed: Option<Arc<editor::Editor>>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    actions: Mutex<Vec<Value>>,
    native: Mutex<Vec<Value>>,
    result: Mutex<Option<Value>>,
    gate: Mutex<Option<String>>,
    released: AtomicBool,
    changed: tokio::sync::Notify,
}
impl Binding {
    pub async fn capture_path(&self, name: &str) -> Result<PathBuf, String> {
        isolation::capture_path(self, name).await
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
    pub fn current(&self, s: &ShellState) -> bool {
        let selected = *self.selected.lock().unwrap();
        enabled()
            && s.core.active_account.as_deref() == Some(ACCOUNT)
            && s.core.active_team.as_deref() == Some("T1")
            && s.core.account_epoch == selected.0
            && s.draft_writes.view_id == selected.1
            && s.core.session.is_none()
            && s.core.transport.is_none()
            && s.core.cache.is_none()
    }
    fn record(&self, value: Value) {
        let mut actions = self.actions.lock().unwrap();
        assert!(actions.len() < 64, "fixture action bound");
        actions.push(value);
    }
    fn result(&self, value: Value) {
        *self.result.lock().unwrap() = Some(value);
        self.changed.notify_waiters();
    }
    pub async fn reload(&self, state: Signal<ShellState>) -> Result<(), String> {
        let ready = {
            let s = state.peek();
            if !self.current(&s) {
                return Err("Enrollment owner changed".into());
            }
            let mut d = self.discovery.lock().unwrap();
            d.select(&s, self.account.clone()).map_err(err)?;
            d.begin_load(&s).map_err(err)?
        };
        ready.ready().await;
        let s = state.peek();
        if !self.current(&s) {
            return Err("Enrollment owner changed during read".into());
        }
        self.discovery
            .lock()
            .unwrap()
            .finish_load(&s, &ready)
            .map_err(err)?;
        self.record(json!({"kind":"current-load","epoch":s.core.account_epoch,"viewId":s.draft_writes.view_id}));
        Ok(())
    }
    async fn hold(&self, phase: &str) {
        let held = if self.scenario == "canceled-waiter-channel" {
            "enqueued"
        } else {
            "prepared"
        };
        if phase != held {
            return;
        }
        *self.gate.lock().unwrap() = Some(phase.into());
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
    pub async fn wait_gate(&self, phase: &str) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.gate.lock().unwrap().as_deref() == Some(phase) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Enrollment phase gate timed out".into())
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
        .map_err(|_| "Enrollment result timed out".into())
    }
}
pub(crate) fn binding(s: &ShellState) -> Option<Arc<Binding>> {
    BINDING.get().filter(|b| b.current(s)).cloned()
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if enabled() {
        if let Some(b) = BINDING.get() {
            let mut values = b.native.lock().unwrap();
            assert!(values.len() < 32, "fixture native bound");
            values.push(json!({"kind":kind,"evidence":evidence}));
        }
    }
}
pub(crate) fn route(s: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(s) else {
        return;
    };
    for (key, value) in changes {
        let result = if let Some(e) = b.failed.as_ref().filter(|e| e.scope().key == key) {
            e.submit(&value)
        } else {
            b.writers
                .get(&key)
                .ok_or(durable::Error::Stale)
                .and_then(|w| w.submit(&value))
        };
        match result {
            Ok(serial) => {
                b.routed.fetch_add(1, Ordering::Relaxed);
                b.record(json!({"kind":"input","key":key,"serial":serial,"local":value}));
            }
            Err(error) => {
                b.refused.fetch_add(1, Ordering::Relaxed);
                b.record(json!({"kind":"input-refused","key":key,"error":err(error)}));
            }
        }
    }
}
pub(crate) async fn flush_inputs(b: &Binding) -> Result<(), String> {
    if let Some(e) = &b.failed {
        let result = e.flush().await;
        if !matches!(result, Err(durable::Error::Storage)) {
            return Err("Expected retained failed writer".into());
        }
    }
    for w in b.writers.values() {
        w.flush().await.map_err(err)?;
    }
    Ok(())
}
pub(crate) async fn arm_failure(b: &Binding) -> Result<(), String> {
    if b.scenario != "failed-editor-channel" {
        return Err("Wrong failure case".into());
    }
    account::wait(
        b.isolated
            .fail_next(durable::test_support::FaultPoint::LocalCommit),
    )
    .await
    .map_err(err)
}
pub(crate) fn replace_shell(mut state: Signal<ShellState>, b: &Binding) -> Result<(), String> {
    if !["owner-change-channel", "canceled-waiter-channel"].contains(&b.scenario.as_str())
        || !b.current(&state.peek())
    {
        return Err("Wrong replacement owner".into());
    }
    let before = {
        let s = state.peek();
        (s.core.account_epoch, s.draft_writes.view_id)
    };
    let mut next = ShellState::fixture(crate::media::MediaRegistry::default());
    apply(&mut next);
    next.main_view = crate::state::MainView::Home;
    next.thread_root = (b.target() == ComposerTarget::Thread).then(|| ROOT.into());
    next.sync_composer_drafts();
    let channel = next
        .core
        .active_channel
        .clone()
        .ok_or("Replacement channel missing")?;
    next.core.pending_scroll_to = Some((
        channel,
        super_platinum_core::domain::PendingScrollTarget::Latest,
    ));
    next.stick_to_bottom = true;
    next.reset_timeline_window();
    next.core.account_epoch = before.0.checked_add(1).ok_or("Epoch exhausted")?;
    // Real replacement creates a fresh desktop view. The Account and Discovery
    // remain in this separately owned Binding, preserving pending receipts.
    if next.draft_writes.view_id == before.1 {
        return Err("Replacement view reused".into());
    }
    b.discovery
        .lock()
        .unwrap()
        .select(&next, b.account.clone())
        .map_err(err)?;
    let after = (next.core.account_epoch, next.draft_writes.view_id);
    *b.selected.lock().unwrap() = after;
    *state.write() = next;
    b.record(json!({"kind":"shell-replaced","before":{"epoch":before.0,"viewId":before.1},"after":{"epoch":after.0,"viewId":after.1}}));
    Ok(())
}
pub(super) fn err(error: durable::Error) -> String {
    format!("{error:?}")
}
