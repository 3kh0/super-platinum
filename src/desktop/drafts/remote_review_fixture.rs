//! Isolated native review evidence. Never constructs a Transport or loads a session.
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Weak,
    sync::atomic::{AtomicU64, Ordering},
};
use super_platinum_core::{
    ComposerState,
    drafts::store::sync::{LocalReview, RemoteReview},
};
mod isolation;
mod seed;
pub(crate) use isolation::wait_runner;

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000081";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const BEFORE: &str =
    "Current local \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const AFTER: &str = "Latest Slack \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const NEWER: &str = "Newer typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const ALTERNATE: &str =
    "Saved alternate \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const BLOCKED: &str =
    "Preserved local \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const OLDER: &str = "Previously observed Slack version";
pub(crate) const SCENARIOS: &[&str] = &[
    "remote-channel",
    "alternate-thread",
    "typing-during-read-channel",
    "typing-during-commit-thread",
    "changed-thread",
    "deferred-composition-channel",
];
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_DRAFT_REVIEW_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("draft-remote-review" | "draft-remote-review-narrow")
        )
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: matches!(target, ComposerTarget::Thread).then(|| ROOT.into()),
    }
}
pub(crate) fn local(text: &str, target: ComposerTarget) -> LocalDraft {
    let mut composer = ComposerState::with_text(text);
    composer.reply_broadcast = matches!(target, ComposerTarget::Thread);
    LocalDraft {
        composer,
        attachments: vec![],
    }
}
pub(crate) fn apply(shell: &mut ShellState) {
    // Static capture is meaningful without a check flag, and opens no storage.
    super::projection_fixture::apply(shell);
    shell.core.active_account = Some(ACCOUNT.into());
    shell.self_account.user_id = "U00000001".into();
    if let Some(workspace) = shell.core.workspaces.get_mut("T1") {
        workspace.self_user_id = "U00000001".into();
    }
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        shell
            .core
            .drafts
            .items
            .insert(key(target), local(BEFORE, target));
    }
    shell.core.drafts.channel = None;
    shell.core.drafts.thread = None;
    shell.sync_composer_drafts();
}
pub(crate) struct Binding {
    pub account: Arc<Account>,
    pub path: PathBuf,
    pub nonce: String,
    pub scenario: String,
    pub phase: String,
    pub epoch: u64,
    pub editors: BTreeMap<DraftKey, Arc<checkpoint::Editor>>,
    pub requests: Mutex<Vec<Value>>,
    pub routed: AtomicU64,
    gate: Mutex<Gate>,
}
#[derive(Default)]
struct Gate {
    armed: Option<&'static str>,
    reached: bool,
    released: bool,
}
fn installed() -> &'static Mutex<Weak<Binding>> {
    static VALUE: OnceLock<Mutex<Weak<Binding>>> = OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}
