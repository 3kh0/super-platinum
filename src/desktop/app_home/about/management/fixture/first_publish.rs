//! Closed first-Publish server. Receipts belong to the original account and
//! remain inspectable after retirement; no synthetic call reaches a network.
use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
mod avatar;
pub(crate) mod confirmation;
mod endpoint;
mod picker;
mod seed;
pub(crate) use seed::seed;

pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_FIRST_PUBLISH_CASE";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_WORKFLOW_MANAGEMENT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("workflow-editor" | "workflow-editor-narrow")
        )
        && supported(&case())
        && [
            delete::CASE_ENV,
            unpublish::CASE_ENV,
            publish::CASE_ENV,
            permissions::CASE_ENV,
            run_access::CASE_ENV,
            managers::CASE_ENV,
        ]
        .iter()
        .all(|key| std::env::var(key).unwrap_or_default().is_empty())
}
pub(crate) fn supported(case: &str) -> bool {
    matches!(
        case,
        "first-default"
            | "first-success"
            | "first-settings"
            | "first-metadata-rejected"
            | "first-settings-rejected-held"
            | "first-primary-unknown"
            | "first-stale-readback"
            | "first-cancel"
            | "first-retirement"
            | "first-avatar"
            | "first-avatar-crop-retry"
            | "first-avatar-crop-cancel"
            | "first-avatar-upload-back"
            | "first-avatar-upload-retry"
            | "first-avatar-upload-cancel-held"
            | "first-avatar-crop-back"
            | "first-avatar-crop-drag"
            | "first-avatar-crop-drag-retry"
            | "first-avatar-crop-back-held"
            | "first-avatar-crop-cancel-held"
            | "first-add-no-selection"
            | "first-add-success"
            | "first-add-duplicate"
            | "first-add-rejected"
            | "first-add-retirement"
            | "first-add-unavailable"
            | "first-tab-success"
            | "first-tab-empty"
            | "first-tab-failure"
            | "first-tab-retirement"
            | "first-tab-rate-limit"
            | "first-details-success"
            | "first-details-errors"
            | "first-details-close-held"
            | "first-details-supersession"
            | "first-details-account-retirement"
            | "first-details-rate-limit"
    )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested()
        && s.app_home
            .about
            .management
            .fixture
            .as_ref()
            .is_some_and(|m| m.first_publish.is_some())
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
    seed: Option<Value>,
    armed: bool,
    confirmation_add: Option<confirmation::Model>,
    channel_workflows: Option<crate::channel_workflows::fixture::Model>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
}
pub(super) fn install(s: &mut ShellState) {
    if !requested() {
        return;
    }
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    m.workflow["id"] = json!("WfREVIEW01");
    m.workflow["is_published"] = json!(false);
    m.workflow
        .as_object_mut()
        .unwrap()
        .remove("last_published_version_id");
    m.workflow["unpublished_change_count"] = json!(0);
    m.workflow["input_parameters"] = json!({});
    m.workflow["steps"][0]["function"]["type"] = json!("builtin");
    m.workflow["steps"][0]["function"]["input_parameters"] = json!([]);
    m.workflow["steps"][0]["errors"] = json!([]);
    m.trigger["workflow"]["workflow_id"] = json!("WfREVIEW01");
    m.trigger["trigger_type_id"] = json!("TT_SHORTCUT");
    m.trigger["shortcut_url"] = json!("https://slack.com/shortcuts/FIRSTFIXTURE");
    let nondefault = matches!(
        case().as_str(),
        "first-settings" | "first-settings-rejected-held"
    );
    if nondefault {
        m.workflow["collaborators"] = json!(["UWORKFLOW", "UMANOLD01"]);
    }
    let copy =
        json!({"ok":true,"permission_type":if nondefault {"everyone"} else {"collaborators"}});
    let run =
        json!({"ok":true,"permission_type":if nondefault {"app_collaborators"} else {"everyone"}});
    let data = Data {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        credentials: serde_json::to_string(s.core.session.as_ref().unwrap()).unwrap(),
        original: json!({"workflow":m.workflow,"trigger":m.trigger,"copy":copy,"run":run}),
        workflow: m.workflow.clone(),
        trigger: m.trigger.clone(),
        copy,
        run,
        seed: None,
        armed: false,
        confirmation_add: confirmation::requested().then(confirmation::Model::new),
        channel_workflows: crate::channel_workflows::fixture::is_case(&case())
            .then(crate::channel_workflows::fixture::Model::new),
        requests: vec![],
        responses: vec![],
        settlements: vec![],
        releases: vec![],
        held: vec![],
    };
    m.first_publish = Some(Model(Arc::new(Mutex::new(data))));
}
fn closed() -> Error {
    Error::Api("workflow_management_not_sent_first_publish_fixture".into())
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
        .and_then(|m| m.first_publish.clone())
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
        let mut value = json!({"original":m.original,"current":{"workflow":m.workflow,"trigger":m.trigger,"copy":m.copy,"run":m.run},
            "seed":m.seed,"requests":m.requests,"responses":m.responses,"settlements":m.settlements,"releases":m.releases,
            "held":m.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),"networkCalls":0,
            "owner":{"account":m.account,"epoch":m.epoch,"team":"TWORKFLOW","user":"UWORKFLOW"}});
        if let Some(add) = &m.confirmation_add {
            value["confirmationAdd"] = add.snapshot();
        }
        if let Some(tab) = &m.channel_workflows {
            value["channelWorkflows"] = tab.snapshot();
        }
        value
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let i = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("No original response held")?;
        let (_, tx) = m.held.remove(i);
        let retired = tx.is_closed();
        let receipt =
            json!({"serial":serial,"receiverRetired":retired,"delivered":tx.send(()).is_ok()});
        m.releases.push(receipt.clone());
        Ok(receipt)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    model(s).map_err(|_| "Original first-Publish fixture absent".into())
}
pub(crate) fn tab_model(s: &ShellState) -> Option<crate::channel_workflows::fixture::Model> {
    s.app_home
        .about
        .management
        .fixture
        .as_ref()?
        .first_publish
        .as_ref()?
        .0
        .lock()
        .ok()?
        .channel_workflows
        .clone()
}
pub(crate) fn tab_request(
    request: &PreparedRequest,
    expected: &PreparedRequest,
) -> Result<(), Error> {
    endpoint::same(request, expected)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    s.app_home
        .about
        .management
        .fixture
        .as_ref()
        .and_then(|m| m.first_publish.as_ref())
        .map(|m| {
            let mut v = m.snapshot();
            v["ownerCurrent"] = json!(m.current(s));
            v
        })
        .unwrap_or(Value::Null)
}
pub(crate) fn arm(s: &ShellState) -> Result<(), String> {
    let m = capture(s)?;
    let mut m = m.0.lock().unwrap();
    if m.armed {
        return Err("First-Publish fixture already armed".into());
    }
    m.armed = true;
    Ok(())
}
fn receipt(result: &Result<Value, Error>) -> Value {
    match result {
        Ok(v) => json!({"raw":v}),
        Err(Error::Transport(_)) => json!({"error":"transport"}),
        Err(Error::Offline(_)) => json!({"error":"offline"}),
        Err(Error::Api(v)) => json!({"error":v}),
        Err(_) => json!({"error":"other"}),
    }
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
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
    let rx = {
        let mut m = model.0.lock().unwrap();
        m.responses
            .push(json!({"serial":serial,"result":receipt(&result)}));
        if hold {
            let (tx, rx) = oneshot::channel();
            m.held.push((serial, tx));
            Some(rx)
        } else {
            None
        }
    };
    let mut settlement = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = rx {
        tokio::time::timeout(Duration::from_secs(45), rx)
            .await
            .map_err(|_| closed())?
            .map_err(|_| closed())?;
    }
    settlement.returned = true;
    result
}
pub(crate) use avatar::avatar;
pub(crate) use endpoint::{mutation, read};
pub(crate) use picker::picker;
