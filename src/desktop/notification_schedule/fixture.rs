//! Disconnected canonical service; actual requests are prepared, never executed.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{SlackClient, client::RequestBody, models::User};
#[derive(Clone, Default)]
pub(crate) struct Fixture {
    pub(super) raw: BTreeMap<String, Value>,
    pub(super) writes: Vec<Value>,
    pub(super) reads: usize,
    pub(super) pending: Option<Attempt>,
    pub(super) read: Option<Read>,
    pub(super) valid: bool,
}
pub(crate) fn requested() -> bool {
    matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("notification-schedule" | "notification-schedule-narrow")
    )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    (requested() || cfg!(test))
        && s.notification_schedule.fixture.is_some()
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(crate) fn install(s: &mut ShellState) {
    crate::channel_mute::fixture::install(s);
    s.overlay = Some(Overlay::Settings);
    s.settings_section = SettingsSection::Notifications;
    s.toast = None;
    let mut raw:BTreeMap<String,Value>=serde_json::from_value(json!({"dnd_enabled":true,"dnd_days":"every_day","dnd_start_hour":"20:00","dnd_end_hour":"8:00","time24":true,"set_tz_automatically":true})).unwrap();
    for day in Day::ALL {
        raw.insert(format!("dnd_enabled_{}", day.as_str()), json!("partial"));
        raw.insert(format!("dnd_before_{}", day.as_str()), json!("8:00"));
        raw.insert(format!("dnd_after_{}", day.as_str()), json!("22:00"));
    }
    s.notification_schedule = State {
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
pub(super) fn snapshot(s: &ShellState) -> api::NotificationSchedule {
    let t = Token::capture(s).unwrap();
    api::NotificationSchedule::from_prefs(&s.notification_schedule.fixture.as_ref().unwrap().raw)
        .unwrap()
        .with_user_timezone(&User {
            id: t.owner.user,
            tz: Some("America/New_York".into()),
            ..Default::default()
        })
        .unwrap()
}
pub(super) fn read(mut state: Signal<ShellState>, r: Read) -> bool {
    let mut s = state.write();
    if !enabled(&s) {
        return false;
    }
    let f = s.notification_schedule.fixture.as_mut().unwrap();
    f.reads += 1;
    assert!(f.read.replace(r.clone()).is_none());
    if r.operation.is_none() {
        complete_read(&mut s);
    }
    true
}
pub(super) fn complete_read(s: &mut ShellState) {
    if let Some(r) = s
        .notification_schedule
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
    let request = api::notification_schedule_update_request(
        &SlackClient::default(),
        &session(s, &a.token).unwrap(),
        &a.intent,
        &fresh,
    )
    .unwrap();
    let valid = request.method == "POST"
        && !request.retry_safe()
        && request.url.contains("/api/users.prefs.set");
    let RequestBody::Form(fields) = request.body else {
        panic!("schedule expected form")
    };
    let map: serde_json::Map<String, Value> = fields
        .into_iter()
        .filter(|(k, _)| !matches!(k.as_str(), "token" | "_x_id" | "_x_version"))
        .map(|(k, v)| (k, json!(v)))
        .collect();
    let f = s.notification_schedule.fixture.as_mut().unwrap();
    f.valid &= valid;
    f.writes.push(Value::Object(map));
    assert!(f.pending.replace(a).is_none());
}
pub(super) fn complete(s: &mut ShellState, unknown: bool) {
    let Some(a) = s
        .notification_schedule
        .fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return;
    };
    for (k, v) in a.intent.desired_patch() {
        s.notification_schedule
            .fixture
            .as_mut()
            .unwrap()
            .raw
            .insert(k.clone(), v.clone());
    }
    let result = if unknown {
        Err(Error::Transport("synthetic unknown outcome".into()))
    } else {
        Ok(())
    };
    if let Some(r) = finish_write(s, &a, result) {
        s.notification_schedule.fixture.as_mut().unwrap().reads += 1;
        let fresh = snapshot(s);
        finish_read(s, &r, Ok(fresh));
    }
}

pub(super) fn complete_without_apply(s: &mut ShellState) {
    if let Some(a) = s
        .notification_schedule
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
    let intent = s.notification_schedule.entries[&t.owner]
        .pending
        .as_ref()
        .unwrap()
        .intent
        .clone();
    for (k, v) in intent.desired_patch() {
        s.notification_schedule
            .fixture
            .as_mut()
            .unwrap()
            .raw
            .insert(k.clone(), v.clone());
    }
}
