//! Disconnected, exact-opt-in user-link responses; records the real request builder.
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Error,
        client::{PreparedRequest, RequestBody},
        models::{Channel, Message},
    },
    state::ChannelMessages,
};

pub(crate) const ACCOUNT: &str = "00000000-0000-4000-8000-000000000082";
pub(crate) const DRAFT: &str = "Unsent user-link draft \u{65e5}\u{672c}\u{8a9e}";
pub(crate) const STAGES: &[&str] = &[
    "initial", "cached", "created", "pending", "stale", "error", "rotation", "restored",
];
static FIXTURE: OnceLock<Mutex<Fixture>> = OnceLock::new();
struct Fixture {
    nonce: String,
    attempts: Vec<Value>,
    completed: usize,
    held: Option<String>,
}

pub(crate) fn enabled() -> bool {
    admitted(
        std::env::var("SUPER_PLATINUM_USER_LINKS_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn admitted(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1") && matches!(alias, Some("user-links" | "user-links-narrow"))
}
pub(crate) fn directory() -> Result<(PathBuf, String), String> {
    if !enabled() {
        return Err("User-link opt-in missing".into());
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_USER_LINKS_DIR").ok_or("Private directory missing")?,
    );
    let nonce = std::env::var("SUPER_PLATINUM_USER_LINKS_NONCE").map_err(|_| "Nonce missing")?;
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("user-links-"))
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
pub(crate) fn owner(shell: &ShellState) -> bool {
    enabled()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.core.transport.is_none()
        && shell.core.session.as_ref().is_some_and(|s| {
            s.d_cookie == "fake-user-links"
                && s.workspaces.len() == 1
                && s.workspaces.get("T1").is_some_and(|w| {
                    w.team_id == "T1"
                        && w.url == "https://user-links.slack.com"
                        && matches!(
                            w.token.as_str(),
                            "fake-user-links" | "fake-user-links-rotated"
                        )
                        && w.enterprise_id.is_none()
                        && shell
                            .core
                            .workspaces
                            .get("T1")
                            .is_some_and(|team| team.self_user_id == w.user_id)
                })
        })
}
pub(crate) fn install(shell: &mut ShellState) {
    let Ok((_, nonce)) = directory() else {
        shell.show_toast("User-link fixture configuration is invalid.");
        return;
    };
    if FIXTURE
        .set(Mutex::new(Fixture {
            nonce,
            attempts: Vec::new(),
            completed: 0,
            held: None,
        }))
        .is_err()
    {
        return;
    }
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.transport = None;
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fake-user-links".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                name: "User links".into(),
                url: "https://user-links.slack.com".into(),
                token: "fake-user-links".into(),
            },
        )]
        .into(),
    });
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    let template = workspace.users["U1"].clone();
    for (id, name) in [
        ("UCACHED", "Cached Person"),
        ("UCREATED", "New Person"),
        ("UDELAY", "Delayed Person"),
        ("UWRONG", "Wrong Response"),
        ("UERROR", "Unavailable Person"),
        ("UROTATE", "Rotating Person"),
        ("UAPP", "App Person"),
    ] {
        let mut user = template.clone();
        user.id = id.into();
        user.name = Some(name.into());
        user.real_name = Some(name.into());
        user.is_bot = false;
        user.deleted = false;
        if let Some(profile) = user.profile.as_mut() {
            profile.real_name = Some(name.into());
            profile.display_name = Some(name.into());
        }
        if id == "UAPP" {
            user.extra.insert("is_app_user".into(), true.into());
        }
        workspace.users.insert(id.into(), user);
    }
    workspace
        .channels
        .insert("DCACHED".into(), channel("DCACHED", "UCACHED"));
    let links = "Open a person and keep their profile beside the direct message.\n<slack://user?team=T1&id=UCACHED|Open cached person>\n<slack://user?team=T1&id=UCREATED|Open new person>\n<slack://user?team=T1&id=UDELAY|Open delayed person>\n<slack://user?team=T1&id=UWRONG|Open wrong response>\n<slack://user?team=T1&id=UERROR|Open unavailable person>\n<slack://user?team=T1&id=UROTATE|Open rotating person>\n<slack://user?team=T1&id=UUNKNOWN|Unknown person>\n<slack://user?team=T1&id=UAPP|App person>";
    let mut messages = ChannelMessages::default();
    messages.upsert(
        serde_json::from_value::<Message>(
            json!({"channel":"C2","ts":"1719800000.000100","user":"U1","text":links}),
        )
        .unwrap(),
    );
    workspace.messages.insert("C2".into(), messages);
    shell.refresh_from_core();
    let index = shell
        .channels
        .iter()
        .position(|row| row.id == "C2")
        .unwrap();
    shell.select_channel(index, ChannelOpen::Global);
    shell.core.composer = super_platinum_core::ComposerState::with_text(DRAFT);
    shell.close_profile();
    shell.overlay = None;
    shell.toast = None;
}
fn channel(id: &str, user: &str) -> Channel {
    Channel {
        id: id.into(),
        is_im: true,
        user: Some(user.into()),
        ..Default::default()
    }
}
pub(crate) fn attempts() -> Result<Vec<Value>, String> {
    Ok(FIXTURE
        .get()
        .ok_or("Fixture missing")?
        .lock()
        .map_err(|_| "Fixture lock")?
        .attempts
        .clone())
}
pub(crate) fn completed() -> Result<usize, String> {
    Ok(FIXTURE
        .get()
        .ok_or("Fixture missing")?
        .lock()
        .map_err(|_| "Fixture lock")?
        .completed)
}
pub(crate) fn hold(user: Option<&str>) -> Result<(), String> {
    FIXTURE
        .get()
        .ok_or("Fixture missing")?
        .lock()
        .map_err(|_| "Fixture lock")?
        .held = user.map(str::to_owned);
    Ok(())
}

