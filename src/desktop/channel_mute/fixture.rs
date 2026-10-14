//! Exact-gated synthetic preference backend. Prepared requests never execute.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

#[derive(Clone, Default)]
pub(super) struct Fixture {
    attempts: Vec<Value>,
    checks: usize,
    valid: bool,
    backend: Value,
    pending: Option<Attempt>,
    read: Option<Read>,
}
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_CHANNEL_MUTE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-mute" | "channel-mute-narrow")
        )
}
pub(crate) fn temporary_clock() -> Option<(i64, i32)> {
    enabled().then_some((4_095_590_400, -14_400)) // 2099-10-13 12:00:00, UTC-04:00
}
fn baseline() -> Value {
    json!({"global":{"global_desktop":"mentions_dms","unknown_global":"preserve"},
        "channels":{"C1":{"muted":true,"desktop":"nothing","unknown_other":{"keep":7}},
        "C2":{"muted":false,"desktop":"everything","temp_mute_expiration_ts":0,"unknown_target":{"keep":[1,2]}}}})
}
pub(crate) fn install(shell: &mut ShellState) {
    shell.core.transport = None;
    shell.core.session = None;
    shell.core.active_account = Some("channel-mute-disconnected".into());
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    let backend = baseline();
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    let channel = workspace.channels.get_mut("C2").unwrap();
    channel.is_channel = true;
    channel.is_im = false;
    channel.is_mpim = false;
    channel.is_archived = false;
    channel.extra.insert("is_member".into(), json!(true));
    workspace
        .notifications
        .apply_prefs(&super_platinum_core::slack::models::BootPrefs::default());
    workspace
        .notifications
        .apply_change("all_notifications_prefs", backend.clone());
    workspace.sidebar.muted.insert("C1".into());
    workspace.sidebar.muted.remove("C2");
    shell.channel_mute = State {
        fixture: Some(Fixture {
            backend,
            valid: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    shell.refresh_from_core();
}
fn session(attempt: &Attempt) -> WorkspaceSession {
    WorkspaceSession {
        team_id: attempt.token.owner.team.clone(),
        user_id: attempt.token.owner.user.clone(),
        enterprise_id: None,
        name: "Disconnected mute fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    }
}
pub(super) fn dispatch(mut state: Signal<ShellState>, attempt: Attempt) -> bool {
    if !enabled() {
        return false;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some()
        || shell.core.session.is_some()
        || shell.channel_mute.fixture.is_none()
    {
        return true;
    }
    if !mark_sending(&mut shell, &attempt) {
        return true;
    }
    let request = api::channel_mute_change_request(
        &SlackClient::default(),
        &session(&attempt),
        &attempt.token.owner.channel,
        attempt.change,
    )
    .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("mute fixture expected form")
    };
    let field = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let channels = field("channel_ids").unwrap().to_owned();
    let prefs: Value = serde_json::from_str(field("prefs").unwrap()).unwrap();
    let expected = match attempt.change {
        api::ChannelMuteChange::Permanent(muted) => {
            json!([{"name":"muted","value":if muted {"true"}else{"false"}}])
        }
        api::ChannelMuteChange::TemporaryUntil(expiration) => {
            json!([{"name":"temp_mute_expiration_ts","value":expiration.to_string()}])
        }
    };
    let exact = request.method == "POST"
        && request
            .url
            .split('?')
            .next()
            .is_some_and(|url| url.ends_with("/api/users.prefs.setNotifications"))
        && field("global") == Some("false")
        && field("sync") == Some("false")
        && channels == "C2"
        && prefs == expected;
    let fixture = shell.channel_mute.fixture.as_mut().unwrap();
    fixture.valid &= exact;
    fixture
        .attempts
        .push(json!({"channels":channels,"prefs":prefs}));
    assert!(
        fixture.pending.replace(attempt).is_none(),
        "duplicate native mute request"
    );
    true
}
pub(super) fn read(mut state: Signal<ShellState>, read: Read) -> bool {
    if !enabled() {
        return false;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() || shell.core.session.is_some() {
        return true;
    }
    let fixture = shell.channel_mute.fixture.as_mut().unwrap();
    fixture.checks += 1;
    assert!(
        fixture.read.replace(read).is_none(),
        "duplicate native preference check"
    );
    true
}
fn canonical(shell: &ShellState) -> api::NotificationPreferences {
    api::parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":shell.channel_mute.fixture.as_ref().unwrap().backend}})).unwrap()
}
pub(crate) fn complete(shell: &mut ShellState, uncertain: bool) {
    let Some(attempt) = shell
        .channel_mute
        .fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return;
    };
    let backend = &mut shell.channel_mute.fixture.as_mut().unwrap().backend["channels"]["C2"];
    match attempt.change {
        api::ChannelMuteChange::Permanent(muted) => {
            backend["muted"] = json!(muted);
            backend["temp_mute_expiration_ts"] = json!(0);
        }
        api::ChannelMuteChange::TemporaryUntil(expiration) => {
            backend["temp_mute_expiration_ts"] = json!(expiration);
            backend["muted"] = json!(false);
        }
    }
    let result = if uncertain {
        Err(Error::Transport("synthetic ambiguous write".into()))
    } else {
        Ok(())
    };
    if finish_write(shell, &attempt, result, now()) {
        let read = Token::capture(shell, "C2").and_then(|token| begin_check(shell, token, now()));
        if let Some(read) = read {
            // A failed automatic read leaves the explicit, read-only Check status
            // action available. Success installs through the production reducer.
            let result = if uncertain {
                Err(Error::Transport("synthetic read failure".into()))
            } else {
                Ok(canonical(shell))
            };
            finish_read(shell, &read, result, now());
        }
    }
}
pub(crate) fn complete_read(shell: &mut ShellState) {
    let Some(read) = shell
        .channel_mute
        .fixture
        .as_mut()
        .and_then(|f| f.read.take())
    else {
        return;
    };
    let prefs = canonical(shell);
    finish_read(shell, &read, Ok(prefs), now());
}
pub(crate) fn realtime(shell: &mut ShellState, muted: bool) {
    shell.channel_mute.fixture.as_mut().unwrap().backend["channels"]["C2"]["muted"] = json!(muted);
    let prefs = canonical(shell);
    let owner = Token::capture(shell, "C2").unwrap().owner;
    adopt(shell, &owner, &prefs, now());
}
pub(crate) fn attempts(shell: &ShellState) -> usize {
    shell.channel_mute.fixture.as_ref().unwrap().attempts.len()
}
pub(crate) fn reads(shell: &ShellState) -> usize {
    shell.channel_mute.fixture.as_ref().unwrap().checks
}
pub(crate) fn prepared_valid(shell: &ShellState) -> bool {
    shell.channel_mute.fixture.as_ref().unwrap().valid
}
pub(crate) fn preserved(shell: &ShellState) -> bool {
    let original = baseline();
    let prefs = &shell.core.workspaces["T1"].notifications;
    serde_json::to_value(&prefs.global).unwrap() == original["global"]
        && serde_json::to_value(&prefs.channels["C1"]).unwrap() == original["channels"]["C1"]
        && prefs.channels["C2"]["desktop"] == original["channels"]["C2"]["desktop"]
        && prefs.channels["C2"]["unknown_target"] == original["channels"]["C2"]["unknown_target"]
}

pub(crate) fn temporary_matches(shell: &ShellState, index: usize, seconds: i64) -> bool {
    let expected = 4_095_590_400_i64 + seconds;
    let fixture = shell.channel_mute.fixture.as_ref().unwrap();
    fixture.attempts.get(index).is_some_and(|attempt| {
        attempt["prefs"] == json!([{"name":"temp_mute_expiration_ts","value":expected.to_string()}])
    }) && shell.core.workspaces["T1"].notifications.channels["C2"]["temp_mute_expiration_ts"]
        == json!(expected)
        && shell.core.workspaces["T1"].notifications.channels["C2"]["muted"] == json!(false)
}
