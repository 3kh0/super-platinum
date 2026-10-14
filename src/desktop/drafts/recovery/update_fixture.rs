//! Exact opted-in isolated Update recovery. No production session or Transport.
use super::*;
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Weak,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use super_platinum_core::{
    ComposerState,
    config::WorkspaceSession,
    drafts::store::sync::LocalReview,
    slack::{SlackClient, api, client::RequestBody},
};
mod isolation;
mod seed;
pub(crate) use isolation::wait_runner;
pub(crate) use seed::install;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000091";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const BASE: &str = "Previous acknowledged content";
pub(crate) const REQUESTED: &str =
    "Requested update \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const NEWER: &str =
    "Newer local typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "positive-channel",
    "typing-before-channel",
    "typing-during-commit-thread",
    "owner-rotation-channel",
    "cooldown-channel",
    "pending-check-channel",
];
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("draft-update-recovery" | "draft-update-recovery-narrow")
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
pub(crate) fn apply(s: &mut ShellState) {
    crate::drafts::projection_fixture::apply(s);
    s.core.active_account = Some(ACCOUNT.into());
    s.self_account.user_id = "U00000001".into();
    s.core.workspaces.get_mut("T1").unwrap().self_user_id = "U00000001".into();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        s.core
            .drafts
            .items
            .insert(key(target), local(REQUESTED, target));
    }
    s.core.drafts.channel = None;
    s.core.drafts.thread = None;
    s.sync_composer_drafts();
}
pub(crate) struct FixtureBinding {
    pub account: Arc<Account>,
    pub path: PathBuf,
    pub nonce: String,
    pub scenario: String,
    pub phase: String,
    pub epoch: u64,
    pub editors: BTreeMap<DraftKey, Arc<checkpoint::Editor>>,
    pub requests: Mutex<Vec<Value>>,
    pub routed: AtomicU64,
    pub ui_done: AtomicBool,
    gate: Mutex<Gate>,
}
#[derive(Default)]
struct Gate {
    armed: Option<&'static str>,
    reached: bool,
    released: bool,
}
fn installed() -> &'static Mutex<Weak<FixtureBinding>> {
    static VALUE: OnceLock<Mutex<Weak<FixtureBinding>>> = OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}
