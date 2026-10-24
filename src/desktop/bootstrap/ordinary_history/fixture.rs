//! Closed, credential-free pages consumed by the production ordinary-history loop.
use super::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    CoreAppState,
    slack::models::{HistoryPage, Message},
    state::ChannelMessages,
};
static SOURCE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const ACCOUNT: &str = "ordinary-history-disconnected";
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("ordinary-history-cursor" | "ordinary-history-cursor-narrow")
        )
        && std::env::var("SUPER_PLATINUM_ORDINARY_HISTORY_CHECK").as_deref() == Ok("1")
}
fn core_enabled(c: &CoreAppState) -> bool {
    c.active_account.as_deref() == Some(ACCOUNT)
        && c.active_team.as_deref() == Some("T1")
        && c.session.is_none()
        && c.transport.is_none()
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && core_enabled(&s.core)
}
pub(super) struct Owner {
    account: Option<String>,
    epoch: u64,
    generation: u64,
}
impl Owner {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        enabled(s).then(|| Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            generation: SOURCE_GENERATION.load(std::sync::atomic::Ordering::SeqCst),
        })
    }
    pub(super) fn current(&self, c: &CoreAppState) -> bool {
        requested()
            && core_enabled(c)
            && self.account == c.active_account
            && self.epoch == c.account_epoch
            && self.generation == SOURCE_GENERATION.load(std::sync::atomic::Ordering::SeqCst)
    }
}
struct Plan {
    channel: String,
    result: Result<HistoryPage, String>,
    hold: bool,
}
#[derive(Default)]
struct Ledger {
    plans: Vec<Plan>,
    requests: Vec<Value>,
    held: Vec<(usize, tokio::sync::oneshot::Sender<()>)>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn ledger() -> &'static Mutex<Ledger> {
    LEDGER.get_or_init(Default::default)
}
pub(super) fn ts(i: usize) -> String {
    format!("1750001000.{i:06}")
}
pub(super) fn batch(
    channel: &str,
    range: std::ops::Range<usize>,
    cursor: Option<&str>,
    hold: bool,
) {
    let page=serde_json::from_value(json!({"messages":range.map(|i|json!({"ts":ts(i),"text":format!("History message {i}"),"user":if i%2==0 {"U2"}else{"U1"}})).collect::<Vec<_>>(),"has_more":false,"response_metadata":{"next_cursor":cursor}})).unwrap();
    ledger().lock().unwrap().plans.push(Plan {
        channel: channel.into(),
        result: Ok(page),
        hold,
    });
}
pub(super) fn error(channel: &str, code: &str) {
    ledger().lock().unwrap().plans.push(Plan {
        channel: channel.into(),
        result: Err(code.into()),
        hold: false,
    });
}
pub(super) struct Pending {
    pub(super) id: usize,
    rx: Option<tokio::sync::oneshot::Receiver<()>>,
    result: Result<HistoryPage, String>,
}
impl Pending {
    pub(super) async fn finish(self) -> Result<HistoryPage, Error> {
        if let Some(rx) = self.rx {
            let _ = rx.await;
        }
        self.result.map_err(Error::Api)
    }
}
pub(super) fn request(s: &ShellState, ticket: u64, args: HistoryArgs) -> Option<Pending> {
    if !enabled(s) {
        return None;
    }
    let mut l = ledger().lock().unwrap();
    let planned = l.plans.first().is_some_and(|p| p.channel == args.channel);
    let plan = if planned {
        l.plans.remove(0)
    } else {
        Plan {
            channel: args.channel.clone(),
            result: Err("unplanned_history_request".into()),
            hold: false,
        }
    };
    let id = l.requests.len();
    let rx = if plan.hold {
        let (tx, rx) = tokio::sync::oneshot::channel();
        l.held.push((id, tx));
        Some(rx)
    } else {
        None
    };
    let response = match &plan.result {
        Ok(p) => json!(p),
        Err(e) => json!({"error":e}),
    };
    l.requests.push(json!({"id":id,"ticket":ticket,"before":owner(s),"args":args_value(&args),"response":response,"planned":planned,"held":plan.hold,"released":!plan.hold,"accepted":null,"outcome":null}));
    Some(Pending {
        id,
        rx,
        result: plan.result,
    })
}
pub(super) fn response(id: Option<usize>, s: &ShellState, accepted: bool, outcome: &str) {
    let Some(id) = id else {
        return;
    };
    let mut l = ledger().lock().unwrap();
    l.requests[id]["accepted"] = json!(accepted);
    l.requests[id]["outcome"] = json!(outcome);
    l.requests[id]["after"] = snapshot(s);
}
pub(super) fn release(id: usize) -> Result<(), String> {
    let mut l = ledger().lock().unwrap();
    let i = l
        .held
        .iter()
        .position(|(n, _)| *n == id)
        .ok_or("Missing owned held response")?;
    let (_, tx) = l.held.remove(i);
    l.requests[id]["released"] = json!(true);
    tx.send(()).map_err(|_| "Held request retired".into())
}
pub(super) fn retire_source() {
    SOURCE_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}
