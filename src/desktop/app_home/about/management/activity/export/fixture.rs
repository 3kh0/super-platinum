//! Closed CSV endpoint; replies pass through the actual claimed export attempt.
use super::*;
use std::sync::{Arc, Mutex};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;

#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
#[derive(Default)]
struct Ledger {
    policy: Policy,
    mode: Mode,
    requests: Vec<Value>,
    held: Option<(u64, oneshot::Sender<Result<Value, Error>>)>,
    finished: usize,
    cancelled: usize,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Policy {
    #[default]
    On,
    Off,
    Missing,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Rejected,
    Malformed,
    Offline,
    Lost,
    Held,
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    super::super::fixture::enabled(s)
        && matches!(
            super::super::fixture::case().as_str(),
            "export" | "export-retirement"
        )
        && s.app_home.about.management.export_fixture.is_some()
}
pub(crate) fn install(s: &mut ShellState) {
    s.app_home.about.management.export_fixture = Some(Model::default());
}
/// Initial selected filter only; this scene tests export consumption, not picker admission.
pub(crate) fn initial_user(s: &mut ShellState) -> Result<Value, String> {
    model(s).map_err(|_| "Export fixture unavailable")?;
    let visit = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or("Missing editor")?
        .id;
    let label = super::super::view::user(s, "TWORKFLOW", "UWORKFLOW");
    let d = super::super::document_mut(s, visit).ok_or("Missing document")?;
    if d.activity.active || d.activity.page.is_some() {
        return Err("Initial export user must precede Activity entry".into());
    }
    d.activity.filters.user = "UWORKFLOW".into();
    d.activity.member_selection = Some(("UWORKFLOW".into(), label.clone()));
    Ok(
        json!({"user":"UWORKFLOW","label":label,"initialFixtureSelection":true,"pickerSelectionClaimed":false}),
    )
}
fn unavailable_fixture() -> Error {
    Error::Offline("Closed Activity export fixture refused the request".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(unavailable_fixture());
    }
    s.app_home
        .about
        .management
        .export_fixture
        .clone()
        .ok_or_else(unavailable_fixture)
}
pub(crate) fn policy(s: &ShellState, value: Policy) -> Result<(), String> {
    model(s)
        .map_err(|_| "Export fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .policy = value;
    Ok(())
}
pub(crate) fn mode(s: &ShellState, value: Mode) -> Result<(), String> {
    model(s)
        .map_err(|_| "Export fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .mode = value;
    Ok(())
}
pub(crate) fn decorate_policy(s: &ShellState, raw: &mut Value) {
    let Ok(model) = model(s) else {
        return;
    };
    let Some(assignments) = raw.get_mut("assignments").and_then(Value::as_object_mut) else {
        return;
    };
    let key = "wfb_export_activity_log_as_csv";
    match model.0.lock().unwrap().policy {
        Policy::On => {
            assignments.insert(key.into(), json!({"group":"on"}));
        }
        Policy::Off => {
            assignments.insert(key.into(), json!({"group":"off"}));
        }
        Policy::Missing => {
            assignments.remove(key);
        }
    }
}
fn result(mode: Mode) -> Result<Value, Error> {
    match mode {
        Mode::Success => {
            Ok(json!({"ok":true,"fixture_receipt":"initiation-only","retained_unknown":17}))
        }
        Mode::Rejected => Ok(json!({"ok":false,"error":"not_allowed"})),
        Mode::Malformed => Ok(json!({"ok":"malformed"})),
        Mode::Offline => Err(Error::Offline("synthetic export never dispatched".into())),
        Mode::Lost => Err(Error::Transport(
            "synthetic export reply lost after dispatch".into(),
        )),
        Mode::Held => Err(unavailable_fixture()),
    }
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    id: u64,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = model(&state.peek())?;
    if !super::fixture_plan(&state.peek(), id, request) {
        return Err(unavailable_fixture());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(unavailable_fixture());
    };
    let args: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| key != "token")
        .map(|(key, value)| (key.clone(), json!(value)))
        .collect();
    let receiver = {
        let mut m = model.0.lock().unwrap();
        if m.held.is_some() || m.requests.iter().any(|r| r["attempt"] == id) {
            return Err(unavailable_fixture());
        }
        let serial = m.requests.len() + 1;
        let mode = std::mem::take(&mut m.mode);
        m.requests.push(
            json!({"serial":serial,"attempt":id,"method":"workflows.activity.export","args":args,
            "dispatch":if matches!(mode,Mode::Offline){"not-sent"}else{"synthetic-entered"}}),
        );
        if !matches!(mode, Mode::Held) {
            m.finished += 1;
            return result(mode);
        }
        let (sender, receiver) = oneshot::channel();
        m.held = Some((id, sender));
        receiver
    };
    struct Finish {
        model: Model,
        completed: bool,
    }
    impl Drop for Finish {
        fn drop(&mut self) {
            let mut m = self.model.0.lock().unwrap();
            m.finished += 1;
            if !self.completed {
                m.cancelled += 1;
            }
        }
    }
    let mut finish = Finish {
        model,
        completed: false,
    };
    let response = receiver.await.map_err(|_| unavailable_fixture())?;
    finish.completed = true;
    response
}
pub(crate) fn release(s: &ShellState, mode: Mode) -> Result<Value, String> {
    let model = model(s).map_err(|_| "Export fixture unavailable")?;
    let (id, sender) = model
        .0
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held export request")?;
    let delivered = sender.send(result(mode)).is_ok();
    Ok(json!({"attempt":id,"delivered":delivered,"cancelledBeforeReply":!delivered}))
}
pub(crate) fn replace_account(mut state: Signal<ShellState>) -> Result<Value, String> {
    let saved = model(&state.peek()).map_err(|_| "Export fixture unavailable")?;
    let proof = super::super::fixture::replace_account(state)?;
    state.write().app_home.about.management.export_fixture = Some(saved);
    Ok(proof)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let Some(model) = s.app_home.about.management.export_fixture.as_ref() else {
        return Value::Null;
    };
    let m = model.0.lock().unwrap();
    json!({"requests":m.requests,"finished":m.finished,"cancelled":m.cancelled,
        "held":m.held.as_ref().map(|(id,_)|id),"networkCalls":0,
        "policy":match m.policy{Policy::On=>"on",Policy::Off=>"off",Policy::Missing=>"missing"}})
}