impl FixtureBinding {
    pub fn target(&self) -> ComposerTarget {
        if self.scenario.ends_with("thread") {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        }
    }
    pub fn owner(&self, key: &DraftKey) -> crate::drafts::Owner {
        self.account.owner(key.clone())
    }
    fn configured(&self) -> bool {
        enabled()
            && std::env::var_os("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_DB")
                .is_some_and(|p| PathBuf::from(p) == self.path)
            && std::env::var("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_PHASE").as_deref()
                == Ok(self.phase.as_str())
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.configured()
            && s.core.session.is_none()
            && s.core.transport.is_none()
            && s.core.active_account.as_deref() == Some(ACCOUNT)
            && s.core.account_epoch == self.epoch
            && s.self_account.user_id == "U00000001"
            && s.core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.self_user_id == "U00000001")
            && self.editors.iter().all(|(key, e)| {
                s.draft_writes
                    .writers
                    .get(ACCOUNT, self.epoch, key)
                    .is_some_and(|found| Arc::ptr_eq(&found, e))
            })
    }
    pub fn arm(&self, stage: &'static str) -> Result<(), String> {
        let mut g = self.gate.lock().map_err(|_| "Gate unavailable")?;
        if g.armed.is_some() {
            return Err("Gate already armed".into());
        }
        *g = Gate {
            armed: Some(stage),
            reached: false,
            released: false,
        };
        Ok(())
    }
    pub async fn wait_reached(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.gate.lock().unwrap().reached {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Update gate not reached".into())
    }
    pub async fn wait_ui(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.ui_done.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Update UI completion did not settle".into())
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
        // A pending-check fixture deliberately exits while this response is held.
        // Timeout stays pending too; it never fabricates a completion.
        loop {
            {
                let mut g = self.gate.lock().unwrap();
                if g.released {
                    *g = Gate::default();
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    pub async fn read(&self, op: &Operation, now_ms: u64) -> Readback {
        if !self.configured()
            || self.phase != "exercise"
            || op.scope.account_id != ACCOUNT
            || op.scope.key != key(self.target())
            || op.kind != OperationKind::Update
            || op.base.is_none()
        {
            return Readback::blocked(network::Block::Scope);
        }
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Disconnected Update fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-not-a-token".into(),
        };
        let request = api::drafts_info(
            &SlackClient::default(),
            &workspace,
            &op.base.as_ref().unwrap().id,
        )
        .unwrap();
        let retry_safe = request.retry_safe();
        let RequestBody::Form(fields) = request.body else {
            panic!("Expected drafts.info form")
        };
        let fields: Vec<_> = fields.into_iter().filter(|(k, _)| k != "token").collect();
        let url = url::Url::parse(&request.url).unwrap();
        let snapshot = op.snapshot.as_ref().unwrap();
        let raw = json!({"ok":true,"draft":{"id":op.base.as_ref().unwrap().id,"client_msg_id":op.client_msg_id,"last_updated_ts":"12.000001","team_id":"T1","user_id":"U00000001","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations}});
        let cooldown = self.scenario == "cooldown-channel";
        self.requests.lock().unwrap().push(json!({"method":request.method,"url":request.url,"path":url.path(),"rawQuery":url.query(),"fields":fields,"retrySafe":retry_safe,"delivered":false,"operation":op,"rawResponse":if cooldown{Value::Null}else{raw.clone()},"outcome":if cooldown{"rate-limited"}else{"positive"},"nowMs":now_ms,"retryAfterSeconds":if cooldown{json!(120)}else{Value::Null}}));
        self.checkpoint("read").await;
        if let Some(row) = self.requests.lock().unwrap().last_mut() {
            row["delivered"] = json!(true);
        }
        if cooldown {
            Readback {
                outcome: None,
                raw: None,
                issue: Some(network::Issue::RateLimited),
                cooldown_until_ms: Some(now_ms.saturating_add(120000)),
                requested: true,
                has_more: false,
            }
        } else {
            network::decode_update(op, &workspace, raw)
        }
    }
}
pub(crate) fn binding(s: &ShellState) -> Option<Arc<FixtureBinding>> {
    installed().lock().ok()?.upgrade().filter(|b| b.current(s))
}
pub(crate) fn route(s: &ShellState, changes: BTreeMap<DraftKey, LocalDraft>) {
    let Some(b) = binding(s) else { return };
    for (key, local) in changes {
        if let Some(e) = b.editors.get(&key) {
            e.submit(local);
            b.routed.fetch_add(1, Ordering::Relaxed);
        }
    }
}
pub(crate) async fn commit_checkpoint(scope: &Scope) {
    if !enabled() || scope.account_id != ACCOUNT {
        return;
    }
    let b = installed().lock().ok().and_then(|b| b.upgrade());
    if let Some(b) = b
        && b.configured()
        && b.phase == "exercise"
    {
        b.checkpoint("commit").await;
    }
}
pub(crate) fn inventory_json(review: &LocalReview) -> Value {
    crate::drafts::remote_review_fixture::inventory_json(review)
}
pub(crate) async fn durable(b: &FixtureBinding) -> Result<Value, String> {
    let reader = Account::new(b.account.backend.clone());
    let mut rows = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let review = reader
            .owner(key(target))
            .review_local()
            .await
            .map_err(|e| format!("Read private inventory: {e:?}"))?;
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

pub(crate) fn ui_completed(scope: &Scope) {
    if !enabled() || scope.account_id != ACCOUNT {
        return;
    }
    if let Some(b) = installed().lock().ok().and_then(|b| b.upgrade())
        && b.configured()
        && scope.key == key(b.target())
    {
        b.ui_done.store(true, Ordering::Release);
    }
}
