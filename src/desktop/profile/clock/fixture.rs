//! Exact opt-in synthetic clock; no system clock, session or transport changes.
use super::Face;
use crate::state::ShellState;
use dioxus::prelude::{ReadableExt, Signal};
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicI64, Ordering},
};

mod check;
pub(crate) use check::run;
const INITIAL: i64 = 1735801130;
static NOW: AtomicI64 = AtomicI64::new(INITIAL);
static EVIDENCE: Mutex<(Vec<Value>, Vec<Value>, Vec<Value>)> =
    Mutex::new((Vec::new(), Vec::new(), Vec::new()));

fn requested() -> bool {
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_PROFILE_TIME_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-pane"
                | "profile-pane-narrow"
                | "profile-pane-min-width"
                | "profile-pane-min-width-narrow"
                | "profile-hover-card"
                | "profile-hover-card-narrow")
        )
}
pub(super) fn enabled(s: &ShellState) -> bool {
    requested() && s.core.session.is_none() && s.core.transport.is_none()
}
pub(super) fn clock(s: &ShellState) -> Option<i64> {
    enabled(s).then(|| NOW.load(Ordering::SeqCst))
}
pub(super) fn tick(now: i64, before: &[Face], after: &[Face], notify: bool) {
    let mut e = EVIDENCE.lock().unwrap();
    let serial = e.0.len() + 1;
    let render_serial = e.2.len();
    if serial <= 2048 {
        e.0.push(json!({"serial":serial,"renderSerial":render_serial,"now":now,"before":before,"after":after,"notify":notify}));
    }
}
pub(super) fn render(now: i64, face: &Face) {
    let mut e = EVIDENCE.lock().unwrap();
    let serial = e.2.len() + 1;
    if serial <= 4096 {
        e.2.push(json!({"serial":serial,"now":now,"face":face}));
    }
}
fn advance(now: i64) {
    EVIDENCE.lock().unwrap().1.push(json!({"now":now}));
    NOW.store(now, Ordering::SeqCst);
}
fn evidence() -> Value {
    let e = EVIDENCE.lock().unwrap();
    json!({"ticks":e.0,"writes":e.1,"renders":e.2,"notifications":e.0.iter().filter(|v|v["notify"]==true).count()})
}
fn model(s: &ShellState) -> Value {
    let user = s
        .profile_user
        .as_deref()
        .or_else(|| s.profile_hover.as_ref().map(|h| h.user_id.as_str()));
    let member = s
        .core
        .active_team
        .as_ref()
        .and_then(|t| s.core.workspaces.get(t))
        .and_then(|w| user.and_then(|u| w.users.get(u)));
    let drafts = json!({"channel":s.core.drafts.channel,"thread":s.core.drafts.thread,
        "items":s.core.drafts.items.iter().map(|(key,value)|json!({"key":key,"value":value})).collect::<Vec<_>>()});
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,
        "user":user,"offset":member.and_then(|u|u.tz_offset),"now":NOW.load(Ordering::SeqCst),
        "profileGeneration":s.profile_generation,"hoverGeneration":s.profile_hover_generation,"profileUser":s.profile_user,"hoverUser":s.profile_hover.as_ref().map(|h|&h.user_id),
        "composer":s.core.composer,"threadComposer":s.core.thread_composer,"drafts":drafts,
        "mainAttachments":s.core.composer_attachments,"threadAttachments":s.core.thread_composer_attachments,
        "channel":s.core.active_channel,"channelGeneration":s.channel_generation})
}
fn expected(s: &ShellState) -> Value {
    let m = model(s);
    let offset = m["offset"].as_i64().map(|v| v as i32);
    let label =
        super_platinum_core::state::format_user_local_time_at(offset, NOW.load(Ordering::SeqCst));
    json!({"user":m["user"],"present":m["user"].is_string(),"label":label,"hover":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default().starts_with("profile-hover")})
}
async fn settled_ticks(state: Signal<ShellState>, now: i64, after: usize) -> Result<(), String> {
    for _ in 0..100 {
        if !enabled(&state.peek()) {
            return Err("Profile clock fixture acquired live context".into());
        }
        let e = evidence();
        if e["ticks"].as_array().unwrap().len() >= after + 2
            && e["ticks"]
                .as_array()
                .unwrap()
                .iter()
                .rev()
                .take(2)
                .filter(|v| v["now"] == now)
                .count()
                == 2
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual runtime clock polls did not settle".into())
}
