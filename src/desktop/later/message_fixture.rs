//! Exact-gated metadata responses for the mounted Later route; never a transport.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    later::{LaterFilter, SavedItem, SavedKey, SavedPage},
    slack::{
        Error,
        models::{Channel, Message, MessagesListPage},
    },
    state::ChannelMessages,
};
pub(crate) const ACCOUNT: &str = "later-cold-message-disconnected";
pub(super) const IDS: [&str; 9] = [
    "CLATERA", "CLATERM", "CLATERE", "CLATERS", "CLATERB", "CLATERD", "CLATERQ", "CLATERF",
    "CLATERN",
];
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("later-cold-message-navigation" | "later-cold-message-navigation-narrow")
        )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_LATER_COLD_MESSAGE_CHECK").as_deref() == Ok("1")
        && s.core.active_account.as_deref() == Some(ACCOUNT)
        && s.core.active_team.as_deref() == Some("T1")
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(super) struct Visit {
    account: Option<String>,
    epoch: u64,
    pub(super) team: String,
}
impl Visit {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        enabled(s).then(|| Self {
            account: s.core.active_account.clone(),
            epoch: s.core.account_epoch,
            team: "T1".into(),
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        enabled(s)
            && s.core.active_account == self.account
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && s.core.later.matches_scope(self.epoch, &self.team)
    }
}
#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Success,
    Missing,
    Error,
}
impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Missing => "missing",
            Self::Error => "error",
        }
    }
}
struct Plan {
    kind: &'static str,
    channel: String,
    outcome: Outcome,
    hold: bool,
}
#[derive(Default)]
struct Ledger {
    plans: Vec<Plan>,
    requests: Vec<Value>,
    routes: Vec<Value>,
    held: Vec<(usize, tokio::sync::oneshot::Sender<()>)>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn ledger() -> &'static Mutex<Ledger> {
    LEDGER.get_or_init(Default::default)
}
pub(super) fn arm(kind: &'static str, channel: &str, outcome: Outcome, hold: bool) {
    ledger().lock().unwrap().plans.push(Plan {
        kind,
        channel: channel.into(),
        outcome,
        hold,
    });
}
async fn response(kind: &str, key: &SavedKey) -> (usize, Outcome) {
    let (id, outcome, rx) = {
        let mut l = ledger().lock().unwrap();
        let p = l
            .plans
            .iter()
            .position(|p| p.kind == kind && p.channel == key.item_id)
            .map(|i| l.plans.remove(i));
        let (outcome, hold) = p
            .map(|p| (p.outcome, p.hold))
            .unwrap_or((Outcome::Success, false));
        let id = l.requests.len();
        let rx = if hold {
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held.push((id, tx));
            Some(rx)
        } else {
            None
        };
        l.requests.push(json!({"ordinal":id,"kind":kind,"channel":key.item_id,"ts":key.ts,"outcome":outcome.label(),"held":hold,"released":!hold,"responseAccepted":null}));
        (id, outcome, rx)
    };
    if let Some(rx) = rx {
        let _ = rx.await;
    }
    (id, outcome)
}
pub(super) fn release(id: usize) -> Result<(), String> {
    let mut l = ledger().lock().unwrap();
    let i = l
        .held
        .iter()
        .position(|(n, _)| *n == id)
        .ok_or("No held response")?;
    let (_, tx) = l.held.remove(i);
    l.requests[id]["released"] = json!(true);
    tx.send(()).map_err(|_| "Response receiver dropped".into())
}
pub(super) fn accepted(id: usize, current: bool) {
    ledger().lock().unwrap().requests[id]["responseAccepted"] = json!(current);
}
pub(super) fn completed(s: &ShellState, key: &SavedKey) {
    if requested() {
        ledger().lock().unwrap().routes.push(json!({"channel":key.item_id,"requestedMessage":key.ts,"requestedReply":if key.item_id=="CLATERN"{None}else{Some(&key.ts)},"root":s.thread_root,"target":s.thread_target,"mainView":format!("{:?}",s.main_view),"requestGeneration":s.later_open_generation}));
    }
}
pub(super) fn routes() -> Vec<Value> {
    ledger().lock().unwrap().routes.clone()
}
pub(super) fn requests() -> Vec<Value> {
    ledger().lock().unwrap().requests.clone()
}
pub(super) fn outstanding() -> usize {
    let l = ledger().lock().unwrap();
    l.plans.len() + l.held.len()
}
pub(super) fn root(id: &str) -> String {
    format!(
        "1750000{}.000001",
        10 + IDS.iter().position(|c| *c == id).unwrap()
    )
}
pub(super) fn key(id: &str) -> SavedKey {
    SavedKey::message(
        id,
        format!(
            "1750000{}.000002",
            10 + IDS.iter().position(|c| *c == id).unwrap()
        ),
    )
}
pub(super) fn message(id: &str, reply: bool) -> Message {
    Message {
        ts: Some(if reply { key(id).ts } else { root(id) }),
        thread_ts: (reply && id != "CLATERN").then(|| root(id)),
        text: Some(format!(
            "{} {}",
            if reply && id == "CLATERN" {
                "Saved message"
            } else if reply {
                "Saved reply"
            } else {
                "Conversation root"
            },
            id
        )),
        user: Some("U2".into()),
        ..Default::default()
    }
}
pub(super) async fn messages(key: &SavedKey) -> (Option<usize>, Result<MessagesListPage, Error>) {
    let (id, outcome) = response("message", key).await;
    let result=match outcome {Outcome::Error=>Err(Error::Api("fixture_metadata_failed".into())),Outcome::Missing=>Ok(MessagesListPage::default()),Outcome::Success=>Ok(serde_json::from_value(json!({"messages_data":{(key.item_id.clone()):{"messages":[message(&key.item_id,false),message(&key.item_id,true)]}}})).unwrap())};
    (Some(id), result)
}
pub(super) async fn channels(key: &SavedKey) -> (Option<usize>, Result<Vec<Channel>, Error>) {
    let (id, outcome) = response("channel", key).await;
    let result=match outcome{Outcome::Error=>Err(Error::Api("fixture_conversation_failed".into())),Outcome::Missing=>Ok(vec![]),Outcome::Success=>Ok(vec![serde_json::from_value(json!({"id":key.item_id,"name":format!("saved-{}",key.item_id.to_lowercase()),"is_channel":true,"is_member":true})).unwrap()])};
    (Some(id), result)
}
pub(super) async fn history_barrier(key: &SavedKey) -> Option<usize> {
    let armed = ledger()
        .lock()
        .unwrap()
        .plans
        .iter()
        .any(|p| p.kind == "history" && p.channel == key.item_id);
    if armed {
        Some(response("history", key).await.0)
    } else {
        None
    }
}
pub(crate) fn apply(s: &mut ShellState) {
    *ledger().lock().unwrap() = Ledger::default();
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = Some(ACCOUNT.into());
    s.core.active_team = Some("T1".into());
    s.overlay = None;
    s.toast = None;
    s.close_thread();
    s.main_view = MainView::Later;
    s.surfaces.remove(&MainView::Later);
    s.core.later.ensure_scope(s.core.account_epoch, "T1");
    let generation = s.core.later.begin(LaterFilter::Saved, false);
    let items = IDS
        .iter()
        .map(|id| SavedItem {
            item_type: "message".into(),
            item_id: (*id).into(),
            ts: key(id).ts,
            ..Default::default()
        })
        .collect();
    s.core.later.apply_page(
        generation,
        SavedPage {
            saved_items: items,
            counts: super_platinum_core::later::SavedCounts {
                uncompleted_count: 9,
                ..Default::default()
            },
            ..Default::default()
        },
        false,
        None,
    );
    for id in IDS {
        let parent = message(id, false);
        let child = message(id, true);
        s.core.workspaces.get_mut("T1").unwrap().channels.remove(id);
        s.core.workspaces.get_mut("T1").unwrap().messages.insert(
            id.into(),
            ChannelMessages {
                messages: if id == "CLATERN" {
                    vec![parent.clone(), child.clone()]
                } else {
                    vec![parent.clone()]
                },
                loaded: true,
                ..Default::default()
            },
        );
        s.core.threads.insert(
            ("T1".into(), id.into(), root(id)),
            ChannelMessages {
                messages: vec![parent, child],
                loaded: true,
                ..Default::default()
            },
        );
    }
    s.refresh_from_core();
}
// Explicit disconnected filter response. Same list-generation transition as
// live load; no credentials, saved mutation, or fabricated transport receipt.
pub(crate) fn filter(state: Signal<ShellState>, filter: LaterFilter, append: bool) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let mut state = state;
    let mut s = state.write();
    let items = s
        .core
        .later
        .items
        .values()
        .filter(|i| i.filter() == filter)
        .cloned()
        .collect();
    let counts = s.core.later.counts.clone();
    let generation = s.core.later.begin(filter, append);
    s.core.later.apply_page(
        generation,
        SavedPage {
            saved_items: items,
            counts,
            ..Default::default()
        },
        append,
        None,
    );
    true
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    let mut saved = s
        .core
        .later
        .items
        .keys()
        .map(|k| json!({"channel":k.item_id,"ts":k.ts}))
        .collect::<Vec<_>>();
    saved.sort_by_key(|v| v["channel"].as_str().unwrap().to_owned());
    let mut hydrated = s
        .core
        .later
        .hydrated
        .keys()
        .map(|k| k.item_id.clone())
        .collect::<Vec<_>>();
    hydrated.sort();
    let known = IDS
        .into_iter()
        .filter(|id| s.core.workspaces["T1"].channels.contains_key(*id))
        .collect::<Vec<_>>();
    json!({"mainView":format!("{:?}",s.main_view),"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"channel":s.core.active_channel,"threadRoot":s.thread_root,"threadTarget":s.thread_target,"savedKeys":saved,"counts":{"saved":s.core.later.counts.uncompleted_count,"archived":s.core.later.counts.archived_count,"completed":s.core.later.counts.completed_count},"hydratedKeys":hydrated,"knownChannels":known,"pending":s.core.pending_scroll_to.as_ref().map(|(channel,target)|json!({"channel":channel,"target":format!("{:?}",target)})),"requestGeneration":s.later_open_generation,"channelGeneration":s.channel_generation,"threadGeneration":s.thread_generation,"listGeneration":s.core.later.generation,"shortcutGeneration":s.shortcut_generation,"overlay":format!("{:?}",s.overlay),"filter":format!("{:?}",s.core.later.filter)})
}
