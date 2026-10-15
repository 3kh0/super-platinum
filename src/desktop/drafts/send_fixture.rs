//! Explicitly opted-in synthetic managed Send. No Session, Transport or cache.
use crate::drafts::{Account, Owner, checkpoint, network, session, storage::Backend};
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    ComposerState,
    config::WorkspaceSession,
    drafts::{
        DraftKey, LocalDraft,
        sync::{Operation, OperationKind, Scope},
    },
    slack::{Error, SlackClient, api, client::RequestBody},
};
#[path = "send_fixture/isolation.rs"]
mod isolation;
#[path = "send_fixture/seed.rs"]
mod seed;
pub(crate) use isolation::wait_runner;
pub(crate) use seed::install;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000101";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const ACK_TS: &str = "1719800500.000001";
pub(crate) const BASE: &str = "Previous acknowledged content";
pub(crate) const REQUESTED: &str =
    "Frozen send \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nsecond line";
pub(crate) const NEWER: &str =
    "Newer local typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "ack-newer-channel",
    "ack-newer-thread",
    "lost-ack-channel",
    "lost-ack-thread",
    "held-response-channel",
    "held-response-thread",
];
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("managed-draft-send" | "managed-draft-send-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_CHECK").as_deref() == Ok("1") && named()
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
    s.core.session = None;
    s.core.transport = None;
    s.core.cache = None;
    s.self_account.user_id = "U00000001".into();
    s.core.workspaces.get_mut("T1").unwrap().self_user_id = "U00000001".into();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        s.core.drafts.items.insert(key(target), local(BASE, target));
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
    pub view_id: u64,
    pub editors: BTreeMap<DraftKey, Arc<checkpoint::Editor>>,
    pub requests: Mutex<Vec<Value>>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    pub ui_done: AtomicBool,
    gate: Mutex<Gate>,
}
#[derive(Default)]
struct Gate {
    armed: bool,
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
    pub fn owner(&self, key: &DraftKey) -> Owner {
        self.account.owner(key.clone())
    }
    fn configured(&self) -> bool {
        enabled()
            && std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_SEND_DB")
                .is_some_and(|p| PathBuf::from(p) == self.path)
            && std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var("SUPER_PLATINUM_MANAGED_DRAFT_SEND_PHASE").as_deref()
                == Ok(self.phase.as_str())
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.configured()
            && s.core.session.is_none()
            && s.core.transport.is_none()
            && s.core.cache.is_none()
            && s.core.active_account.as_deref() == Some(ACCOUNT)
            && s.core.account_epoch == self.epoch
            && s.draft_writes.view_id == self.view_id
            && s.self_account.user_id == "U00000001"
            && s.core
                .workspaces
                .get("T1")
                .is_some_and(|w| w.team_id == "T1" && w.self_user_id == "U00000001")
    }
    pub fn record_routing(&self, submitted: usize, refused: usize) {
        self.routed.fetch_add(submitted as u64, Ordering::Relaxed);
        self.refused.fetch_add(refused as u64, Ordering::Relaxed);
    }
    pub fn arm(&self, stage: &'static str) -> Result<(), String> {
        let mut gate = self.gate.lock().map_err(|_| "Gate unavailable")?;
        if stage != "response" || gate.armed {
            return Err("Unsupported or already armed gate".into());
        }
        *gate = Gate {
            armed: true,
            ..Default::default()
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
        Err("Send response gate not reached".into())
    }
    pub async fn wait_ui(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.ui_done.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Send UI completion not reached".into())
    }
    pub fn release(&self) {
        self.gate.lock().unwrap().released = true;
    }
    async fn checkpoint(&self) {
        {
            let mut gate = self.gate.lock().unwrap();
            gate.reached = true;
        }
        // Held-response cases deliberately exit while this remains unresolved.
        // There is no timeout that fabricates an acknowledgement or retries.
        loop {
            if self.gate.lock().unwrap().released {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    pub async fn execute(&self, operation: &Operation, now_ms: u64) -> network::Attempt {
        if !self.configured()
            || self.phase != "exercise"
            || operation.scope.account_id != ACCOUNT
            || operation.scope.key != key(self.target())
            || operation.kind != OperationKind::Send
            || operation.base.is_none()
            || !self.gate.lock().is_ok_and(|g| g.armed)
        {
            return network::Attempt::blocked(network::Block::Scope);
        }
        let owner = self.owner(&operation.scope.key);
        let Ok(view) = owner.load().await else {
            return network::Attempt::blocked(network::Block::Scope);
        };
        let Some(stored) = view
            .stored
            .as_ref()
            .filter(|s| s.state.pending() == Some(operation))
        else {
            return network::Attempt::blocked(network::Block::Scope);
        };
        let Some(consume) = stored.state.consumption().cloned() else {
            return network::Attempt::blocked(network::Block::Scope);
        };
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Isolated Send".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-not-a-token".into(),
        };
        let Ok(prepared) =
            api::prepare_draft_operation(&SlackClient::default(), &workspace, operation, now_ms)
        else {
            return network::Attempt::blocked(network::Block::Unsupported);
        };
        let raw = json!({"ok":true,"channel":"C2","ts":ACK_TS,"draft_id":operation.base.as_ref().unwrap().id,"message":{"client_msg_id":operation.client_msg_id,"user":"U00000001","channel":"C2","ts":ACK_TS,"text":REQUESTED,"blocks":operation.snapshot.as_ref().unwrap().blocks,"thread_ts":operation.scope.key.thread}});
        // Channel ACKs omit the optional thread key rather than asserting null.
        let mut raw = raw;
        if operation.scope.key.thread.is_none() {
            raw["message"].as_object_mut().unwrap().remove("thread_ts");
        }
        let attempt=network::execute_prepared(operation,&workspace,prepared.request,now_ms,|request|async move {
            let RequestBody::Form(fields)=&request.body else {return Err(Error::Transport("Unexpected synthetic body".into()));};
            let form:Vec<_>=fields.iter().filter(|(name,_)|name!="token").cloned().collect();
            let sequence={
                let mut ledger=self.requests.lock().unwrap();
                let sequence=ledger.len()+1;
                ledger.push(json!({"sequence":sequence,"method":request.method,"url":request.url,"form":form,"retrySafe":request.retry_safe(),"operation":operation,"consume":consume,"rawResponse":null,"error":null,"outcome":"pending","delivered":false}));
                sequence
            };
            self.checkpoint().await;
            let lost=self.scenario.starts_with("lost-ack-");
            {
                let mut ledger=self.requests.lock().unwrap();
                let row=&mut ledger[sequence-1];
                row["delivered"]=json!(true);
                if lost {row["error"]=json!("transport");} else {row["rawResponse"]=raw.clone();}
            }
            if lost {Err(Error::Transport("Isolated lost acknowledgement".into()))} else {Ok(raw.clone())}
        }).await;
        if let Some(row) = self
            .requests
            .lock()
            .unwrap()
            .iter_mut()
            .find(|row| row["operation"]["id"] == operation.id)
        {
            row["outcome"] = json!(match &attempt.outcome {
                crate::drafts::dispatch::Outcome::Sent { .. } => "sent",
                crate::drafts::dispatch::Outcome::Uncertain => "uncertain",
                _ => "other",
            });
        }
        attempt
    }
}
pub(crate) fn binding(shell: &ShellState) -> Option<Arc<FixtureBinding>> {
    installed()
        .lock()
        .ok()?
        .upgrade()
        .filter(|b| b.current(shell))
}
pub(crate) fn ui_completed(account: &str, key: &DraftKey) {
    if account != ACCOUNT || !enabled() {
        return;
    }
    if let Some(b) = installed().lock().ok().and_then(|b| b.upgrade())
        && b.configured()
        && *key == self::key(b.target())
    {
        b.ui_done.store(true, Ordering::Release);
    }
}
pub(crate) async fn durable(b: &FixtureBinding) -> Result<Value, String> {
    let mut rows = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        // Reuse the already initialized owner. A fresh Account would normalize
        // an actual live InFlight operation as if the process had restarted.
        let review = b
            .owner(&key(target))
            .review_local()
            .await
            .map_err(|e| format!("Private inventory: {e:?}"))?;
        rows.insert(
            if matches!(target, ComposerTarget::Thread) {
                "thread"
            } else {
                "channel"
            }
            .into(),
            crate::drafts::remote_review_fixture::inventory_json(&review),
        );
    }
    Ok(Value::Object(rows))
}
