//! Closed Permissions responses, preserving original dispatch and receiver custody.
use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
#[cfg(test)]
mod tests;
mod wire;

pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_PERMISSIONS_CASE";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn supported(value: &str) -> bool {
    matches!(
        value,
        "permissions-success"
            | "permissions-rejected"
            | "permissions-unknown"
            | "permissions-retirement"
    )
}
pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("workflow-editor" | "workflow-editor-narrow")
        )
        && std::env::var(super::run_access::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
        && std::env::var(super::managers::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
        && supported(&case())
        && std::env::var(super::delete::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
        && std::env::var(super::unpublish::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
}

#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    account: Option<String>,
    epoch: u64,
    credentials: String,
    original: Value,
    workflow: Value,
    trigger: Value,
    copy: Value,
    run: Value,
    external: Value,
    armed: bool,
    held_once: bool,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<(usize, oneshot::Sender<()>)>,
}
pub(super) fn install(s: &mut ShellState) {
    if !requested() {
        return;
    }
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    let copy = json!({"ok":true,"permission_type":"collaborators"});
    let run = json!({"ok":true,"permission_type":"app_collaborators"});
    let external = json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]}});
    let data = Data {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        credentials: serde_json::to_string(s.core.session.as_ref().unwrap()).unwrap(),
        original: json!({"workflow":m.workflow,"trigger":m.trigger,"copy":copy,"run":run,"external":external}),
        workflow: m.workflow.clone(),
        trigger: m.trigger.clone(),
        copy,
        run,
        external,
        armed: false,
        held_once: false,
        requests: vec![],
        responses: vec![],
        settlements: vec![],
        releases: vec![],
        held: None,
    };
    s.app_home
        .about
        .management
        .fixture
        .as_mut()
        .unwrap()
        .permissions = Some(Model(Arc::new(Mutex::new(data))));
}
fn closed() -> Error {
    Error::Api("workflow_management_not_sent_permissions_fixture".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !requested() || !super::enabled(s) {
        return Err(closed());
    }
    let m = s
        .app_home
        .about
        .management
        .fixture
        .as_ref()
        .and_then(|m| m.permissions.clone())
        .ok_or_else(closed)?;
    if !m.current(s) {
        return Err(closed());
    }
    Ok(m)
}
impl Model {
    fn current(&self, s: &ShellState) -> bool {
        let m = self.0.lock().unwrap();
        s.core.active_account == m.account
            && s.core.account_epoch == m.epoch
            && s.core.active_team.as_deref() == Some("TWORKFLOW")
            && s.core
                .session
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())
                .as_ref()
                == Some(&m.credentials)
    }
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        json!({"original":m.original,"owner":{"account":m.account,"epoch":m.epoch,"team":"TWORKFLOW","user":"UWORKFLOW"},
            "current":{"workflow":m.workflow,"trigger":m.trigger,"copy":m.copy,"run":m.run,"external":m.external},
            "requests":m.requests,"responses":m.responses,"settlements":m.settlements,
            "releases":m.releases,"held":m.held.as_ref().map(|(n,_)|n),
            "networkCalls":0,"completed":m.settlements.len(),
            "cancelled":m.settlements.iter().filter(|v|v["returned"]==false).count()})
    }
    /// Releases only this retained original sender, even after its account retired.
    pub(crate) fn release(&self) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let (serial, sender) = m
            .held
            .take()
            .ok_or("No original Permissions response held")?;
        let receiver_retired = sender.is_closed();
        let v = json!({"serial":serial,"receiverRetired":receiver_retired,"delivered":sender.send(()).is_ok(),
            "response":m.responses.iter().find(|v|v["serial"]==serial).cloned()});
        m.releases.push(v.clone());
        Ok(v)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    model(s).map_err(|_| "Original Permissions fixture unavailable".into())
}
pub(crate) fn arm(s: &ShellState) -> Result<(), String> {
    let m = capture(s)?;
    let mut m = m.0.lock().unwrap();
    if m.armed {
        return Err("Permissions fixture already armed".into());
    }
    m.armed = true;
    Ok(())
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    s.app_home
        .about
        .management
        .fixture
        .as_ref()
        .and_then(|m| m.permissions.as_ref())
        .map(|m| {
            let mut v = m.snapshot();
            v["ownerCurrent"] = json!(m.current(s));
            v
        })
        .unwrap_or(Value::Null)
}

