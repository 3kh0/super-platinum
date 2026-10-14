//! Disconnected canonical service; actual requests are prepared, never executed.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{SlackClient, client::RequestBody};
#[derive(Clone, Default)]
pub(crate) struct Fixture {
    pub(super) raw: Value,
    pub(super) writes: Vec<Value>,
    pub(super) reads: usize,
    pub(super) pending: Option<Attempt>,
    pub(super) read: Option<Read>,
    pub(super) valid: bool,
}
pub(crate) fn requested() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("notification-presentation" | "notification-presentation-narrow")
    )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    (requested() || cfg!(test))
        && s.presentation_notifications.fixture.is_some()
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(crate) fn install(s: &mut ShellState) {
    crate::channel_mute::fixture::install(s);
    s.overlay = Some(Overlay::Settings);
    s.settings_section = SettingsSection::Notifications;
    s.toast = None;
    let workspace = s.core.workspaces.get_mut("T1").unwrap();
    workspace.team_plan = Some("paid".into());
    workspace.vip_users.insert(workspace.self_user_id.clone());
    let raw = json!({"all_notifications_prefs":{"global":{"global_desktop":"everything","desktop_sound":"been_tree.mp3","priority_desktop_sound":"none","no_text_in_notifications":false,"unknown_global":{"keep":9}},"channels":{"C2":{"desktop":"nothing","muted":true,"unknown_channel":7}}},"mute_sounds":false,"mac_ssb_bounce":""});

    s.presentation_notifications = State {
        fixture: Some(Fixture {
            raw,
            valid: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .user
        .insert("unknown_pref".into(), json!({"keep":7}));
}
pub(super) fn snapshot(s: &ShellState) -> api::PresentationNotificationPreferences {
    api::PresentationNotificationPreferences::from_response(
        &json!({"ok":true,"prefs":s.presentation_notifications.fixture.as_ref().unwrap().raw}),
    )
    .unwrap()
}
pub(super) fn read(mut state: Signal<ShellState>, r: Read) -> bool {
    let mut s = state.write();
    if !enabled(&s) {
        return false;
    }
    let f = s.presentation_notifications.fixture.as_mut().unwrap();
    f.reads += 1;
    assert!(f.read.replace(r.clone()).is_none());
    if r.operation.is_none() {
        complete_read(&mut s);
    }
    true
}
pub(super) fn complete_read(s: &mut ShellState) {
    if let Some(r) = s
        .presentation_notifications
        .fixture
        .as_mut()
        .and_then(|f| f.read.take())
    {
        let fresh = snapshot(s);
        finish_read(s, &r, Ok(fresh));
    }
}
pub(super) fn dispatch(mut state: Signal<ShellState>, a: Attempt) -> bool {
    let mut s = state.write();
    if !enabled(&s) {
        return false;
    }
    dispatch_shell(&mut s, a);
    true
}
pub(super) fn dispatch_shell(s: &mut ShellState, a: Attempt) {
    let fresh = snapshot(s);
    if !preflight(s, &a, &fresh) {
        return;
    }
    let request = api::presentation_notification_request(
        &SlackClient::default(),
        &session(s, &a.token).unwrap(),
        &a.intent,
        &fresh,
    )
    .unwrap();
    let valid = request.method == "POST"
        && !request.retry_safe()
        && url::Url::parse(&request.url).is_ok_and(|url| {
            url.path()
                == if a.intent.is_user_pref() {
                    "/api/users.prefs.set"
                } else {
                    "/api/users.prefs.setNotifications"
                }
        });
    let RequestBody::Form(fields) = request.body else {
        panic!("presentation expected form")
    };
    let map: serde_json::Map<String, Value> = fields
        .into_iter()
        .filter(|(k, _)| !matches!(k.as_str(), "token" | "_x_id" | "_x_version"))
        .map(|(k, v)| (k, json!(v)))
        .collect();
    let f = s.presentation_notifications.fixture.as_mut().unwrap();
    f.valid &= valid;
    f.writes.push(Value::Object(map));
    assert!(f.pending.replace(a).is_none());
}
pub(super) fn complete(s: &mut ShellState, unknown: bool) {
    let Some(a) = s
        .presentation_notifications
        .fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return;
    };
    apply(s, &a.intent);
    let result = if unknown {
        Err(Error::Transport("synthetic unknown outcome".into()))
    } else {
        Ok(())
    };
    if let Some(r) = finish_write(s, &a, result) {
        s.presentation_notifications.fixture.as_mut().unwrap().reads += 1;
        let fresh = snapshot(s);
        finish_read(s, &r, Ok(fresh));
    }
}

pub(super) fn complete_without_apply(s: &mut ShellState) {
    if let Some(a) = s
        .presentation_notifications
        .fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    {
        finish_write(
            s,
            &a,
            Err(Error::Transport("synthetic delayed outcome".into())),
        );
    }
}
pub(super) fn apply_unconfirmed(s: &mut ShellState) {
    let t = Token::capture(s).unwrap();
    let intent = s.presentation_notifications.entries[&t.owner]
        .pending
        .as_ref()
        .unwrap()
        .intent
        .clone();
    apply(s, &intent);
}
fn apply(s: &mut ShellState, intent: &api::PresentationNotificationIntent) {
    let raw = &mut s.presentation_notifications.fixture.as_mut().unwrap().raw;
    let target = if intent.is_user_pref() {
        raw.as_object_mut().unwrap()
    } else {
        raw["all_notifications_prefs"]["global"]
            .as_object_mut()
            .unwrap()
    };
    for (k, v) in intent.desired_patch() {
        target.insert(k.clone(), v.clone());
    }
}
