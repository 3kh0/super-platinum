//! Named disconnected execution cohort; never an ordinary selector.
//! Actual cold owner, socket provenance, native admission and typed executor run.
//! Only captured synthetic credentials and the final HTTP response are supplied.
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
pub(crate) mod check;
mod evidence;
mod http;
mod isolation;
mod seed;
pub(crate) use evidence::{inventory, snapshot};
pub(crate) use http::response;
pub(crate) use seed::install;
pub(crate) const ROOT_TS: &str = "1719800400.000000";
pub(crate) const INITIAL: &str = "Protected owned draft";
pub(crate) const REQUESTED: &str =
    "Frozen owned send \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nsecond line";
pub(crate) const NEWER: &str =
    "Newer owned typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const ACK_TS: &str = "1719800500.000001";
pub(crate) const SCENARIOS: &[&str] = &[
    "newer-before-prepare-channel",
    "newer-before-prepare-thread",
];
static RUN: OnceLock<Arc<Run>> = OnceLock::new();
thread_local! {static SESSION:RefCell<Option<Rc<RefCell<Session>>>>=const{RefCell::new(None)};}
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-owned-send" | "protected-owned-send-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROTECTED_OWNED_SEND_CHECK").as_deref() == Ok("1")
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: (target == ComposerTarget::Thread).then(|| ROOT_TS.into()),
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
    shell.core.active_account = None;
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
pub(crate) struct Run {
    pub account: Arc<Account>,
    pub registry: Arc<registry::Registry>,
    pub source: Arc<Source>,
    // Dropping the actual captured worker would retire its socket proof.
    pub worker: Mutex<crate::drafts::observation::WorkerSource>,
    pub network: crate::drafts::network::Binding,
    pub root: std::path::PathBuf,
    pub captures: std::path::PathBuf,
    pub nonce: String,
    pub scenario: String,
    pub ticket: String,
    pub epoch: u64,
    pub view_id: u64,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    pub actions: Mutex<Vec<Value>>,
    pub native: Mutex<Vec<Value>>,
    pub attempts: Mutex<Vec<Value>>,
    pub parent: Mutex<Option<operation::Operation>>,
    pub held: AtomicBool,
    pub released: AtomicBool,
    changed: tokio::sync::Notify,
}
impl Run {
    pub fn target(&self) -> ComposerTarget {
        if self.scenario.ends_with("thread") {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        }
    }
    pub fn editor(&self, target: ComposerTarget) -> Result<Arc<editor::Editor>, durable::Error> {
        self.registry
            .get(self.account.id(), self.epoch, &key(target))
            .ok_or(durable::Error::Stale)
    }
    pub fn current(&self, s: &ShellState) -> bool {
        enabled()
            && seed::permits_socket(s)
            && s.core.account_epoch == self.epoch
            && s.draft_writes.view_id == self.view_id
            && self.source.current(s)
            && self.network.current(s)
    }
    pub async fn wait_held(&self) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.held.load(Ordering::Acquire) {
                    break;
                }
                changed.await;
            }
        })
        .await
        .map_err(|_| "Owned post-clear gate timed out".into())
    }
    pub fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
}
pub(crate) fn permits_socket(s: &ShellState) -> bool {
    seed::permits_socket(s)
}
pub(crate) fn binding(s: &ShellState) -> Option<Arc<Run>> {
    RUN.get().filter(|b| b.current(s)).cloned()
}
pub(crate) fn record_native(kind: &str, evidence: Value) {
    if enabled() {
        if let Some(b) = RUN.get() {
            b.native
                .lock()
                .unwrap()
                .push(json!({"kind":kind,"evidence":evidence}));
        }
    }
}
pub(crate) fn route(s: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(s) else {
        return;
    };
    for (key, local) in changes {
        let result = b
            .registry
            .get(b.account.id(), b.epoch, &key)
            .ok_or(durable::Error::Stale)
            .and_then(|e| e.submit(&local));
        match result {
            Ok(serial) => {
                b.routed.fetch_add(1, Ordering::AcqRel);
                b.actions
                    .lock()
                    .unwrap()
                    .push(json!({"kind":"input","key":key,"serial":serial,"local":local}));
            }
            Err(e) => {
                b.refused.fetch_add(1, Ordering::AcqRel);
                b.actions
                    .lock()
                    .unwrap()
                    .push(json!({"kind":"input-refused","key":key,"error":format!("{e:?}")}));
            }
        }
    }
}
pub(crate) fn poll(s: &ShellState) -> bool {
    let Some(b) = binding(s) else {
        return false;
    };
    // Once captured, the real Delivery owns publication using its original
    // Session predecessor. The fixture must not race that predecessor forward.
    if b.parent.lock().unwrap().is_some() {
        return false;
    }
    SESSION.with_borrow(|slot| {
        let Some(session) = slot else {
            return false;
        };
        let Ok(mut session) = session.try_borrow_mut() else {
            return false;
        };
        let mut changed = false;
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let Ok(editor) = b.editor(target) else {
                continue;
            };
            let view = editor.snapshot().view;
            let Some(before) = session
                .entry(b.account.id(), b.epoch, &key(target))
                .cloned()
            else {
                continue;
            };
            if !before.same(&view) {
                changed |= session.apply(b.account.id(), b.epoch, &before, view);
            }
        }
        changed
    })
}
pub(crate) fn intercept(state: Signal<ShellState>, target: ComposerTarget) -> bool {
    if !named() {
        return false;
    }
    let Some(b) = binding(&state.peek()) else {
        return true;
    };
    if target != b.target() || b.parent.lock().unwrap().is_some() {
        return true;
    }
    let result = (|| {
        let root = dioxus::core::try_consume_context::<
            Rc<crate::view::composer::protected_operations::Root>,
        >()
        .ok_or(durable::Error::Stale)?;
        let editor = b.editor(target)?;
        let session = SESSION
            .with_borrow(|s| s.clone())
            .ok_or(durable::Error::Stale)?;
        let delivery = delivery::Delivery::capture(
            state,
            session,
            b.registry.clone(),
            editor.clone(),
            b.source.clone(),
            b.network.clone(),
        )?;
        crate::view::composer::protected_actions::start(
            &root,
            crate::view::composer::protected_actions::Start {
                state,
                target,
                registry: b.registry.clone(),
                editor,
                visit: Visit::new(),
                request: operation::Request::Send,
                route: crate::view::composer::protected_actions::Route::Delivery(delivery, || {
                    1_000
                }),
            },
        )
        .map_err(|v| v.error)
    })();
    match result {
        Ok(parent) => {
            *b.parent.lock().unwrap() = Some(parent);
        }
        Err(e) => {
            b.refused.fetch_add(1, Ordering::AcqRel);
            b.actions
                .lock()
                .unwrap()
                .push(json!({"kind":"send-refused","error":format!("{e:?}")}));
        }
    }
    true
}
/// Called only after the real native clear result has been recorded; this hold
/// changes scheduling, never the frozen input, native ACK or queue admission.
pub(crate) async fn after_clear(
    parent: &operation::Operation,
    applied: &Applied,
) -> Result<(), durable::Error> {
    if !named() {
        return Ok(());
    }
    let b = RUN.get().ok_or(durable::Error::Unsupported)?;
    if !enabled()
        || applied.after.scope().account_id != b.account.id()
        || applied.after.scope().key != key(b.target())
        || applied.native_clear != NativeClear::Cleared
        || !parent.current()
    {
        return Err(durable::Error::Stale);
    }
    b.held.store(true, Ordering::Release);
    b.changed.notify_waiters();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let changed = b.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if b.released.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    })
    .await
    .map_err(|_| durable::Error::Pending)?;
    if !parent.current() {
        return Err(durable::Error::Stale);
    }
    Ok(())
}
