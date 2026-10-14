//! Disconnected canonical preference service and real prepared request capture.
use super::*;
use super_platinum_core::slack::{SlackClient, client::RequestBody};
#[derive(Clone, Default)]
pub(crate) struct Fixture {
    pub(super) backend: Value,
    pub(super) requests: Vec<Value>,
    pub(super) reads: usize,
    pub(super) pending: Option<Attempt>,
    pub(super) read: Option<Read>,
    pub(super) valid: bool,
}
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_CHANNEL_NOTIFICATIONS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-notifications" | "channel-notifications-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    super::super::fixture::install(shell);
    shell.channel_mute.fixture = None;
    let backend = json!({"global":{"global_desktop":"mentions_dms","global_mobile":"mentions_dms","global_desktop_push_enabled":true,"unknown_global":{"keep":true}},"channels":{
        "C2":{"desktop":"mentions_dms","mobile":"mentions_dms","muted":false,"follow_all_threads":true,"badge_all_unreads":true,"suppress_at_channel":true,"unknown_target":[1,2]},
        "C1":{"desktop":"everything","muted":true,"unknown_other":7}}});
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .apply_change("all_notifications_prefs", backend.clone());
    shell.channel_mute.settings_fixture = Some(Fixture {
        backend,
        valid: true,
        ..Default::default()
    });
    shell.refresh_from_core();
}
fn active(shell: &ShellState) -> bool {
    (enabled() || cfg!(test))
        && shell.channel_mute.settings_fixture.is_some()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(super) fn snapshot(shell: &ShellState) -> api::NotificationPreferences {
    api::parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":shell.channel_mute.settings_fixture.as_ref().unwrap().backend}})).unwrap()
}
pub(in crate::channel_mute) fn dispatch(mut state: Signal<ShellState>, attempt: Attempt) -> bool {
    if !enabled() {
        return false;
    }
    dispatch_shell(&mut state.write(), attempt);
    true
}
pub(super) fn dispatch_shell(shell: &mut ShellState, attempt: Attempt) {
    if !active(shell) {
        return;
    }
    let fresh = snapshot(shell);
    if attempt.settings.is_some() {
        if !preflight(shell, &attempt, &fresh, now()) {
            return;
        }
    } else if !mark_sending(shell, &attempt) {
        return;
    }
    let client = SlackClient::default();
    let session = synthetic_session(&attempt.token);
    let request = if let Some(intent) = attempt.settings.as_ref() {
        api::channel_notification_request(&client, &session, intent, &fresh, now())
    } else {
        api::channel_mute_change_request(
            &client,
            &session,
            &attempt.token.owner.channel,
            attempt.change,
        )
    };
    let mut record = json!({});
    let valid = request.is_ok_and(|r| {
        let base = r.method == "POST"
            && !r.retry_safe()
            && r.url.contains("/api/users.prefs.setNotifications");
        let RequestBody::Form(fields) = r.body else {
            return false;
        };
        for (k, v) in fields {
            if !matches!(k.as_str(), "token" | "_x_id" | "_x_version") {
                record[k] = json!(v);
            }
        }
        base && record["global"] == "false"
            && (record["channel_id"] == "C2" || record["channel_ids"] == "C2")
    });
    let f = shell.channel_mute.settings_fixture.as_mut().unwrap();
    f.valid &= valid;
    f.requests.push(record);
    assert!(
        f.pending.replace(attempt).is_none(),
        "duplicate fixture write"
    );
}
pub(in crate::channel_mute) fn read(mut state: Signal<ShellState>, read: Read) -> bool {
    if !enabled() {
        return false;
    }
    let mut s = state.write();
    if active(&s) {
        s.channel_mute.settings_fixture.as_mut().unwrap().read = Some(read);
    }
    true
}
pub(super) fn complete(shell: &mut ShellState, unknown: bool) {
    let Some(a) = shell
        .channel_mute
        .settings_fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return;
    };
    let backend = &mut shell
        .channel_mute
        .settings_fixture
        .as_mut()
        .unwrap()
        .backend["channels"]["C2"];
    if let Some(i) = a.settings.as_ref() {
        for (k, v) in i.desired_patch() {
            backend[k] = v.clone();
        }
        if i.changed_keys()
            .iter()
            .any(|k| k == "temp_mute_expiration_ts")
        {
            backend["temp_mute_expiration_ts"] = json!(0);
        }
    } else {
        match a.change {
            api::ChannelMuteChange::Permanent(v) => {
                backend["muted"] = json!(v);
                backend["temp_mute_expiration_ts"] = json!(0);
            }
            api::ChannelMuteChange::TemporaryUntil(v) => {
                backend["muted"] = json!(false);
                backend["temp_mute_expiration_ts"] = json!(v);
            }
        }
    }
    let current = finish_write(
        shell,
        &a,
        if unknown {
            Err(Error::Transport("synthetic ambiguous response".into()))
        } else {
            Ok(())
        },
        now(),
    );
    if current && !unknown {
        if let Some(r) = Token::capture(shell, "C2").and_then(|t| begin_check(shell, t, now())) {
            finish(shell, r);
        }
    }
}
fn finish(shell: &mut ShellState, read: Read) {
    shell.channel_mute.settings_fixture.as_mut().unwrap().reads += 1;
    let p = snapshot(shell);
    super::super::finish_read(shell, &read, Ok(p), now());
}
pub(super) fn complete_read(shell: &mut ShellState) {
    if let Some(r) = shell
        .channel_mute
        .settings_fixture
        .as_mut()
        .and_then(|f| f.read.take())
    {
        finish(shell, r);
    }
}
