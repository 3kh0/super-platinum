//! Closed automatic member freshness: real Connection ACKs and conditional IO.
//! Only initial Boot/Init seeds source. Later rows use production adoption.
pub(crate) use super::member_recovery::{DRAFT, LABEL, MEMBER, blocks, pending, release, requests};
use super::*;
use crate::directory_member_fetches::automatic;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::{events::RtEvent, realtime::Connection};
use tokio::sync::mpsc;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_CHECK";
pub(crate) const EDITED: &str = "Native unsaved description \u{4e2d}\u{1f642} with ";
pub(crate) const RENAMED: &str = "Renamed Cafe\u{301} member";

pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-member-freshness" | "profile-member-freshness-narrow")
        )
}
pub(crate) fn enabled() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && super::member_recovery::enabled()
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_FRESHNESS_CASE")
        .unwrap_or_else(|_| "renamed".into())
}
pub(crate) fn install(shell: &mut ShellState) {
    super::member_recovery::install(shell);
}
struct Socket {
    account: String,
    generation: u64,
    connection: Connection,
    receiver: mpsc::Receiver<String>,
}
struct Frame {
    socket: usize,
    request: Value,
    reply: Option<Value>,
    accepted: Option<bool>,
}
#[derive(Default)]
struct Sockets {
    sockets: Vec<Socket>,
    frames: Vec<Frame>,
}
fn sockets() -> &'static Mutex<Sockets> {
    static SOCKETS: OnceLock<Mutex<Sockets>> = OnceLock::new();
    SOCKETS.get_or_init(Mutex::default)
}
fn drain(book: &mut Sockets) -> Result<(), String> {
    for (socket, held) in book.sockets.iter_mut().enumerate() {
        while let Ok(raw) = held.receiver.try_recv() {
            let request: Value =
                serde_json::from_str(&raw).map_err(|_| "Invalid closed socket frame")?;
            if request["type"] != "subscription_update"
                || request["ids"] != json!([MEMBER])
                || !matches!(
                    request["subtype"].as_str(),
                    Some("user_change_subscribe_request" | "user_change_unsubscribe_request")
                )
                || request["id"].as_u64().is_none()
            {
                return Err("Unexpected automatic socket operation".into());
            }
            book.frames.push(Frame {
                socket,
                request,
                reply: None,
                accepted: None,
            });
        }
    }
    Ok(())
}
pub(crate) fn frames() -> Result<Vec<Value>, String> {
    let mut book = sockets().lock().unwrap();
    drain(&mut book)?;
    Ok(book
        .frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let socket = &book.sockets[frame.socket];
            json!({"index":index,"socket":frame.socket,"account":socket.account,
            "generation":socket.generation,"request":frame.request,"reply":frame.reply,
            "accepted":frame.accepted})
        })
        .collect())
}
pub(crate) fn acknowledge(index: usize, accepted: bool) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed freshness fixture required".into());
    }
    let mut book = sockets().lock().unwrap();
    drain(&mut book)?;
    let frame = book
        .frames
        .get(index)
        .ok_or("Missing original subscription")?;
    if frame.reply.is_some() {
        return Err("Subscription already answered".into());
    }
    let reply = if accepted {
        json!({"reply_to":frame.request["id"],"ok":true})
    } else {
        json!({"reply_to":frame.request["id"],"ok":false,
            "error":{"code":42,"msg":"closed fixture rejection"}})
    };
    let routed = book.sockets[frame.socket]
        .connection
        .receive_member_subscription_reply(&reply);
    let frame = &mut book.frames[index];
    frame.reply = Some(reply.clone());
    frame.accepted = Some(routed);
    Ok(json!({"index":index,"reply":reply,"routed":routed}))
}
pub(crate) fn connect(shell: &mut ShellState) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed freshness fixture required".into());
    }
    let team = shell
        .core
        .active_team
        .clone()
        .ok_or("Missing fixture team")?;
    let account = shell
        .core
        .active_account
        .clone()
        .ok_or("Missing fixture account")?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing fixture workspace")?;
    let before = workspace.rt_generation;
    let generation = before
        .checked_add(1)
        .ok_or("Fixture socket generation exhausted")?;
    let (tx, receiver) = mpsc::channel(32);
    let connection = Connection::from_sender(tx);
    workspace.rt_generation = generation;
    workspace.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
    // Rebind actual assignment/source witnesses to this accepted connection.
    // Initial boot has settled, so this carries no unprovided network branch.
    let _work = crate::app_home::channel_fetches::connected(shell, &team, generation)
        .ok_or("Fixture channel owner refused accepted connection")?;
    crate::app_home::cfm_lifecycle::connected(shell, &team, generation);
    automatic::connected(shell, &team, generation);
    let mut book = sockets().lock().unwrap();
    book.sockets.push(Socket {
        account,
        generation,
        connection,
        receiver,
    });
    Ok(json!({"before":before,"after":generation,"socket":book.sockets.len()-1}))
}
pub(crate) fn response(team: &str, name: &str, updated: u64, deleted: bool) -> Value {
    json!({"ok":true,"results":[{"id":MEMBER,"team_id":team,
        "name":"username-must-not-win","is_stranger":false,"deleted":deleted,
        "real_name":"Top fallback","profile":{"display_name":name,"real_name":"Profile fallback"},
        "updated":updated}],"can_interact":{(MEMBER):true}})
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !enabled() {
        return Err("Closed freshness fixture required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing freshness owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let row = response(&owner.team, LABEL, 101, false)["results"][0].clone();
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Member freshness"},
        "self":user,"users":[user,row],"no_login":false,"noSocket":false,
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Member freshness"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    connect(&mut state.write())?;
    automatic::resume(state, &owner.team);
    Ok(())
}
pub(crate) fn changed(shell: &mut ShellState, name: &str, updated: u64) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed freshness fixture required".into());
    }
    let team = shell
        .core
        .active_team
        .clone()
        .ok_or("Missing fixture team")?;
    let generation = shell.core.workspaces[&team].rt_generation;
    let event = RtEvent::UserChanged(
        serde_json::from_value(response(&team, name, updated, false)["results"][0].clone())
            .map_err(|_| "Invalid fixture user")?,
    );
    let before = snapshot(shell);
    crate::realtime::apply_in_view(&mut shell.core, &team, generation, event.clone(), false);
    automatic::event(shell, &team, generation, &event);
    shell.refresh_from_core();
    Ok(json!({"before":before,"after":snapshot(shell),"acceptedGeneration":generation}))
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let mut out = super::member_known::snapshot(shell);
    let team = shell.core.active_team.as_deref().unwrap_or("");
    out["automatic"] = automatic::fixture_status(shell, team);
    out["socketGeneration"] = json!(shell.core.workspaces.get(team).map(|w| w.rt_generation));
    out["subscriptions"] = frames()
        .map(Value::Array)
        .unwrap_or_else(|error| json!({"error":error}));
    out["networkCalls"] = json!(0);
    out["opening"] = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .map(|d| {
            json!({"identity":d.rich_opening,"reading":d.rich_reading,"mounted":d.rich.is_some(),"lease":d.rich_recovery.is_some(),
            "visit":d.visit,"serial":d.serial})
        })
        .unwrap_or(Value::Null);
    out
}
pub(crate) fn replace_account(state: Signal<ShellState>) -> Value {
    let before = snapshot(&state.peek());
    let replaced = super::member_recovery::replace_account(state);
    json!({"before":before,"after":snapshot(&state.peek()),"actualInstallLoaded":replaced["actualInstallLoaded"]})
}