fn owner(s: &ShellState) -> Value {
    let h = s.app_home.about.management.surface.as_ref();
    let d = h.and_then(|h| document(s, h.id));
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,
        "team":s.core.active_team,"user":h.map(|h|&h.owner.ctx.session.user_id),
        "visit":h.map(|h|h.id),"document":d.map(|d|d.id),"revision":d.map(|d|d.revision)})
}
fn receipt(result: &Result<Value, Error>) -> Value {
    match result {
        Ok(v) => json!({"raw":v,"error":null}),
        Err(Error::Transport(_)) => json!({"raw":null,"error":"transport"}),
        Err(Error::Offline(_)) => json!({"raw":null,"error":"offline"}),
        Err(Error::Api(code)) => json!({"raw":null,"error":code}),
        Err(_) => json!({"raw":null,"error":"other"}),
    }
}
struct Finish {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Finish {
    fn drop(&mut self) {
        self.model
            .0
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":self.serial,"returned":self.returned}));
    }
}
async fn deliver(
    model: Model,
    serial: usize,
    hold: bool,
    result: Result<Value, Error>,
) -> Result<Value, Error> {
    let receiver = {
        let mut m = model.0.lock().unwrap();
        m.responses
            .push(json!({"serial":serial,"result":receipt(&result)}));
        if hold {
            let (tx, rx) = oneshot::channel();
            m.held = Some((serial, tx));
            m.held_once = true;
            Some(rx)
        } else {
            None
        }
    };
    let mut finish = Finish {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = receiver {
        tokio::time::timeout(std::time::Duration::from_secs(45), rx)
            .await
            .map_err(|_| closed())?
            .map_err(|_| closed())?;
    }
    finish.returned = true;
    result
}

pub(crate) async fn read(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let (method, fields) = super::decode(request)?;
        let witness = owner(&s);
        let mut result = super::response(&mut s, request);
        let (serial, hold) = {
            let mut m = model.0.lock().unwrap();
            if m.held.is_some() {
                return Err(closed());
            }
            if let Ok(v) = &mut result {
                match method {
                    "functions.workflows.permissions.copy.list" => *v = m.copy.clone(),
                    "workflows.triggers.permissions.list" => *v = m.run.clone(),
                    "workflows.triggers.prefs.get" => *v = m.external.clone(),
                    _ => {}
                }
            }
            let serial = m.requests.len() + 1;
            m.requests.push(wire::row(
                serial,
                "read",
                method,
                fields,
                request.retry_safe(),
                witness,
                false,
            ));
            (serial, false)
        };
        Ok((model, serial, hold, result))
    })();
    Some(match prepared {
        Ok((model, serial, hold, result)) => deliver(model, serial, hold, result).await,
        Err(e) => Err(e),
    })
}

pub(crate) async fn mutation(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let (method, fields) = super::decode(request)?;
        wire::validate_mutation(&s, request, method, fields)?;
        let witness = owner(&s);
        let (serial, hold, result) = {
            let mut m = model.0.lock().unwrap();
            if !m.armed
                || m.held.is_some()
                || m.requests
                    .iter()
                    .any(|r| r["phase"] == "mutation" && r["method"] == method)
            {
                return Err(closed());
            }
            let rejected = case() == "permissions-rejected";
            if !rejected {
                m.copy["permission_type"] = json!("everyone");
            }
            let result = if rejected {
                Ok(json!({"ok":false,"error":"not_allowed"}))
            } else if case() == "permissions-unknown" {
                Err(Error::Transport(
                    "synthetic copy permission reply lost after dispatch".into(),
                ))
            } else {
                Ok(
                    json!({"ok":true,"permission_type":"everyone","opaque_receipt":{"preserve":[1,"copy"]}}),
                )
            };
            let hold = !m.held_once && case() == "permissions-retirement";
            let serial = m.requests.len() + 1;
            m.requests.push(wire::row(
                serial, "mutation", method, fields, false, witness, !rejected,
            ));
            (serial, hold, result)
        };
        let old = s.app_home.about.management.fixture.as_mut().unwrap();
        {
            let data = model.0.lock().unwrap();
            old.workflow = data.workflow.clone();
            old.trigger = data.trigger.clone();
        }
        old.requests.push(request.clone());
        old.writes += 1;
        Ok((model, serial, hold, result))
    })();
    Some(match prepared {
        Ok((model, serial, hold, result)) => deliver(model, serial, hold, result).await,
        Err(e) => Err(e),
    })
}
