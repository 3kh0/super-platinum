//! Explicit disconnected read adapter for reconnect acceptance. No transport.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{slack::models::Message, state::ChannelMessages};

pub(super) const ACCOUNT: &str = "reconnect-thread-disconnected";
pub(super) const CHANNEL: &str = "C2";
pub(super) const ROOT: &str = "1750000900.000001";
pub(super) const CACHED: &str = "1750000900.000002";
pub(super) const TARGET: &str = "1750000900.000003";
pub(super) const OTHER: &str = "1750000901.000001";
pub(super) const OTHER_REPLY: &str = "1750000901.000002";
pub(crate) fn enabled(s: &ShellState) -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("reconnect-thread-continuation" | "reconnect-thread-continuation-narrow")
        )
        && std::env::var("SUPER_PLATINUM_RECONNECT_THREAD_CHECK").as_deref() == Ok("1")
        && s.core.active_account.as_deref() == Some(ACCOUNT)
        && s.core.active_team.as_deref() == Some("T1")
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
#[derive(Default)]
struct Ledger {
    runs: Vec<Value>,
    requests: Vec<Value>,
    hold: Option<String>,
    held: Option<(usize, tokio::sync::oneshot::Sender<()>)>,
    fail_thread: bool,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn ledger() -> &'static Mutex<Ledger> {
    LEDGER.get_or_init(Default::default)
}
pub(super) fn runs() -> Vec<Value> {
    ledger().lock().unwrap().runs.clone()
}
pub(super) fn requests() -> Vec<Value> {
    ledger().lock().unwrap().requests.clone()
}
pub(super) fn arm(phase: &str) {
    ledger().lock().unwrap().hold = Some(phase.into());
}
pub(super) fn fail_next_thread() {
    ledger().lock().unwrap().fail_thread = true;
}
pub(super) fn begin(s: &ShellState) -> Option<usize> {
    if !enabled(s) {
        return None;
    }
    let mut l = ledger().lock().unwrap();
    let id = l.runs.len();
    l.runs
        .push(json!({"ordinal":id,"owner":owner(s),"capturedTarget":s.thread_target}));
    Some(id)
}
async fn request(
    before: Value,
    captured: Option<String>,
    run: Option<usize>,
    phase: &str,
    requested: Option<&str>,
) -> usize {
    let (id, rx) = {
        let mut l = ledger().lock().unwrap();
        let id = l.requests.len();
        let held = l.hold.as_deref() == Some(phase);
        let rx = if held {
            l.hold = None;
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held = Some((id, tx));
            Some(rx)
        } else {
            None
        };
        l.requests.push(json!({"ordinal":id,"run":run,"phase":phase,"before":before,"capturedTarget":captured,"requestedTarget":requested,"held":held,"released":!held,"accepted":null,"after":null}));
        (id, rx)
    };
    if let Some(rx) = rx {
        let _ = rx.await;
    }
    id
}
fn finish(id: usize, s: &ShellState, accepted: bool) {
    let mut l = ledger().lock().unwrap();
    l.requests[id]["accepted"] = json!(accepted);
    l.requests[id]["after"] = owner(s);
    l.requests[id]["currentTarget"] = json!(s.thread_target);
}
pub(super) fn release() -> Result<usize, String> {
    let mut l = ledger().lock().unwrap();
    let (id, tx) = l.held.take().ok_or("No held reconnect read")?;
    l.requests[id]["released"] = json!(true);
    tx.send(()).map_err(|_| "Held reconnect read closed")?;
    Ok(id)
}
pub(super) async fn boundary(
    state: Signal<ShellState>,
    run: Option<usize>,
    phase: &str,
) -> Option<usize> {
    let run = run?;
    let (before, captured) = {
        let s = state.peek();
        (owner(&s), s.thread_target.clone())
    };
    Some(request(before, captured, Some(run), phase, None).await)
}
pub(super) fn finish_boundary(id: Option<usize>, s: &ShellState, accepted: bool) {
    if let Some(id) = id {
        finish(id, s, accepted);
    }
}
pub(in crate::bootstrap) async fn thread_read(
    state: Signal<ShellState>,
    target: Option<&str>,
    reconnect: bool,
) -> usize {
    let run = reconnect
        .then(|| ledger().lock().unwrap().runs.len().checked_sub(1))
        .flatten();
    let (before, captured) = {
        let s = state.peek();
        (owner(&s), s.thread_target.clone())
    };
    request(before, captured, run, "thread", target).await
}
// Called only after the common thread loader's publication guard; no await here.
pub(in crate::bootstrap) fn finish_thread(
    id: usize,
    s: &mut ShellState,
    channel: &str,
    root: &str,
    accepted: bool,
) {
    finish(id, s, accepted);
    if !accepted {
        return;
    }
    let fail = {
        let mut l = ledger().lock().unwrap();
        let fail = l.fail_thread;
        l.fail_thread = false;
        l.requests[id]["outcome"] = json!(if fail { "error" } else { "success" });
        fail
    };
    s.thread_loading = false;
    if fail {
        s.thread_error = Some("Replies could not load. Please try again.".into());
    } else {
        s.thread_error = None;
        if root == ROOT {
            let messages = s
                .core
                .threads
                .get_mut(&("T1".into(), channel.into(), root.into()))
                .unwrap();
            messages.upsert(message(TARGET, ROOT, "Recovered exact reply"));
        }
    }
    s.refresh_from_core();
}
fn message(ts: &str, root: &str, text: &str) -> Message {
    Message {
        ts: Some(ts.into()),
        thread_ts: Some(root.into()),
        text: Some(text.into()),
        user: Some("U2".into()),
        ..Default::default()
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
    let a = message(ROOT, ROOT, "Reconnect conversation root");
    let cached = message(CACHED, ROOT, "Cached reply stays available");
    let b = message(OTHER, OTHER, "Newer conversation root");
    let reply = message(OTHER_REPLY, OTHER, "Newer thread reply");
    s.core.workspaces.get_mut("T1").unwrap().messages.insert(
        CHANNEL.into(),
        ChannelMessages {
            messages: vec![a.clone(), b.clone()],
            loaded: true,
            ..Default::default()
        },
    );
    for (root, messages) in [(ROOT, vec![a, cached]), (OTHER, vec![b, reply])] {
        s.core.threads.insert(
            ("T1".into(), CHANNEL.into(), root.into()),
            ChannelMessages {
                messages,
                loaded: true,
                ..Default::default()
            },
        );
    }
    reset(s, None);
}
// Independent failure scenario starts from the original cached root/reply bag.
// This is fixture setup, not a message deletion or a live history result.
pub(super) fn reset_failure(s: &mut ShellState) {
    s.core
        .threads
        .get_mut(&("T1".into(), CHANNEL.into(), ROOT.into()))
        .unwrap()
        .messages
        .retain(|m| m.ts.as_deref() != Some(TARGET));
    reset(s, Some(TARGET));
}
pub(super) fn reset(s: &mut ShellState, target: Option<&str>) {
    s.overlay = None;
    s.toast = None;
    s.main_view = MainView::Home;
    let index = s.channels.iter().position(|c| c.id == CHANNEL).unwrap();
    s.select_channel(index, crate::state::ChannelOpen::InSurface);
    s.begin_thread(ROOT.into(), target.map(str::to_owned));
    s.thread_loading = false;
    s.remember_surface();
    s.refresh_from_core();
}
pub(super) fn owner(s: &ShellState) -> Value {
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"view":format!("{:?}",s.main_view),"channel":s.core.active_channel,"channelGeneration":s.channel_generation,"root":s.thread_root,"threadGeneration":s.thread_generation,"requestGeneration":s.reconnect_generation,"shortcutGeneration":s.shortcut_generation,"overlay":format!("{:?}",s.overlay),"tab":format!("{:?}",s.channel_tab),"noSession":s.core.session.is_none(),"noTransport":s.core.transport.is_none()})
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    let cache=[ROOT,OTHER].into_iter().map(|root|json!({"root":root,"messages":s.core.threads[&("T1".into(),CHANNEL.into(),root.into())].messages.iter().map(|m|json!({"ts":m.ts,"text":m.text})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    json!({"owner":owner(s),"target":s.thread_target,"loading":s.thread_loading,"error":s.thread_error,"cache":cache})
}
