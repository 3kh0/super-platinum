//! Closed Run access responses, preserving original dispatch and receiver custody.
use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
mod picker;
mod prerequisites;
mod raw;
mod requests;
mod seed;
#[cfg(test)]
mod tests;
mod wire;
pub(crate) use seed::seed;

pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_RUN_ACCESS_CASE";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn supported(value: &str) -> bool {
    matches!(
        value,
        "run-success"
            | "run-named-combined"
            | "run-rejected"
            | "run-unknown"
            | "run-followup-failure"
            | "run-retirement"
            | "run-cold-targets"
            | "run-everyone-external"
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
        && std::env::var(super::managers::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
        && supported(&case())
        && std::env::var(super::permissions::CASE_ENV)
            .unwrap_or_default()
            .is_empty()
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
    restriction: Value,
    seed: Option<Value>,
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
    let run = prerequisites::original_run()
        .unwrap_or_else(|| json!({"ok":true,"permission_type":"app_collaborators"}));
    let external = json!({"ok":true,"trippable_by_slack_connected_teams":{"hermes_triggers_trippable_by_slack_connected_teams_mcp":"NONE","allowed_teams":["TRUNEXTER"],"allowed_teams_with_3p_connectors":[],"restricted_teams":[]}});
    let restriction =
        json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false});
    let data = Data {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        credentials: serde_json::to_string(s.core.session.as_ref().unwrap()).unwrap(),
        original: json!({"workflow":m.workflow,"trigger":m.trigger,"copy":copy,"run":run,"external":external,"restriction":restriction}),
        workflow: m.workflow.clone(),
        trigger: m.trigger.clone(),
        copy,
        run,
        external,
        restriction,
        seed: None,
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
        .run_access = Some(Model(Arc::new(Mutex::new(data))));
}
fn closed() -> Error {
    Error::Api("workflow_management_not_sent_run_access_fixture".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !requested() {
        return Err(closed());
    }
    let m = s
        .app_home
        .about
        .management
        .fixture
        .as_ref()
        .and_then(|m| m.run_access.clone())
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
            "current":{"workflow":m.workflow,"trigger":m.trigger,"copy":m.copy,"run":m.run,"external":m.external,"restriction":m.restriction},"seed":m.seed,
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
            .ok_or("No original Run access response held")?;
        let receiver_retired = sender.is_closed();
        let v = json!({"serial":serial,"receiverRetired":receiver_retired,"delivered":sender.send(()).is_ok(),
            "response":m.responses.iter().find(|v|v["serial"]==serial).cloned()});
        m.releases.push(v.clone());
        Ok(v)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    model(s).map_err(|_| "Original Run access fixture unavailable".into())
}
pub(crate) fn arm(s: &ShellState) -> Result<(), String> {
    let m = capture(s)?;
    let mut m = m.0.lock().unwrap();
    if m.armed {
        return Err("Run access fixture already armed".into());
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
        .and_then(|m| m.run_access.as_ref())
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

pub(crate) use picker::picker;
pub(crate) use requests::{mutation, read};
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && model(s).is_ok()
}

pub(crate) fn prerequisite() -> bool {
    prerequisites::requested()
}
