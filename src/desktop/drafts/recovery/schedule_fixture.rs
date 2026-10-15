//! Explicit synthetic Schedule recovery; only HTTP execution is substituted.
use super::*;
use crate::drafts::{Account, Owner};
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    ComposerState,
    config::WorkspaceSession,
    slack::{Error, SlackClient, client::RequestBody},
};
#[path = "schedule_fixture/interleave.rs"]
mod interleave;
#[path = "schedule_fixture/isolation.rs"]
mod isolation;
#[path = "schedule_fixture/seed.rs"]
mod seed;
pub(crate) use interleave::{interleave, projection, restore_storage, storage_fault};
pub(crate) use isolation::wait_runner;
pub(crate) use seed::install;
pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000111";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const POST_AT: i64 = 500;
pub(crate) const REQUESTED: &str =
    "Frozen schedule \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const CURRENT: &str =
    "Retained local draft \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const NEWER: &str =
    "Newer local typing \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
pub(crate) const SCENARIOS: &[&str] = &[
    "positive-channel",
    "positive-thread",
    "absent-channel",
    "changed-payload-channel",
    "cooldown-channel",
    "typing-read-thread",
    "typing-commit-thread",
    "owner-rotation-channel",
    "writer-replacement-channel",
    "queue-reset-channel",
    "queue-newer-channel",
    "storage-retry-channel",
    "pending-check-channel",
    "unmount-thread",
    "successor-channel",
    "reenroll-channel",
];
pub(crate) fn named() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("draft-schedule-recovery" | "draft-schedule-recovery-narrow")
    )
}
pub(crate) fn enabled() -> bool {
    named() && std::env::var("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_CHECK").as_deref() == Ok("1")
}
pub(crate) fn key(target: ComposerTarget) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: (target == ComposerTarget::Thread).then(|| ROOT.into()),
    }
}
pub(crate) fn local(text: &str, target: ComposerTarget) -> LocalDraft {
    let mut composer = ComposerState::with_text(text);
    composer.reply_broadcast = target == ComposerTarget::Thread;
    LocalDraft {
        composer,
        attachments: vec![],
    }
}
pub(crate) fn frozen(key: &DraftKey) -> Snapshot {
    let mut destination = json!({"channel_id":key.channel});
    if let Some(root) = &key.thread {
        destination["thread_ts"] = json!(root);
        destination["broadcast"] = json!(true);
    }
    Snapshot {
        blocks: vec![json!({"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[{"type":"text","text":REQUESTED,"style":{"bold":true}}]},
            {"type":"rich_text_list","style":"bullet","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Frozen checklist item","style":{"italic":true}}]}]}
        ]})],
        file_ids: vec![],
        destinations: vec![destination],
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
        s.core
            .drafts
            .items
            .insert(key(target), local(CURRENT, target));
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
    pub operations: BTreeMap<DraftKey, Operation>,
    pub withheld: Vec<Operation>,
    pub requests: Mutex<Vec<Value>>,
    pub routed: AtomicU64,
    pub refused: AtomicU64,
    pub ui_done: AtomicBool,
    cooldown: Mutex<u64>,
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
    pub fn operation(&self) -> &Operation {
        &self.operations[&key(self.target())]
    }
    pub fn owner(&self, key: &DraftKey) -> Owner {
        self.account.owner(key.clone())
    }
    fn configured(&self) -> bool {
        enabled()
            && std::env::var_os("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_DB")
                .is_some_and(|p| PathBuf::from(p) == self.path)
            && std::env::var("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_NONCE").as_deref()
                == Ok(self.nonce.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_SCENARIO").as_deref()
                == Ok(self.scenario.as_str())
            && std::env::var("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_PHASE").as_deref()
                == Ok(self.phase.as_str())
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.configured()
            && s.core.session.is_none()
            && s.core.transport.is_none()
            && s.core.cache.is_none()
            && s.core.active_account.as_deref() == Some(ACCOUNT)
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_deref() == Some("T1")
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
        if !matches!(stage, "read" | "commit" | "project") {
            return Err("Unknown Schedule gate".into());
        }
        let mut g = self.gate.lock().map_err(|_| "Gate unavailable")?;
        if g.armed.is_some() {
            return Err("Gate already armed".into());
        }
        self.ui_done.store(false, Ordering::Release);
        *g = Gate {
            armed: Some(stage),
            ..Default::default()
        };
        Ok(())
    }
    pub fn reset_ui(&self) {
        self.ui_done.store(false, Ordering::Release);
    }
    pub async fn wait_reached(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.gate.lock().unwrap().reached {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Schedule gate not reached".into())
    }
    pub async fn wait_ui(&self) -> Result<(), String> {
        for _ in 0..250 {
            if self.ui_done.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Err("Schedule UI did not finish".into())
    }
    pub fn release(&self) {
        self.gate.lock().unwrap().released = true;
    }
    async fn checkpoint(&self, stage: &str) {
        {
            let mut g = self.gate.lock().unwrap();
            if g.armed != Some(stage) {
                return;
            }
            g.reached = true;
        }
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
        if !self.configured() || self.phase != "exercise" || op != self.operation() {
            return Readback::blocked(network::Block::Scope);
        }
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U00000001".into(),
            enterprise_id: None,
            name: "Isolated Schedule".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-not-a-token".into(),
        };
        let raw = if self.scenario == "absent-channel" {
            json!({"ok":true,"draft":null})
        } else {
            let mut raw = positive(op);
            if self.scenario == "changed-payload-channel" {
                raw["draft"]["blocks"][0]["elements"][0]["elements"][0]["text"] =
                    json!("Changed remote payload");
            }
            raw
        };
        let limited = self.scenario == "cooldown-channel";
        let result=network::check_schedule_with(ACCOUNT,&SlackClient::default(),&workspace,&self.cooldown,op,now_ms,|request|async move{
            let RequestBody::Form(fields)=&request.body else{return Err(Error::Transport("Unexpected fixture request body".into()));};
            let fields:Vec<_>=fields.iter().filter(|(name,_)|name!="token").cloned().collect();
            let admitted=durable(self).await.map_err(|_|Error::Transport("Fixture admission inventory unavailable".into()))?;
            {let mut ledger=self.requests.lock().unwrap();let sequence=ledger.len()+1;
                ledger.push(json!({"sequence":sequence,"method":request.method,"url":request.url,"fields":fields,"retrySafe":request.retry_safe(),"operation":op,"admitted":admitted,"nowMs":now_ms,"delivered":false,"rawResponse":null,"retryAfterSeconds":if limited{json!(120)}else{Value::Null}}));}
            self.checkpoint("read").await;
            if let Some(row)=self.requests.lock().unwrap().last_mut(){row["delivered"]=json!(true);if !limited{row["rawResponse"]=raw.clone();}}
            if limited{Err(Error::RateLimited{retry_after_secs:Some(120)})}else{Ok(raw)}
        }).await;
        if let Some(row) = self.requests.lock().unwrap().last_mut() {
            row["result"] = json!({"requested":result.requested,"confirmed":matches!(&result.outcome,Some(Outcome::Scheduled{..})),"cooldownUntilMs":result.cooldown_until_ms,"issue":result.issue.map(|issue|format!("{issue:?}")),"hasMore":result.has_more,"raw":result.raw});
        }
        result
    }
}
pub(crate) fn positive(op: &Operation) -> Value {
    let snapshot = op.snapshot.as_ref().unwrap();
    json!({"ok":true,"draft":{"id":op.base.as_ref().unwrap().id,"client_msg_id":op.client_msg_id,"last_updated_ts":"12.000001","team_id":"T1","user_id":"U00000001","is_deleted":false,"is_sent":false,"date_created":100,"date_scheduled":POST_AT,"is_from_composer":true,"blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations}})
}
pub(crate) fn binding(s: &ShellState) -> Option<Arc<FixtureBinding>> {
    installed().lock().ok()?.upgrade().filter(|b| b.current(s))
}
pub(crate) async fn checkpoint(scope: &Scope, stage: &str) {
    if !enabled() || scope.account_id != ACCOUNT {
        return;
    }
    let b = installed().lock().ok().and_then(|b| b.upgrade());
    if let Some(b) = b
        && b.configured()
        && scope.key == key(b.target())
    {
        b.checkpoint(stage).await;
    }
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
pub(crate) async fn durable(b: &FixtureBinding) -> Result<Value, String> {
    let mut rows = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let review = b
            .owner(&key(target))
            .review_local()
            .await
            .map_err(|e| format!("Private inventory: {e:?}"))?;
        rows.insert(
            if target == ComposerTarget::Channel {
                "channel"
            } else {
                "thread"
            }
            .into(),
            crate::drafts::remote_review_fixture::inventory_json(&review),
        );
    }
    let mut withheld = Vec::new();
    for op in &b.withheld {
        let review = b
            .owner(&op.scope.key)
            .review_local()
            .await
            .map_err(|e| format!("Withheld inventory: {e:?}"))?;
        withheld.push(crate::drafts::remote_review_fixture::inventory_json(
            &review,
        ));
    }
    rows.insert("withheld".into(), json!(withheld));
    Ok(Value::Object(rows))
}
