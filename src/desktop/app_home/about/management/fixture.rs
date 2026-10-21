//! Closed, stateful server for the named offline workflow metadata scenes.
use super::*;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
pub(crate) mod check;
pub(crate) mod delete;
pub(crate) mod first_publish;
pub(crate) mod managers;
pub(crate) mod permissions;
pub(crate) mod publish;
pub(crate) mod run_access;
pub(crate) mod unpublish;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Success,
    TriggerRejected,
    ReplyLoss,
    ChangedPreflight,
    PolicyDenied,
    MissingAvailability,
}
#[derive(Clone)]
pub(super) struct Model {
    deletion: Option<delete::Model>,
    publication: Option<unpublish::Model>,
    publishing: Option<publish::Model>,
    first_publish: Option<first_publish::Model>,
    permissions: Option<permissions::Model>,
    run_access: Option<run_access::Model>,
    managers: Option<managers::Model>,
    workflow: Value,
    trigger: Value,
    mode: Mode,
    requests: Vec<PreparedRequest>,
    writes: usize,
    network: usize,
}
pub(super) fn enabled(s: &ShellState) -> bool {
    (crate::fixture::is_fixture() || cfg!(test))
        && s.app_home.about.management.fixture.is_some()
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .is_some_and(|h| h.fixture)
}
pub(crate) fn install(s: &mut ShellState, managed: bool, uncertain: bool) {
    if first_publish::requested() && s.core.active_account.is_none() {
        s.core.active_account = Some("synthetic-workflow-first-publish".into());
    }
    let managed = managed || managers::requested() && managers::case() == "managers-no-trigger";
    if managers::requested() && s.core.active_account.is_none() {
        s.core.active_account = Some("synthetic-workflow-managers".into());
    }
    if run_access::requested() && s.core.active_account.is_none() {
        s.core.active_account = Some("synthetic-workflow-run-access".into());
    }
    let old = s.core.active_team.clone().unwrap();
    let mut workspace = s.core.workspaces[&old].clone();
    let mut user = workspace.users[&workspace.self_user_id].clone();
    workspace.team_id = "TWORKFLOW".into();
    workspace.self_user_id = "UWORKFLOW".into();
    user.id = workspace.self_user_id.clone();
    user.is_restricted = false;
    user.is_ultra_restricted = false;
    workspace.users.insert(user.id.clone(), user);
    s.core.active_team = Some(workspace.team_id.clone());
    s.core
        .workspaces
        .insert(workspace.team_id.clone(), workspace);
    first_publish::confirmation::install_identity(s);
    super::super::fixture::install(s, false);
    let h = s.app_home.about.surface.as_mut().unwrap();
    let owner = h.owner.clone();
    let app = h.home.as_ref().unwrap().app.clone();
    h.info=Some(Info::decode(&app,json!({"ok":true,"app_id":app,"name":"Review requests","owner_ids":if managed{vec!["UOTHER"]}else{vec!["UWORKFLOW"]}})).unwrap());
    h.profile=Some(Profile::decode(&app,json!({"ok":true,"app_profile":{"id":app,"team_id":owner.ctx.session.team_id,"is_installed":true,"is_workflow_app":true}})).unwrap());
    h.connection = None;
    s.app_home.about.management.fixture = Some(Model {
        deletion: None,
        publication: None,
        publishing: None,
        first_publish: None,
        permissions: None,
        run_access: None,
        managers: None,
        workflow: json!({"id":"WF_REVIEW","app_id":app,"title":"Review requests","description":"Collect the details before a team review.",
            "is_published":false,"is_empty":false,"date_updated":1,"collaborators":[owner.ctx.session.user_id],
            "steps":[{"id":"STEP_REVIEW","function":{"id":"FN_REVIEW","product_level_availability":{"is_available":true}},"inputs":{"message":{"value":"Review this"}}}],"retained_unknown":42}),
        trigger: json!({"id":"TR_REVIEW","type":"shortcut","name":"Review requests","description":"Collect the details before a team review.",
            "workflow":{"workflow_id":"WF_REVIEW","app_id":app,"app":{"id":app}},"inputs":{},"available_data":{},"retained_unknown":19}),
        mode: if uncertain {
            Mode::ReplyLoss
        } else {
            Mode::Success
        },
        requests: Vec::new(),
        writes: 0,
        network: 0,
    });
    delete::install(s);
    unpublish::install(s);
    publish::install(s);
    permissions::install(s);
    run_access::install(s);
    managers::install(s);
    first_publish::install(s);
}
fn refused() -> Error {
    Error::Api("workflow_management_not_sent_fixture".into())
}
fn decode(request: &PreparedRequest) -> Result<(&str, &[(String, String)]), Error> {
    if request.method != "POST" || !request.url.starts_with("https://example.test/api/") {
        return Err(refused());
    }
    let method = request
        .url
        .trim_start_matches("https://example.test/api/")
        .split('?')
        .next()
        .ok_or_else(refused)?;
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    Ok((method, fields))
}
pub(super) fn response(s: &mut ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    let owner = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let (method, fields) = decode(request)?;
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    m.requests.push(request.clone());
    let mut result = match method {
        "experiments.getByUser" => {
            json!({"ok":true,"assignments":{"slackforce_pnp_wfb":{"group":"on"},"wfb_hermes_trials":{"group":"off"}}})
        }
        "client.init" => {
            json!({"ok":true,"team":{"id":owner.ctx.session.team_id,"plan":"paid"},"self":{"id":owner.ctx.session.user_id,"is_restricted":false},"workspaces":[{"id":owner.ctx.session.team_id,"plan":"paid"}]})
        }
        "features.access.policies.list" => {
            json!({"ok":true,"feature_policies":{"hermes__read":{"is_enabled":true},"hermes__write":{"is_enabled":m.mode!=Mode::PolicyDenied}}})
        }
        "client.extras" => {
            json!({"ok":true,"team_prefs":{"toplevel_prefs":{"workflow_builder_enabled":true,"who_can_create_workflows":"regular"},"workspace_prefs":{}}})
        }
        "functions.workflows.list" => {
            let filters: Value = serde_json::from_str(field("filter_options").ok_or_else(refused)?)
                .map_err(|_| refused())?;
            let found = m.workflow["title"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains(&filters["name"].as_str().unwrap_or("").to_lowercase())
                && filters
                    .get("is_published")
                    .is_none_or(|v| v == &m.workflow["is_published"]);
            json!({"ok":true,"workflows":if found{vec![m.workflow.clone()]}else{vec![]},"response_metadata":{"next_cursor":""}})
        }
        "functions.workflows.getLatest" => {
            if field("workflow_id") != Some("WF_REVIEW") {
                return Err(refused());
            }
            let mut workflow = m.workflow.clone();
            if m.mode == Mode::ChangedPreflight {
                workflow["retained_unknown"] = json!(43);
            }
            if m.mode == Mode::MissingAvailability {
                workflow["steps"][0]["function"]["product_level_availability"] = Value::Null;
            }
            json!({"ok":true,"decorated_workflow":workflow,"available_data":{},"expression_data":{}})
        }
        "functions.workflows.getIsRestricted" => {
            json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false})
        }
        "workflows.triggers.list" => json!({"ok":true,"triggers":[m.trigger]}),
        "workflows.triggers.info" => json!({"ok":true,"trigger":m.trigger}),
        "workflows.triggers.prefs.get" => {
            json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]}})
        }
        "functions.workflows.permissions.copy.list" => {
            json!({"ok":true,"permission_type":"collaborators"})
        }
        "workflows.triggers.permissions.list" => {
            json!({"ok":true,"permission_type":"app_collaborators"})
        }
        _ => return Err(refused()),
    };
    if method == "experiments.getByUser" {
        activity::export::fixture::decorate_policy(s, &mut result);
    }
    Ok(result)
}
pub(super) fn mutation(s: &mut ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    let (method, fields) = decode(request)?;
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    if request.retry_safe() {
        return Err(refused());
    }
    m.requests.push(request.clone());
    m.writes += 1;
    match method {
        "functions.workflows.update" if field("workflow_id") == Some("WF_REVIEW") => {
            m.workflow["title"] = json!(field("title").ok_or_else(refused)?);
            m.workflow["description"] = json!(field("description").ok_or_else(refused)?);
        }
        "workflows.triggers.update" if field("trigger_id") == Some("TR_REVIEW") => {
            if m.mode == Mode::TriggerRejected {
                return Ok(json!({"ok":false,"error":"not_allowed"}));
            }
            m.trigger["name"] = json!(field("name").ok_or_else(refused)?);
            m.trigger["description"] = json!(field("description").ok_or_else(refused)?);
        }
        _ => return Err(refused()),
    }
    if m.mode == Mode::ReplyLoss {
        Err(Error::Transport(
            "synthetic reply lost after dispatch".into(),
        ))
    } else {
        Ok(json!({"ok":true}))
    }
}
pub(crate) fn set_mode(s: &mut ShellState, mode: Mode) {
    if let Some(m) = s.app_home.about.management.fixture.as_mut() {
        m.mode = mode;
    }
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let book = &s.app_home.about.management;
    let h = book.surface.as_ref();
    let d = h.and_then(|h| document(s, h.id));
    json!({"route":if d.is_some(){"editor"}else if h.is_some(){"managed"}else{"about"},
        "visit":h.map(|h|h.id),"busy":h.is_some_and(|h|h.read.is_some()),
        "title":d.map(|d|d.title.as_str()),"description":d.map(|d|d.description.as_str()),"dirty":d.is_some_and(Document::dirty),
        "pending":d.is_some_and(|d|d.pending.is_some()),"observed":d.is_some_and(|d|d.observed.is_some()),
        "notice":d.and_then(|d|d.notice.as_ref()),"outcomes":h.map(|h|write::outcomes(s,h.id)),
        "writes":book.fixture.as_ref().map(|m|m.writes),"reads":book.fixture.as_ref().map(|m|m.requests.len()-m.writes),
        "networkCalls":book.fixture.as_ref().map(|m|m.network),"retainedDocuments":book.documents.len(),"retainedAttempts":book.attempts.len()})
}
pub(crate) fn replace_account(state: Signal<ShellState>) -> Result<Value, String> {
    let (before, media, epoch, model) = {
        let s = state.peek();
        if !enabled(&s) {
            return Err("Account replacement requires the disconnected workflow fixture".into());
        }
        (
            snapshot(&s),
            s.media.clone(),
            s.core.account_epoch,
            s.app_home.about.management.fixture.clone().unwrap(),
        )
    };
    let mut loaded = ShellState::fixture(media);
    loaded.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    loaded.core.account_epoch = epoch + 1;
    install(&mut loaded, false, false);
    loaded.app_home.about.management.fixture = Some(model);
    crate::bootstrap::install_loaded(state, loaded);
    let s = state.peek();
    let retained = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .filter(|d| d.pending.is_some())
        .count();
    Ok(
        json!({"before":before,"after":snapshot(&s),"previousEpoch":epoch,"currentEpoch":s.core.account_epoch,
        "retainedPending":retained,"foreignDocumentHidden":!showing(&s)}),
    )
}