impl Binding {
    pub fn target(&self) -> ComposerTarget {
        if self.scenario.ends_with("thread") {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        }
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.configured()
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.active_account.as_deref() == Some(ACCOUNT)
            && shell.core.account_epoch == self.epoch
            && shell.self_account.user_id == "U00000001"
            && shell
                .core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U00000001")
            && self.editors.iter().all(|(key, editor)| {
                shell
                    .draft_writes
                    .writers
                    .get(ACCOUNT, self.epoch, key)
                    .is_some_and(|e| Arc::ptr_eq(&e, editor))
            })
    }
    fn configured(&self) -> bool {
        enabled()
            && std::env::var_os("SUPER_PLATINUM_DRAFT_REVIEW_DB")
                .is_some_and(|p| PathBuf::from(p) == self.path)
            && std::env::var("SUPER_PLATINUM_DRAFT_REVIEW_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_REVIEW_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_REVIEW_PHASE").as_deref()
                == Ok(self.phase.as_str())
    }
    pub fn arm(&self, stage: &'static str) -> Result<(), String> {
        let mut gate = self.gate.lock().map_err(|_| "Gate poisoned")?;
        if gate.armed.is_some() {
            return Err("Gate already armed".into());
        }
        *gate = Gate {
            armed: Some(stage),
            reached: false,
            released: false,
        };
        Ok(())
    }
    pub async fn wait_reached(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.gate.lock().map_err(|_| "Gate poisoned")?.reached {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Fixture checkpoint not reached".into())
    }
    pub fn release(&self) {
        self.gate.lock().unwrap().released = true;
    }
    async fn checkpoint(&self, stage: &'static str) {
        {
            let mut g = self.gate.lock().unwrap();
            if g.armed != Some(stage) {
                return;
            }
            g.reached = true;
        }
        for _ in 0..500 {
            {
                let mut g = self.gate.lock().unwrap();
                if g.released {
                    *g = Gate::default();
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        // Timeout never fabricates completion. The checker fails; actual owner
        // guards still govern any continuation after the retained task resumes.
    }
}
pub(crate) fn binding(shell: &ShellState) -> Option<Arc<Binding>> {
    installed()
        .lock()
        .ok()?
        .upgrade()
        .filter(|b| b.current(shell))
}
pub(crate) fn route(shell: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(shell) else { return };
    for (key, local) in changes {
        if let Some(editor) = b.editors.get(&key) {
            editor.submit(local);
            b.routed.fetch_add(1, Ordering::Relaxed);
        }
    }
}
/// Called only inside the retained Editor commit, after serial admission.
pub(crate) async fn commit_checkpoint(scope: &Scope, editor: &Arc<checkpoint::Editor>) {
    if !enabled() || scope.account_id != ACCOUNT {
        return;
    }
    let b = installed().lock().ok().and_then(|v| v.upgrade());
    if let Some(b) = b
        && b.configured()
        && b.phase == "exercise"
        && b.editors
            .get(&scope.key)
            .is_some_and(|e| Arc::ptr_eq(e, editor))
    {
        b.checkpoint("during-commit").await;
    }
}
#[derive(Clone)]
pub(crate) struct FixtureBinding {
    binding: Arc<Binding>,
    key: DraftKey,
}
#[derive(Clone)]
pub(crate) struct FixtureProof {
    binding: FixtureBinding,
    review: RemoteReview,
}
impl FixtureBinding {
    pub fn capture(shell: &ShellState, key: &DraftKey) -> Option<Self> {
        let binding = binding(shell)?;
        (binding.phase == "exercise"
            && *key == self::key(binding.target())
            && binding.editors.contains_key(key))
        .then(|| Self {
            binding,
            key: key.clone(),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
    }
    pub async fn checkpoint(&self, stage: &'static str) {
        self.binding.checkpoint(stage).await;
    }
    pub async fn read(&self, inventory: LocalReview) -> Result<FixtureProof, ()> {
        if inventory.scope.account_id != ACCOUNT || inventory.scope.key != self.key {
            return Err(());
        }
        let id = &inventory
            .stored
            .as_ref()
            .ok_or(())?
            .state
            .acknowledged()
            .ok_or(())?
            .id;
        let client = super_platinum_core::slack::client::SlackClient::default();
        let workspace = super_platinum_core::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Isolated review".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only-token".into(),
        };
        let request = super_platinum_core::slack::api::drafts_info(&client, &workspace, id)
            .map_err(|_| ())?;
        let raw = seed::raw(self.binding.target())?;
        let super_platinum_core::slack::client::RequestBody::Form(form) = &request.body else {
            return Err(());
        };
        self.binding.requests.lock().map_err(|_|())?.push(json!({"method":request.method,"url":request.url,"form":form,"retrySafe":request.retry_safe(),"rawResponse":raw,"inventory":inventory_json(&inventory)}));
        self.checkpoint("during-read").await;
        super_platinum_core::slack::api::decode_drafts_info(id, raw.clone()).map_err(|_| ())?;
        let review = RemoteReview::prepare(inventory, raw, "U00000001").map_err(|_| ())?;
        Ok(FixtureProof {
            binding: self.clone(),
            review,
        })
    }
}
impl FixtureProof {
    pub fn review(&self) -> &RemoteReview {
        &self.review
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
    }
    pub fn into_current_review(self, shell: &ShellState) -> Result<RemoteReview, ()> {
        self.current(shell).then_some(self.review).ok_or(())
    }
}
pub(crate) fn inventory_json(review: &LocalReview) -> Value {
    json!({"scope":review.scope,"canonical":review.local,"stored":review.stored.as_ref().map(|s|json!({"revision":s.revision,"state":s.state})),
        "blocked":review.blocked.as_ref().map(|b|json!({"token":b.token,"local":b.draft})),
        "alternates":review.alternates.iter().map(|a|json!({"writer":a.writer_id,"serial":a.revision,"local":a.draft})).collect::<Vec<_>>()})
}
pub(crate) async fn durable(b: &Binding) -> Result<Value, String> {
    let reader = Account::new(b.account.backend.clone());
    let mut rows = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let review = reader
            .owner(key(target))
            .review_local()
            .await
            .map_err(|e| format!("Read isolated inventory: {e:?}"))?;
        rows.insert(
            if matches!(target, ComposerTarget::Thread) {
                "thread"
            } else {
                "channel"
            }
            .into(),
            inventory_json(&review),
        );
    }
    Ok(Value::Object(rows))
}
pub(crate) use seed::install;