pub(super) fn requests() -> Vec<Value> {
    ledger().lock().unwrap().requests.clone()
}
pub(super) fn drained() -> bool {
    let l = ledger().lock().unwrap();
    l.plans.is_empty()
        && l.held.is_empty()
        && l.requests
            .iter()
            .all(|r| !r["outcome"].is_null() && r["planned"] == true)
}
pub(crate) fn apply(s: &mut ShellState) {
    *ledger().lock().unwrap() = Ledger::default();
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = Some(ACCOUNT.into());
    s.core.active_team = Some("T1".into());
    reset(s, "C1", 30..54);
    reset(s, "C2", 0..0);
    s.core.pending_scroll_to = Some((
        "C2".into(),
        super_platinum_core::domain::PendingScrollTarget::Message(ts(53)),
    ));
}

pub(super) fn reset(s: &mut ShellState, channel: &str, range: std::ops::Range<usize>) {
    s.ordinary_history
        .entries
        .remove(&("T1".into(), channel.into()));
    s.close_thread();
    s.close_profile();
    s.overlay = None;
    s.toast = None;
    s.main_view = crate::state::MainView::Home;
    let messages = range
        .map(|i| Message {
            ts: Some(ts(i)),
            text: Some(format!("History message {i}")),
            user: Some(if i % 2 == 0 { "U2" } else { "U1" }.into()),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let loaded = !messages.is_empty();
    s.core.workspaces.get_mut("T1").unwrap().messages.insert(
        channel.into(),
        ChannelMessages {
            messages,
            loaded,
            last_read: Some(ts(1)),
            ..Default::default()
        },
    );
    select(s, channel);
}
pub(super) fn select(s: &mut ShellState, channel: &str) {
    let index = s.channels.iter().position(|c| c.id == channel).unwrap();
    s.select_channel(index, crate::state::ChannelOpen::InSurface);
    s.core.pending_scroll_to = Some((
        channel.into(),
        super_platinum_core::domain::PendingScrollTarget::Latest,
    ));
    crate::read_preferences::opening::cancel(s);
    s.unread_anchor = None;
    s.stick_to_bottom = true;
    s.loading_older = false;
    s.core.composer =
        super_platinum_core::ComposerState::with_text(format!("Unsent draft {channel}"));
    s.refresh_from_core();
    s.reset_timeline_window();
}
fn args_value(a: &HistoryArgs) -> Value {
    json!({"channel":a.channel,"cursor":a.cursor,"oldest":a.oldest,"latest":a.latest,"inclusive":a.inclusive,"limit":a.limit})
}
pub(super) fn owner(s: &ShellState) -> Value {
    json!({"sourceGeneration":SOURCE_GENERATION.load(std::sync::atomic::Ordering::SeqCst),"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"channel":s.core.active_channel,"scope":s.timeline_scope(),"noSession":s.core.session.is_none(),"noTransport":s.core.transport.is_none()})
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    let bags=["C1","C2"].into_iter().map(|channel|{
        let b=s.core.workspaces["T1"].messages.get(channel);
        let e=s.ordinary_history.entries.get(&("T1".into(),channel.into()));
        json!({"channel":channel,"messages":b.map(|b|b.messages.iter().map(|m|json!({"ts":m.ts,"text":m.text})).collect::<Vec<_>>()).unwrap_or_default(),"loaded":b.map(|b|b.loaded),"historyFailed":b.map(|b|b.history_failed),"historyRefreshing":b.map(|b|b.history_refreshing),"hasMoreOlder":b.map(|b|b.has_more_older),"lastRead":b.and_then(|b|b.last_read.clone()),"continuation":e.map(|e|json!({"ticket":e.ticket,"current":e.owner.current(&s.core),"loading":e.loading,"error":e.error,"paused":e.paused,"restartRequired":e.restart_required,"initial":e.initial,"request":args_value(e.pages.request()),"seen":e.pages.seen(),"restarted":e.pages.restarted(),"more":e.pages.has_more()}))})
    }).collect::<Vec<_>>();
    json!({"owner":owner(s),"bags":bags,"draft":s.core.composer,"loadingOlder":s.loading_older,"pending":s.core.pending_scroll_to.as_ref().map(|(c,t)|json!({"channel":c,"target":format!("{t:?}")}))})
}
