//! Exact-opt-in Escape fixture: real request builders, synthetic acknowledgements.
use crate::state::ShellState;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use super_platinum_core::{
    config::WorkspaceSession,
    domain::ReadTarget,
    slack::{SlackClient, api, client::RequestBody},
    state::ReadMode,
};
pub(crate) const ACCOUNT: &str = "reading-shortcuts-disconnected";
pub(crate) const DRAFT: &str = "Keep this channel draft.\nSecond line \u{65e5}\u{672c}.";
pub(crate) const REPLY: &str = "Keep this thread reply.\nSecond line.";
pub(crate) const ROOT: &str = "1719800400.000000";
pub(crate) const THREAD_LATEST: &str = "1719800404.000000";
pub(crate) const STAGES: &[&str] = &[
    "initial",
    "metadata-read",
    "ghost-cleared",
    "composer-empty",
    "composer-nonempty",
    "thread-read",
    "thread-closed",
    "focused-primary",
    "blocked-keys",
    "stale-events",
];
#[derive(Default)]
struct Ledger {
    stage: String,
    attempts: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_READ_SHORTCUTS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("reading-shortcuts" | "reading-shortcuts-narrow")
        )
}
pub(crate) fn directory() -> Result<(PathBuf, String), String> {
    if !enabled() {
        return Err("Reading-shortcuts opt-in missing".into());
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_READ_SHORTCUTS_DIR").ok_or("Private directory missing")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_READ_SHORTCUTS_NONCE").map_err(|_| "Nonce missing")?;
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("reading-shortcuts-"))
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
    let _ = LEDGER.set(Mutex::new(Ledger::default()));
    reset(s);
}
pub(crate) fn reset(s: &mut ShellState) {
    s.close_thread();
    s.close_profile();
    s.core.composer.text = DRAFT.into();
    crate::read_preferences::navigation_fixture::reset(s, ReadMode::NewestKeepUnseen);
    s.core.pending_scroll_to = Some((
        "C2".into(),
        super_platinum_core::domain::PendingScrollTarget::Latest,
    ));
    s.stick_to_bottom = true;
    let channel = s
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C2")
        .unwrap();
    channel.unread_count = Some(36);
    channel.unread_count_display = Some(36);
    channel.has_unreads = true;
    channel.extra.insert("latest".into(), json!(ts(48)));
    s.refresh_from_core();
}
pub(crate) fn ts(index: usize) -> String {
    crate::read_preferences::navigation_fixture::ts(index)
}
pub(crate) fn thread(s: &mut ShellState) {
    crate::thread_unread::fixture(s);
    let key = ("T1".into(), "C2".into(), ROOT.into());
    s.core.threads.get_mut(&key).unwrap().messages[0]
        .extra
        .insert("subscribed".into(), json!(true));
    s.core.thread_composer.text = REPLY.into();
    s.thread_loading = false;
    s.thread_error = None;
    s.thread_target = None;
    s.thread_at_bottom = true;
    s.refresh_from_core();
}
pub(crate) fn stage(stage: &str) -> Result<(), String> {
    LEDGER
        .get()
        .ok_or("No Escape ledger")?
        .lock()
        .map_err(|_| "Escape ledger poisoned")?
        .stage = stage.into();
    Ok(())
}
pub(crate) fn attempts() -> Result<Vec<Value>, String> {
    Ok(LEDGER
        .get()
        .ok_or("No Escape ledger")?
        .lock()
        .map_err(|_| "Escape ledger poisoned")?
        .attempts
        .clone())
}
pub(crate) fn record_mark(s: &mut ShellState, target: &ReadTarget, ts: &str) -> bool {
    if !owner(s) {
        return false;
    }
    let Some(lock) = LEDGER.get() else {
        return false;
    };
    let Ok(mut ledger) = lock.lock() else {
        return false;
    };
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: s.core.workspaces["T1"].self_user_id.clone(),
        enterprise_id: None,
        name: "Disconnected reading shortcuts".into(),
        url: "https://reading-shortcuts.slack.com".into(),
        token: "fake-reading-shortcuts".into(),
    };
    let request = match target {
        ReadTarget::Conversation { team, channel } if team == "T1" && channel == "C2" => {
            api::conversations_mark(
                &SlackClient::default(),
                &session,
                channel.clone(),
                ts.into(),
            )
        }
        ReadTarget::Thread {
            team,
            channel,
            root_ts,
        } if team == "T1" && channel == "C2" && root_ts == ROOT => api::subscriptions_thread_mark(
            &SlackClient::default(),
            &session,
            channel.clone(),
            root_ts.clone(),
            ts.into(),
        ),
        _ => return false,
    };
    let target = match target {
        ReadTarget::Conversation { team, channel } => {
            json!({"kind":"conversation","team":team,"channel":channel})
        }
        ReadTarget::Thread {
            team,
            channel,
            root_ts,
        } => json!({"kind":"thread","team":team,"channel":channel,"rootTs":root_ts}),
    };
    let fields = match &request.body {
        RequestBody::Form(fields) => json!(fields),
        _ => return false,
    };
    let stage = ledger.stage.clone();
    ledger.attempts.push(json!({"stage":stage,"target":target,"ts":ts,"method":request.method,"url":request.url,"fields":fields}));
    true
}
