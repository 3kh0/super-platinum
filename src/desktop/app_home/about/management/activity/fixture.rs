//! Closed Activity endpoint. All raw replies still pass through production decoders.
use super::*;
use std::sync::{Arc, Mutex};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
pub(crate) mod channel;
pub(crate) mod check;
pub(crate) mod file;
pub(crate) mod mixed;
#[cfg(test)]
mod tests;
pub(crate) mod users;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_WORKFLOW_ACTIVITY_CHECK";
pub(crate) const TITLE: &str = "Révision retained for Activity";
pub(crate) const DESCRIPTION: &str = "Keep these unsaved details.\nReview 中🙂 e\u{301} together.";
pub(crate) const FIRST: &str = "E_REVIEW";
pub(crate) const SECOND: &str = "E_SECOND";
pub(crate) const CHILD: &str = "E_CHILD";
const CURSOR: &str = "activity+/page=2";

#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
#[derive(Default)]
struct Ledger {
    file: Option<file::Model>,
    channel: Option<channel::Model>,
    users: Option<users::Model>,
    requests: Vec<Value>,
    mode: Mode,
    held: Option<(u64, oneshot::Sender<Result<Value, Error>>, Value)>,
    finished: usize,
    cancelled: usize,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Malformed,
    Empty,
    Held,
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_WORKFLOW_ACTIVITY_CASE").unwrap_or_default()
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    crate::fixture::is_fixture()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("workflow-activity"
                | "workflow-activity-narrow"
                | "workflow-activity-members"
                | "workflow-activity-members-narrow")
        )
        && matches!(
            case().as_str(),
            "browse"
                | "mixed-navigation"
                | "mixed-navigation-reverse"
                | "mixed-navigation-success"
                | "file"
                | "file-retirement"
                | "file-emoji"
                | "file-emoji-retirement"
                | "file-emoji-cold"
                | "file-encoded"
                | "file-recovery"
                | "file-recovery-retirement"
                | "date-range"
                | "date-range-retirement"
                | "retirement"
                | "picker"
                | "channel"
                | "channel-held-success"
                | "channel-special"
                | "channel-shared-frozen"
                | "channel-frozen-grace"
                | "user-real"
                | "user-display"
                | "user-refresh"
                | "user-deactivated"
                | "export"
                | "export-retirement"
                | "execution"
                | "execution-failure"
                | "execution-retirement"
                | "execution-thread"
                | "execution-pins"
                | "execution-focused"
                | "execution-thread-retirement"
                | "execution-pins-retirement"
                | "execution-thread-replacement"
                | "execution-pin-removal"
        )
        && s.app_home.about.management.activity_fixture.is_some()
}
pub(crate) fn install(s: &mut ShellState) {
    install_case(s, &case());
}
fn install_case(s: &mut ShellState, scenario: &str) {
    if matches!(
        scenario,
        "file"
            | "file-retirement"
            | "file-emoji"
            | "file-emoji-retirement"
            | "file-emoji-cold"
            | "file-encoded"
            | "file-recovery"
            | "file-recovery-retirement"
    ) || mixed::is_case(scenario)
    {
        s.core.active_account = Some(file::ACCOUNT.into());
    }
    if scenario.starts_with("user-") {
        s.core.active_account = Some("synthetic-workflow-activity-user".into());
    }
    if matches!(
        scenario,
        "channel"
            | "channel-held-success"
            | "channel-special"
            | "channel-shared-frozen"
            | "channel-frozen-grace"
    ) {
        // Real tab history requires account custody. Bind it before About and
        // management capture their owners; a non-UUID cannot persist history.
        s.core.active_account = Some(
            if channel::warm::requested() {
                channel::warm::ACCOUNT
            } else {
                "synthetic-workflow-activity-channel"
            }
            .into(),
        );
    }
    super::super::fixture::install(s, false, scenario == "retirement");
    s.app_home.about.management.activity_fixture = Some(Model::default());
    if matches!(
        scenario,
        "file"
            | "file-retirement"
            | "file-emoji"
            | "file-emoji-retirement"
            | "file-emoji-cold"
            | "file-encoded"
            | "file-recovery"
            | "file-recovery-retirement"
    ) || mixed::is_case(scenario)
    {
        file::install(s);
    }
    if scenario.starts_with("execution") {
        super::super::entry::fixture::install(s);
    }
    if scenario.starts_with("export") {
        export::fixture::install(s);
    }
    if matches!(
        scenario,
        "channel"
            | "channel-held-success"
            | "channel-special"
            | "channel-shared-frozen"
            | "channel-frozen-grace"
    ) || mixed::is_case(scenario)
    {
        channel::install(s, scenario == "channel-special");
    }
    if scenario.starts_with("user-") {
        users::install(s);
    }
    let ready = "https://fixture.invalid/activity-ready.png";
    let cold = "https://fixture.invalid/activity-cold.png";
    if let Some(workspace) = s.core.workspaces.get_mut("TWORKFLOW") {
        workspace.apply_emojis(vec![
            serde_json::from_value(json!({"name":"activity_ready","value":ready})).unwrap(),
            serde_json::from_value(json!({"name":"activity_cold","value":cold})).unwrap(),
        ]);
        if file::emoji_case(scenario) {
            // Same local ready asset, through the real alias resolver. The
            // custom wave collision must lose to Activity's standard catalog.
            workspace.apply_emojis(vec![
                serde_json::from_value(
                    json!({"name":"activity_ready_alias","value":"alias:activity_ready"}),
                )
                .unwrap(),
                serde_json::from_value(json!({"name":"wave","value":ready})).unwrap(),
            ]);
        }
    }
    let id = s.media.register_emoji("activity_ready", ready);
    s.media.insert(
        id,
        "image/png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/png/icon_16.png"
        ))
        .as_slice(),
    );
}
fn refused() -> Error {
    Error::Api("workflow_activity_not_sent_fixture".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    s.app_home
        .about
        .management
        .activity_fixture
        .clone()
        .ok_or_else(refused)
}
pub(crate) fn mode(s: &ShellState, mode: Mode) -> Result<(), String> {
    model(s)
        .map_err(|_| "Activity fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .mode = mode;
    Ok(())
}
fn fields(request: &PreparedRequest) -> Result<(&str, Value), Error> {
    // Generic transport retry classification is independent of this closed
    // read-only endpoint allowlist. Actual Activity plans are not retry-safe.
    if request.method != "POST" {
        return Err(refused());
    }
    let endpoint = request
        .url
        .strip_prefix("https://example.test/api/")
        .ok_or_else(refused)?;
    // The real rest_form producer appends per-request client metadata. It is
    // separate from the endpoint path and the source Activity form arguments.
    let (method, query) = endpoint.split_once('?').ok_or_else(refused)?;
    if query.split('&').any(|pair| {
        pair.split_once('=').is_none_or(|(key, _)| {
            ![
                "_x_id",
                "_x_csid",
                "_x_desktop_ia",
                "_x_foreground",
                "_x_frontend_build_type",
                "_x_gantry",
                "_x_num_retries",
                "_x_version_ts",
                "fp",
                "slack_route",
            ]
            .contains(&key)
        })
    }) {
        return Err(refused());
    }
    if !matches!(method, "workflows.activity.list" | "workflows.activity.get") {
        return Err(refused());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    let mut args = serde_json::Map::new();
    for (key, value) in fields {
        if key == "token" {
            continue;
        }
        args.insert(key.clone(), json!(value));
    }
    if args.get("workflow_id") != Some(&json!("WF_REVIEW")) {
        return Err(refused());
    }
    let reason = if method == "workflows.activity.list" {
        "multi-click-activity-list-fetcher"
    } else {
        "Load instance steps for activity tab"
    };
    if args.get("_x_reason") != Some(&json!(reason)) {
        return Err(refused());
    }
    Ok((method, Value::Object(args)))
}
fn event(id: &str, status: &str) -> Value {
    json!({"workflow_id":"WF_REVIEW","workflow_execution_id":id,
      "workflow_instance_status":status,"workflow_status":status,
      "workflow_run_id":format!("RUN_{id}"),"actor_id":"UWORKFLOW","last_updated":1781524800,
      "last_activity":"Review details collected.","child_execution_details":{"total_count":if id==FIRST{1}else{0},"error_count":0},
      "future":{"preserved":true}})
}
fn reply(method: &str, args: &Value, empty: bool) -> Result<Value, Error> {
    if method == "workflows.activity.get" {
        let id = args["workflow_execution_id"].as_str().ok_or_else(refused)?;
        if ![FIRST, SECOND, CHILD].contains(&id) {
            return Err(refused());
        }
        let mut row = event(id, if id == SECOND { "ERROR" } else { "SUCCESS" });
        // A detail observation may differ from the originally selected list row.
        if id == FIRST {
            row["workflow_status"] = json!("PENDING");
        }
        row["trace_id"] = json!("synthetic-review-trace");
        row["started"] = json!(1781524800);
        row["trigger_message"] = json!({
            "template":"Prefix :wave: {wave} :activity_ready: :activity_cold:",
            "variables":{"wave":{"type":"EMOJI","value":":wave:"}}
        });
        row["step_events"] = json!([{"step_execution_id":"STEP_REVIEW","function_id":"FN_REVIEW",
          "last_updated":1781524800000_i64,"step_status":"SUCCESS",
          "outcome_message":{
            "template":"L<@U1><b>x</b>{unknown}{no}{zero} \u{4e2d}\u{1f642} <@U1><i>y</i> :wave:",
            "variables":{"unknown":{"type":"FUTURE","value":"ignored"},"no":false,"zero":0}
          },"future":{"preserved":17}}]);
        if matches!(
            case().as_str(),
            "channel"
                | "channel-held-success"
                | "channel-special"
                | "channel-shared-frozen"
                | "channel-frozen-grace"
        ) {
            row["trigger_message"] = json!({"template":"Open {known} then {known}","variables":{"known":{"type":"CHANNEL","value":channel::KNOWN}}});
            row["step_events"][0]["outcome_message"] = json!({"template":"Check {missing}","variables":{"missing":{"type":"CHANNEL","value":channel::MISSING}}});
            if channel::shared_frozen::requested() {
                channel::shared_frozen::messages(&mut row);
            }
            if case() == "channel-special" {
                channel::special_messages(&mut row);
            }
        }
        if case().starts_with("user-") {
            users::messages(&mut row);
        }
        if file::requested() {
            file::messages(&mut row);
        }
        if mixed::is_case(&case()) {
            mixed::messages(&mut row);
        }
        return Ok(json!({"ok":true,"workflow_event":row}));
    }
    let limit = args["limit"]
        .as_str()
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(refused)?;
    if ![20, 50, 75, 100].contains(&limit) {
        return Err(refused());
    }
    let filters: Value = serde_json::from_str(args["filters"].as_str().ok_or_else(refused)?)
        .map_err(|_| refused())?;
    if !filters.is_object() {
        return Err(refused());
    }
    let parent = args["parent_workflow_execution_id"].as_str();
    if parent.is_some_and(|id| id != FIRST) {
        return Err(refused());
    }
    let cursor = args["cursor"].as_str();
    if cursor.is_some_and(|id| id != CURSOR) {
        return Err(refused());
    }
    let rows = if empty {
        vec![]
    } else if parent.is_some() {
        vec![event(CHILD, "SUCCESS")]
    } else if cursor.is_some() {
        vec![event(SECOND, "ERROR")]
    } else {
        vec![event(FIRST, "SUCCESS")]
    };
    Ok(
        json!({"ok":true,"workflow_events":rows,"response_metadata":{"next_cursor":if !empty&&parent.is_none()&&cursor.is_none(){CURSOR}else{""}}}),
    )
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = model(&state.peek())?;
    let (method, args) = fields(request)?;
    let receiver = {
        let mut m = model.0.lock().unwrap();
        if m.held.is_some() {
            return Err(refused());
        }
        let serial = m.requests.len() as u64 + 1;
        m.requests
            .push(json!({"serial":serial,"method":method,"args":args}));
        let mode = std::mem::take(&mut m.mode);
        if matches!(mode, Mode::Held) {
            let (send, recv) = oneshot::channel();
            m.held = Some((serial, send, reply(method, &args, false)?));
            Some(recv)
        } else {
            m.finished += 1;
            return if matches!(mode, Mode::Malformed) {
                Ok(json!({"ok":true,"workflow_events":"malformed"}))
            } else {
                reply(method, &args, matches!(mode, Mode::Empty))
            };
        }
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
        model: model.clone(),
        completed: false,
    };
    let result = receiver.unwrap().await.map_err(|_| refused())?;
    finish.completed = true;
    result
}
pub(crate) fn release(s: &ShellState) -> Result<Value, String> {
    let model = model(s).map_err(|_| "Activity fixture unavailable")?;
    let (_, sender, raw) = model
        .0
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held Activity request")?;
    let delivered = sender.send(Ok(raw)).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":!delivered}))
}
pub(crate) fn replace_account(state: Signal<ShellState>) -> Result<Value, String> {
    let saved = model(&state.peek()).map_err(|_| "Activity fixture unavailable")?;
    let proof = super::super::fixture::replace_account(state)?;
    let mut state = state;
    state.write().app_home.about.management.activity_fixture = Some(saved);
    Ok(proof)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let metadata = super::super::fixture::snapshot(s);
    let activity = s.app_home.about.management.surface.as_ref()
        .and_then(|h|super::super::document(s,h.id)).map(|d| {
        let a=&d.activity;
        json!({"active":a.active,"documentRevision":d.revision,"filters":a.filters,
          "page":a.page.as_ref().map(|p|p.raw()),"query":a.page.as_ref().map(|p|p.query().request().arguments().clone()),
          "previous":a.previous.iter().map(|q|q.request().arguments().clone()).collect::<Vec<_>>(),
          "detail":a.detail.as_ref().map(|d|d.raw()),"detailRow":a.detail_row,"breadcrumbs":a.breadcrumbs,"notice":a.notice,"nameNotice":a.name_notice,"nameRefreshPending":a.name_refresh_read.is_some()})
    });
    let ledger = s.app_home.about.management.activity_fixture.as_ref().map(|m| {
        let m=m.0.lock().unwrap();
        json!({"requests":m.requests,"finished":m.finished,"cancelled":m.cancelled,"held":m.held.as_ref().map(|(id,_,_)|id),"networkCalls":0})
    });
    json!({"metadata":metadata,"activity":activity,"ledger":ledger,"account":s.core.active_account,"epoch":s.core.account_epoch,"export":export::snapshot(s),"exportFixture":export::fixture::snapshot(s),"entry":super::super::entry::snapshot(s)})
}

/// Closed policy response seam; the caller retains original Read custody.
pub(crate) async fn name_request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    users::request(state, request).await
}