pub(crate) async fn execute(
    state: Signal<ShellState>,
    request: PreparedRequest,
    user: &str,
) -> Result<Value, Error> {
    let invalid = || Error::Api("invalid_user_link_fixture".into());
    if !owner(&state.peek()) {
        return Err(invalid());
    }
    let (_, nonce) = directory().map_err(|_| invalid())?;
    let fields = match request.body {
        RequestBody::Form(fields) => fields,
        _ => return Err(invalid()),
    };
    {
        let mut fixture = FIXTURE
            .get()
            .ok_or_else(invalid)?
            .lock()
            .map_err(|_| invalid())?;
        if fixture.nonce != nonce {
            return Err(invalid());
        }
        fixture
            .attempts
            .push(json!({"method":request.method,"url":request.url,"fields":fields}));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let held = FIXTURE
            .get()
            .ok_or_else(invalid)?
            .lock()
            .map_err(|_| invalid())?
            .held
            .as_deref()
            == Some(user);
        if !held {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Api("fixture_hold_timeout".into()));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let returned = match user {
        "UCREATED" => Ok(channel("DCREATED", user)),
        "UDELAY" => Ok(channel("DDELAY", user)),
        "UWRONG" => Ok(channel("DWRONG", "UCREATED")),
        "UROTATE" => Ok(channel("DROTATE", user)),
        "UERROR" => Err(Error::Api("user_not_found".into())),
        _ => Err(invalid()),
    };
    FIXTURE
        .get()
        .ok_or_else(invalid)?
        .lock()
        .map_err(|_| invalid())?
        .completed += 1;
    returned.map(|channel| json!({"ok":true,"channel":channel}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_adapter_requires_both_exact_alias_and_opt_in() {
        for alias in [Some("user-links"), Some("user-links-narrow")] {
            assert!(admitted(Some("1"), alias));
            for flag in [None, Some("true"), Some("0"), Some("1 ")] {
                assert!(!admitted(flag, alias));
            }
        }
        for alias in [
            None,
            Some("user-links-other"),
            Some("slack-links"),
            Some("normal"),
        ] {
            assert!(!admitted(Some("1"), alias));
        }
    }
}
