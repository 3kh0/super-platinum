//! Opt-in replacement for the info HTTP response only. Production reducers run.
//! Never constructs a Transport or uses a persisted session. Pending responses
//! are released by the checker; it cannot directly call finish or mutate rows.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{config::WorkspaceSession, slack::client::RequestBody};

struct Pending {
    request: Value,
    response: Value,
    sender: Option<tokio::sync::oneshot::Sender<Value>>,
    decoded: bool,
    applied: Option<bool>,
}
#[derive(Default)]
struct Ledger {
    requests: Vec<Pending>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_HYDRATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-hydration" | "slack-list-hydration-narrow")
        )
}
fn isolated(shell: &ShellState) -> bool {
    check_enabled()
        && fixture::enabled(shell)
        && shell.core.transport.is_none()
        && shell.core.session.is_none()
}
pub(crate) fn begin(shell: &ShellState) -> Result<(), String> {
    if !isolated(shell) {
        return Err("Exact disconnected hydration fixture required".into());
    }
    let mut ledger = ledger().lock().unwrap();
    if !ledger.requests.is_empty() {
        return Err("Hydration fixture ledger already used".into());
    }
    ledger.requests = Vec::new();
    Ok(())
}
fn raw_response(attempt: &Attempt, index: usize) -> Value {
    let (title, child) = match index {
        0 => ("Hydrated project parent", "R_HYDRATE_CHILD"),
        1 => ("STALE response must not appear", "R_HYDRATE_STALE"),
        _ => ("Hydrated project parent again", "R_HYDRATE_SECOND"),
    };
    let mut parent = attempt.page.record(&attempt.parent).unwrap().raw.clone();
    parent["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|field| field["key"] == "NAME")
        .unwrap()["text"] = json!(title);
    json!({"ok":true,"list":attempt.document.raw,"record":parent,
        "subtasks":[{"id":child,"list_id":attempt.list,"parent_record_id":attempt.parent,
            "position":if index==0 {"0000000002"}else{"0000000003"},
            "view_positions":{"V_FUTURE":"preserved"},"opaque":{"keep":true},
            "date_created":99+index,"created_by":"U1","fields":[
                {"key":"NAME","text":if index==0 {"Hydrated first subtask"}else if index==1 {"STALE child"}else{"Hydrated second subtask"}},
                {"key":"done","checkbox":false}]}]})
}
pub(super) async fn execute(
    state: Signal<ShellState>,
    attempt: &Attempt,
) -> Result<ListRecordHydration, Error> {
    if !isolated(&state.peek()) || !attempt.dispatch_ready(&state.peek()) {
        return Err(Error::Api("hydration_fixture_not_current".into()));
    }
    let session = WorkspaceSession {
        team_id: attempt.scope.team.clone(),
        user_id: state.peek().self_account.user_id.clone(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_record_subtasks(
        &state.peek().core.client,
        &session,
        &attempt.list,
        &attempt.parent,
    );
    let RequestBody::Form(fields) = request.body else {
        return Err(Error::Api("fixture_expected_form".into()));
    };
    if request.method != "POST"
        || !url::Url::parse(&request.url).is_ok_and(|url| {
            url.host_str() == Some("example.test") && url.path() == "/api/lists.records.info"
        })
        || fields
            != vec![
                ("token".into(), "fixture-not-a-token".into()),
                ("list_id".into(), "F_LIST_FIXTURE".into()),
                ("id".into(), "R_HYDRATE_PARENT".into()),
                ("include_is_subscribed".into(), "true".into()),
                ("_x_reason".into(), "get-record-subtasks".into()),
            ]
    {
        return Err(Error::Api("fixture_actual_request_mismatch".into()));
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let index = {
        let mut ledger = ledger().lock().unwrap();
        let index = ledger.requests.len();
        if index >= 3 {
            return Err(Error::Api("fixture_unexpected_extra_request".into()));
        }
        ledger.requests.push(Pending {request:json!({"method":request.method,"url":request.url,"fields":fields,
            "open":attempt.open,"detail":attempt.detail,"revision":attempt.revision,"serial":attempt.serial}),
            response:raw_response(attempt,index),sender:Some(sender),decoded:false,applied:None});
        index
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(60), receiver)
        .await
        .map_err(|_| Error::Transport("fixture response timeout".into()))?
        .map_err(|_| Error::Transport("fixture response lost".into()))?;
    // The same public decoder used by fetch_list_record_subtasks receives raw
    // server-shaped data; production finish decides whether it may be merged.
    let result = ListRecordHydration::from_response(&attempt.list, &attempt.parent, &raw)
        .map_err(|error| Error::Api(error.to_string()));
    ledger().lock().unwrap().requests[index].decoded = true;
    result
}
pub(crate) fn release(shell: &ShellState, index: usize) -> Result<(), String> {
    if !isolated(shell) {
        return Err("Fixture isolation lost".into());
    }
    let mut ledger = ledger().lock().unwrap();
    let pending = ledger
        .requests
        .get_mut(index)
        .ok_or("Missing actual request")?;
    pending
        .sender
        .take()
        .ok_or("Response already released")?
        .send(pending.response.clone())
        .map_err(|_| "Actual request receiver disappeared".into())
}
pub(super) fn completed(serial: u64, applied: bool) {
    let mut ledger = ledger().lock().unwrap();
    if let Some(request) = ledger
        .requests
        .iter_mut()
        .find(|p| p.request["serial"] == serial)
    {
        if request.applied.is_none() {
            request.applied = Some(applied);
        }
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if !isolated(shell) {
        return Err("Fixture isolation lost".into());
    }
    let ledger = ledger().lock().unwrap();
    Ok(
        json!({"attempts":ledger.requests.iter().map(|p|p.request.clone()).collect::<Vec<_>>(),
        "responses":ledger.requests.iter().map(|p|p.response.clone()).collect::<Vec<_>>(),
        "decoded":ledger.requests.iter().map(|p|p.decoded).collect::<Vec<_>>(),
        "applied":ledger.requests.iter().map(|p|p.applied).collect::<Vec<_>>(),
        "released":ledger.requests.iter().map(|p|p.sender.is_none()).collect::<Vec<_>>()}),
    )
}
