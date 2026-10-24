//! Controlled history responses for an explicitly disconnected mounted fixture.
use crate::state::{MainView, ShellState};
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    CoreAppState,
    domain::PendingScrollTarget,
    slack::{
        Error, api,
        models::{HistoryPage, Message},
    },
    state::ChannelMessages,
};

pub(super) const ACCOUNT: &str = "reconnect-unread-disconnected";
pub(super) const BOUNDARY: &str = "1750001000.000010";
pub(super) const CACHED: &str = "1750001000.000001";
pub(super) const FIRST: &str = "1750001000.000011";
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("reconnect-unread-opening" | "reconnect-unread-opening-narrow")
        )
        && std::env::var("SUPER_PLATINUM_RECONNECT_UNREAD_CHECK").as_deref() == Ok("1")
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && core_enabled(&s.core)
}
fn core_enabled(core: &CoreAppState) -> bool {
    core.active_account.as_deref() == Some(ACCOUNT)
        && core.active_team.as_deref() == Some("T1")
        && core.session.is_none()
        && core.transport.is_none()
}
pub(crate) struct Owner {
    account: Option<String>,
    epoch: u64,
    team: Option<String>,
}
impl Owner {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        enabled(s).then(|| Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: s.core.active_team.clone(),
        })
    }
    pub(crate) fn current(&self, core: &CoreAppState) -> bool {
        requested()
            && core_enabled(core)
            && core.active_account == self.account
            && core.account_epoch == self.epoch
            && core.active_team == self.team
    }
}
enum Outcome {
    Page(HistoryPage),
    Error,
}
struct Plan {
    channel: String,
    outcome: Outcome,
    hold: bool,
}
#[derive(Default)]
struct Ledger {
    plans: Vec<Plan>,
    openings: Vec<Value>,
    requests: Vec<Value>,
    events: Vec<Value>,
    held: Vec<(usize, tokio::sync::oneshot::Sender<()>)>,
    started: Vec<std::time::Instant>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn ledger() -> &'static Mutex<Ledger> {
    LEDGER.get_or_init(Default::default)
}
pub(super) fn page(
    channel: &str,
    ts: &str,
    text: &str,
    more: bool,
    cursor: Option<&str>,
    hold: bool,
) {
    let value = json!({"messages":[{"ts":ts,"text":text,"user":"U2"}],"has_more":more,"response_metadata":{"next_cursor":cursor}});
    ledger().lock().unwrap().plans.push(Plan {
        channel: channel.into(),
        outcome: Outcome::Page(serde_json::from_value(value).unwrap()),
        hold,
    });
}
pub(super) fn batch(channel: &str, cursor: Option<&str>, hold: bool) {
    let value = json!({"messages":(30..=53).map(|i|json!({"ts":format!("1750001000.{i:06}"),"text":format!("Newer unread {i}"),"user":"U2"})).collect::<Vec<_>>(),"has_more":true,"response_metadata":{"next_cursor":cursor}});
    ledger().lock().unwrap().plans.push(Plan {
        channel: channel.into(),
        outcome: Outcome::Page(serde_json::from_value(value).unwrap()),
        hold,
    });
}
pub(super) fn error(channel: &str) {
    ledger().lock().unwrap().plans.push(Plan {
        channel: channel.into(),
        outcome: Outcome::Error,
        hold: false,
    });
}
pub(super) fn begin(s: &ShellState, reconnect: bool) -> Option<usize> {
    if !enabled(s) {
        return None;
    }
    let mut l = ledger().lock().unwrap();
    let id = l.openings.len();
    l.openings
        .push(json!({"id":id,"reconnect":reconnect,"before":snapshot(s)}));
    Some(id)
}
pub(super) struct Pending {
    pub(super) id: usize,
    rx: Option<tokio::sync::oneshot::Receiver<()>>,
    outcome: Outcome,
}
impl Pending {
    pub(super) async fn finish(self) -> Result<HistoryPage, Error> {
        if let Some(rx) = self.rx {
            let _ = rx.await;
        }
        match self.outcome {
            Outcome::Page(p) => Ok(p),
            Outcome::Error => Err(Error::Api("fixture_history_failed".into())),
        }
    }
}
pub(super) fn request(opening: usize, args: api::HistoryArgs) -> Pending {
    let mut l = ledger().lock().unwrap();
    let planned = l.plans.first().is_some_and(|p| p.channel == args.channel);
    let plan = if planned {
        l.plans.remove(0)
    } else {
        Plan {
            channel: args.channel.clone(),
            outcome: Outcome::Error,
            hold: false,
        }
    };
    let id = l.requests.len();
    let response = match &plan.outcome {
        Outcome::Page(p) => json!(p),
        Outcome::Error => Value::Null,
    };
    let rx = if plan.hold {
        let (tx, rx) = tokio::sync::oneshot::channel();
        l.held.push((id, tx));
        Some(rx)
    } else {
        None
    };
    l.requests.push(json!({"id":id,"opening":opening,"args":{"channel":args.channel,"oldest":args.oldest,"latest":args.latest,"cursor":args.cursor,"limit":args.limit,"inclusive":args.inclusive},"response":response,"planned":planned,"held":plan.hold,"released":!plan.hold,"accepted":null,"outcome":null,"elapsedMs":null}));
    l.started.push(std::time::Instant::now());
    Pending {
        id,
        rx,
        outcome: plan.outcome,
    }
}
pub(super) fn response(id: Option<usize>, s: &ShellState, accepted: bool, outcome: &str) {
    let Some(id) = id else {
        return;
    };
    let mut l = ledger().lock().unwrap();
    let elapsed = l.started[id].elapsed().as_millis() as u64;
    l.requests[id]["accepted"] = json!(accepted);
    l.requests[id]["outcome"] = json!(outcome);
    l.requests[id]["elapsedMs"] = json!(elapsed);
    l.requests[id]["afterOwner"] = owner(s);
    if outcome == "timeout" {
        l.held.retain(|(held, _)| *held != id);
        l.requests[id]["canceled"] = json!(true);
    }
}
pub(super) fn event(opening: Option<usize>, kind: &str, s: &ShellState, detail: Value) {
    let Some(id) = opening else {
        return;
    };
    ledger()
        .lock()
        .unwrap()
        .events
        .push(json!({"opening":id,"kind":kind,"state":snapshot(s),"detail":detail}));
}
pub(super) fn release(id: usize) -> Result<(), String> {
    let mut l = ledger().lock().unwrap();
    let i = l
        .held
        .iter()
        .position(|(n, _)| *n == id)
        .ok_or("No owned held history response")?;
    let (_, tx) = l.held.remove(i);
    l.requests[id]["released"] = json!(true);
    tx.send(())
        .map_err(|_| "Held history response retired".into())
}
pub(super) fn requests() -> Vec<Value> {
    ledger().lock().unwrap().requests.clone()
}
pub(super) fn events() -> Vec<Value> {
    ledger().lock().unwrap().events.clone()
}
pub(super) fn openings() -> Vec<Value> {
    ledger().lock().unwrap().openings.clone()
}
pub(super) fn drained() -> bool {
    let l = ledger().lock().unwrap();
    l.plans.is_empty() && l.held.is_empty() && l.requests.iter().all(|r| !r["outcome"].is_null())
}
pub(crate) fn apply(s: &mut ShellState) {
    *ledger().lock().unwrap() = Ledger::default();
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = Some(ACCOUNT.into());
    s.core.active_team = Some("T1".into());
    reset(s, "C1");
    reset(s, "C2");
}
pub(super) fn reset(s: &mut ShellState, channel: &str) {
    s.close_thread();
    s.close_profile();
    s.overlay = None;
    s.toast = None;
    s.main_view = MainView::Home;
    s.core.workspaces.get_mut("T1").unwrap().messages.insert(
        channel.into(),
        ChannelMessages {
            messages: vec![Message {
                ts: Some(CACHED.into()),
                text: Some("Cached before unread boundary".into()),
                user: Some("U2".into()),
                ..Default::default()
            }],
            loaded: true,
            last_read: Some(BOUNDARY.into()),
            latest: Some(CACHED.into()),
            unread_count: 1,
            ..Default::default()
        },
    );
    reopen(s, channel);
}
pub(super) fn reopen(s: &mut ShellState, channel: &str) {
    let index = s.channels.iter().position(|c| c.id == channel).unwrap();
    s.select_channel(index, crate::state::ChannelOpen::InSurface);
    s.core.pending_scroll_to = Some((
        channel.into(),
        PendingScrollTarget::FirstUnreadAfter(BOUNDARY.into()),
    ));
    s.read_presentation.unread_loading = Some((s.timeline_scope(), BOUNDARY.into()));
    s.unread_anchor = Some((channel.into(), BOUNDARY.into()));
    s.stick_to_bottom = false;
    s.refresh_from_core();
    s.reset_timeline_window();
}
pub(super) fn owner(s: &ShellState) -> Value {
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"channel":s.core.active_channel,"scope":s.timeline_scope(),"requestGeneration":s.reconnect_generation,"view":format!("{:?}",s.main_view),"overlay":format!("{:?}",s.overlay),"shortcutGeneration":s.shortcut_generation,"noSession":s.core.session.is_none(),"noTransport":s.core.transport.is_none()})
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    let cache=["C2","C1"].into_iter().map(|id|{
        let bag=s.core.workspaces["T1"].messages.get(id);
        json!({"channel":id,"messages":bag.map(|b|b.messages.iter().map(|m|json!({"ts":m.ts,"text":m.text})).collect::<Vec<_>>()).unwrap_or_default(),"loaded":bag.map(|b|b.loaded),"historyFailed":bag.map(|b|b.history_failed),"historyRefreshing":bag.map(|b|b.history_refreshing),"hasMoreOlder":bag.map(|b|b.has_more_older),"latest":bag.and_then(|b|b.latest.clone()),"lastRead":bag.and_then(|b|b.last_read.clone())})
    }).collect::<Vec<_>>();
    json!({"owner":owner(s),"pending":s.core.pending_scroll_to.as_ref().map(|(c,t)|json!({"channel":c,"target":format!("{t:?}")})),"fetching":s.read_presentation.unread_fetching,"loading":s.read_presentation.unread_loading,"paused":s.read_presentation.unread_paused,"continuation":s.read_presentation.unread_continuation.as_ref().map(|c|json!({"scope":c.scope,"boundary":c.boundary,"cursor":c.pages.cursor,"latest":c.pages.latest,"ownerCurrent":c.owner.current(&s.core)})),"cache":cache})
}
