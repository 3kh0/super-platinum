//! Isolated native adoption harness. Host ownership survives ShellState replacement.
//! Construction has no Source, Session, Transport, socket, or default store.
use super::{Owner, *};
use crate::state::{ComposerTarget, ShellState};
use dioxus::{core::Task, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
mod adoption;
pub(crate) mod check;
mod control;
mod controller;
mod inventory;
mod isolation;
mod replacement;
mod resume;
mod seed;
mod ui;
pub(crate) use control::{arm_failure, flush_inputs, old_lane_refuses, route};
pub(crate) use inventory::{durable, projection};
pub(crate) use isolation::{preflight, wait_runner};
pub(crate) use replacement::replace_shell;
pub(crate) use seed::install;
pub(crate) use ui::view;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000351";
pub(crate) const ROOT: &str = super::enrollment_fixture::ROOT;
pub(crate) const INITIAL: &str = super::enrollment_fixture::INITIAL;
pub(crate) const NEWER: &str = super::enrollment_fixture::NEWER;
pub(crate) const SCENARIOS: &[&str] = &[
    "adopt-channel",
    "adopt-thread",
    "late-input-channel",
    "native-refusals-channel",
    "owner-replacement-channel",
    "cancel-boundaries-channel",
    "failed-managed-write-channel",
];
static BINDING: OnceLock<Arc<Binding>> = OnceLock::new();
thread_local! { static WAITER: RefCell<Option<Task>> = const { RefCell::new(None) }; }
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-adoption" | "protected-adoption-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_ADOPTION_CHECK").as_deref() == Ok("1")
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    super::enrollment_fixture::key(target)
}
pub(crate) fn local(text: &str, target: ComposerTarget) -> LocalDraft {
    super::enrollment_fixture::local(text, target)
}
pub(crate) fn apply(shell: &mut ShellState) {
    super::enrollment_fixture::apply(shell);
    shell.core.active_account = Some(ACCOUNT.into());
}
pub(crate) struct Binding {
    pub account: Arc<Account>,
    pub root: PathBuf,
    capture_dir: PathBuf,
    pub nonce: String,
    pub scenario: String,
    isolated: durable::test_support::Isolated,
    pub host: Mutex<lifecycle::Host>,
    selected: Mutex<lifecycle::Selection>,
    selected_view: Mutex<(u64, u64)>,
    opened: BTreeMap<DraftKey, lifecycle::LocalOpen>,
    adoption: Mutex<Option<lifecycle::Adoption>>,
    registered: Mutex<Option<Arc<lifecycle::AdoptionResult>>>,
    install_visit: Mutex<Option<Visit>>,
    acknowledged: Mutex<std::collections::BTreeSet<String>>,
    registry_labels: Mutex<Vec<(Arc<registry::Registry>, String)>>,
    armed: Mutex<std::collections::BTreeSet<String>>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    actions: Mutex<Vec<Value>>,
    native: Mutex<Vec<Value>>,
    result: Mutex<Option<Value>>,
    gate: Mutex<Option<String>>,
    releases: Mutex<std::collections::BTreeSet<String>>,
    changed: tokio::sync::Notify,
}
impl Binding {
    pub async fn capture_path(&self, name: &str) -> Result<PathBuf, String> {
        isolation::capture_path(self, name).await
    }
    pub fn target(&self) -> ComposerTarget {
        if self.scenario == "adopt-thread" {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        }
    }
    pub fn owner(&self, target: ComposerTarget) -> Result<Owner, String> {
        self.account.owner(key(target)).map_err(err)
    }
    pub fn current(&self, s: &ShellState) -> bool {
        let selected = *self.selected_view.lock().unwrap();
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
        assert!(actions.len() < 256, "bounded adoption fixture ledger");
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
                return Err("Adoption owner changed".into());
            }
            self.host.lock().unwrap().begin_load(&s).map_err(err)?
        };
        ready.ready().await;
        let s = state.peek();
        if !self.current(&s) {
            return Err("Adoption owner changed during read".into());
        }
        self.host
            .lock()
            .unwrap()
            .finish_load(&s, &ready)
            .map_err(err)?;
        self.record(json!({"kind":"current-load","epoch":s.core.account_epoch,"viewId":s.draft_writes.view_id}));
        Ok(())
    }
    pub fn original_lane(&self, target: ComposerTarget) -> Result<Arc<lifecycle::Lane>, String> {
        let opened = self
            .opened
            .get(&key(target))
            .ok_or("Original local lane missing")?;
        self.host.lock().unwrap().local(opened).map_err(err)
    }
}
pub(crate) fn binding(s: &ShellState) -> Option<Arc<Binding>> {
    BINDING.get().filter(|b| b.current(s)).cloned()
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if enabled() {
        if let Some(b) = BINDING.get() {
            let mut values = b.native.lock().unwrap();
            assert!(values.len() < 256, "bounded adoption native evidence");
            values.push(json!({"kind":kind,"evidence":evidence}));
        }
    }
}
fn err(error: durable::Error) -> String {
    format!("{error:?}")
}
