//! Exact-gated disconnected read-admission ledger; no request executes.
use crate::state::ShellState;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody, models::Message},
    state::{ChannelMessages, ReadMode, ReadPreferences},
};
pub(crate) const ACCOUNT: &str = "read-navigation-disconnected";
pub(crate) const STAGES: &[&str] = &[
    "left-off",
    "newest-read",
    "newest-unseen",
    "divider",
    "ghost",
    "loading",
    "saved-offset",
    "explicit-target",
    "cold-boundary",
    "manual-hold",
];
#[derive(Default)]
struct Ledger {
    armed: Option<String>,
    attempts: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_READ_NAVIGATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("read-navigation" | "read-navigation-narrow")
        )
}
pub(crate) fn directory() -> Result<(PathBuf, String), String> {
    if !enabled() {
        return Err("Read-navigation opt-in missing".into());
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_READ_NAVIGATION_DIR")
            .ok_or("Private directory missing")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_READ_NAVIGATION_NONCE").map_err(|_| "Nonce missing")?;
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("read-navigation-"))
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("Invalid user-link fixture configuration".into());
    }
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "Private directory missing")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Invalid private directory".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Reparse directory refused".into());
        }
    }
    Ok((path, nonce))
}

pub(crate) fn owner(s: &ShellState) -> bool {
    enabled()
        && s.core.active_account.as_deref() == Some(ACCOUNT)
        && s.core.active_team.as_deref() == Some("T1")
        && s.core.active_channel.as_deref() == Some("C2")
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(crate) fn install(s: &mut ShellState) {
    s.core.session = None;
    s.core.transport = None;
    s.core.active_account = Some(ACCOUNT.into());
    s.overlay = None;
    s.toast = None;
    s.close_thread();
    s.close_profile();
    reset(s, ReadMode::LeftOffAndRead);
    let _ = LEDGER.set(Mutex::new(Ledger::default()));
}
pub(crate) fn ts(index: usize) -> String {
    format!("1750000{:03}.000001", index)
}
pub(crate) fn reset(s: &mut ShellState, mode: ReadMode) {
    crate::unread::release(s);
    s.channel_generation += 1;
    s.core.pending_scroll_to = None;
    s.loading = false;
    s.loading_older = false;
    s.overlay = None;
    s.toast = None;
    s.read_presentation = Default::default();
    let w = s.core.workspaces.get_mut("T1").unwrap();
    w.read_preferences = ReadPreferences::from_prefs(&mode.patch());
    w.channels
        .get_mut("C2")
        .unwrap()
        .extra
        .insert("is_member".into(), json!(true));
    w.messages.insert("C2".into(),ChannelMessages{messages:(1..=48).map(|i|Message{ts:Some(ts(i)),user:Some("U2".into()),text:Some(format!("Reading position {i:02}. A synthetic message for measuring the real conversation viewport and unread divider.")),..Default::default()}).collect(),loaded:true,last_read:Some(ts(12)),latest:Some(ts(48)),unread_count:36,..Default::default()});
    s.unread_anchor = Some(("C2".into(), ts(12)));
    s.refresh_from_core();
    super::opening::apply(s, true);
}
pub(crate) fn arm(stage: &str) -> Result<(), String> {
    let mut l = LEDGER
        .get()
        .ok_or("No read ledger")?
        .lock()
        .map_err(|_| "Read ledger poisoned")?;
    l.armed = Some(stage.into());
    Ok(())
}
pub(crate) fn disarm() -> Result<(), String> {
    let mut l = LEDGER
        .get()
        .ok_or("No read ledger")?
        .lock()
        .map_err(|_| "Read ledger poisoned")?;
    l.armed = None;
    Ok(())
}
pub(crate) fn attempts() -> Result<Vec<Value>, String> {
    Ok(LEDGER
        .get()
        .ok_or("No read ledger")?
        .lock()
        .map_err(|_| "Read ledger poisoned")?
        .attempts
        .clone())
}
/// Armed one-shot admission records a real builder without acknowledging a cursor.
/// Unarmed observer calls are intentionally ignored; the receipt declares this.
pub(crate) fn record_mark(
    s: &mut ShellState,
    team: &str,
    channel: &str,
    latest: &str,
    previous: Option<&str>,
    revision: u64,
    scope: &str,
) -> bool {
    if !owner(s) || team != "T1" || channel != "C2" || scope != s.timeline_scope() {
        return false;
    }
    let Some(lock) = LEDGER.get() else {
        return false;
    };
    let Ok(mut l) = lock.lock() else { return false };
    let Some(stage) = l.armed.take() else {
        return true;
    };
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: s.core.workspaces["T1"].self_user_id.clone(),
        enterprise_id: None,
        name: "Disconnected navigation".into(),
        url: "https://read-navigation.slack.com".into(),
        token: "fake-read-navigation".into(),
    };
    let request = api::conversations_mark(
        &SlackClient::default(),
        &session,
        channel.into(),
        latest.into(),
    );
    let fields = match &request.body {
        RequestBody::Form(fields) => json!(fields),
        _ => Value::Null,
    };
    l.attempts.push(json!({"stage":stage,"method":request.method,"url":request.url,"fields":fields,"previous":previous,"revision":revision,"scope":scope}));
    true
}
