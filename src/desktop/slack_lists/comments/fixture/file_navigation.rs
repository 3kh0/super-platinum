//! Closed target reads for whole-file navigation. Never installs a target view.
use super::*;
use super_platinum_core::slack::api;
mod canvas;
#[cfg(test)]
mod tests;

pub(crate) const CANVAS: &str = completion::files::CANVAS;
pub(crate) const TARGET_LIST: &str = completion::files::LIST;
pub(crate) const RECEIVED_TS: &str = "1781524801.000001";
pub(crate) const LIST_CONTENT: &str = "Native target List row";
pub(crate) const CANVAS_CONTENT: &str = "Native target Canvas content";

pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("file-navigation"
                | "file-navigation-custody"
                | "cold-files"
                | "cold-files-recovery")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell) && completion::enabled(shell)
}
#[derive(Default)]
pub(super) struct State {
    received: Value,
    requests: Vec<Value>,
    returns: usize,
}
fn refused() -> Error {
    Error::Offline("Closed file navigation reader unavailable".into())
}
fn model(shell: &ShellState) -> Result<Model, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    shell
        .slack_lists
        .comments
        .fixture
        .clone()
        .ok_or_else(refused)
}
pub(crate) fn received_message() -> Value {
    json!({"ts":RECEIVED_TS,"thread_ts":ROOT,"user":completion::members::FIRST,
    "text":"Open the existing Canvas and List.","navigation_opaque":{"keep":"original received comment"},
    "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"canvas","file_id":CANVAS,"text":"Original Canvas fallback","url":"https://fixture.invalid/files/FLISTFILE1","style":{"bold":true},"navigation_opaque":{"occurrence":1}},
        {"type":"text","text":" and "},
        {"type":"file","file_id":TARGET_LIST,"text":"Original List fallback","url":"https://fixture.invalid/files/FLISTFILE2","style":{"italic":true},"navigation_opaque":{"occurrence":2}}
    ]}]}]})
}
/// Original comment fixture must already exist; this adds only its raw server row.
pub(crate) fn seed(shell: &mut ShellState) -> Result<Value, String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut ledger = model.0.lock().map_err(|_| "Navigation ledger poisoned")?;
    if ledger.navigation.is_some() || !ledger.requests.is_empty() {
        return Err("Seed navigation before the first comment read".into());
    }
    let received = received_message();
    ledger.navigation = Some(State {
        received: received.clone(),
        ..Default::default()
    });
    let (records, unrelated) = super::record_proof(ledger.records.iter());
    Ok(
        json!({"received":received,"originalDocument":ledger.document,
        "originalRecords":records,"originalUnrelated":unrelated,"canonicalResponse":completion::files::RESPONSE}),
    )
}
pub(super) fn decorate(state: &Option<State>, response: &mut Value) {
    let Some(state) = state else { return };
    if let Some(messages) = response["messages"].as_array_mut() {
        if let Some(row) = messages.iter_mut().find(|row| row["ts"] == RECEIVED_TS) {
            *row = state.received.clone();
        }
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Ok(model) = model(shell) else {
        return json!({"enabled":false});
    };
    let ledger = model.0.lock().unwrap();
    let Some(nav) = &ledger.navigation else {
        return json!({"enabled":false});
    };
    let (records, unrelated) = super::record_proof(ledger.records.iter());
    json!({"enabled":true,"received":nav.received,"originalDocument":ledger.document,
        "originalRecords":records,"originalUnrelated":unrelated,"targetRequests":nav.requests,"returns":nav.returns,
        "networkCalls":0,"httpCapability":false,
        "listContent":LIST_CONTENT,"canvasContent":CANVAS_CONTENT})
}
/// Re-enters the original List through its ordinary opener and closed read path.
/// The native checker must separately open the original item/Comments/Write.
pub(crate) fn return_original(state: Signal<ShellState>) -> Result<(), String> {
    let (model, file) = {
        let shell = state.peek();
        let model = model(&shell).map_err(|e| e.to_string())?;
        if !crate::canvas::surface::can_leave(&shell) {
            return Err("Target Canvas has an active edit".into());
        }
        let file = {
            let ledger = model.0.lock().map_err(|_| "Navigation ledger poisoned")?;
            if ledger.navigation.is_none() {
                return Err("Navigation seed required".into());
            }
            serde_json::from_value::<super_platinum_core::slack::models::File>(
                ledger.document.clone(),
            )
            .map_err(|e| e.to_string())?
        };
        (model, file)
    };
    crate::slack_lists::open(state, file);
    if state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .is_none_or(|v| v.id != LIST || v.loaded.is_none())
    {
        return Err("Original List did not reopen".into());
    }
    model.0.lock().unwrap().navigation.as_mut().unwrap().returns += 1;
    Ok(())
}
fn source_file(id: &str) -> Result<Value, Error> {
    let response: Value =
        serde_json::from_str(completion::files::RESPONSE).map_err(|_| refused())?;
    response["files"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == id))
        .cloned()
        .ok_or_else(refused)
}
fn record(
    shell: &ShellState,
    target: &str,
    request: PreparedRequest,
    response: Value,
) -> Result<(), Error> {
    let model = model(shell)?;
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let RequestBody::Form(body) = request.body else {
        return Err(refused());
    };
    let args = body
        .into_iter()
        .filter(|(key, _)| key != "token")
        .collect::<BTreeMap<_, _>>();
    let mut ledger = model.0.lock().map_err(|_| refused())?;
    let nav = ledger.navigation.as_mut().ok_or_else(refused)?;
    nav.requests
        .push(json!({"serial":nav.requests.len()+1,"target":target,
        "method":request.method,"path":url.path(),"args":args,"response":response}));
    Ok(())
}
fn session(shell: &ShellState) -> Result<WorkspaceSession, Error> {
    completion::handle(shell)?;
    shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get("T1"))
        .cloned()
        .ok_or_else(refused)
}
fn list_document() -> Result<Value, Error> {
    let mut file = source_file(TARGET_LIST)?;
    file["access"] = json!("read");
    file["is_locked"] = json!(true);
    file["list_metadata"] = json!({"schema":[
        {"id":"C_NAV_NAME","key":"NAME","name":"Item","type":"text","is_primary_column":true}
    ],"views":[]});
    Ok(file)
}
fn list_records() -> Value {
    json!({"ok":true,"list_id":TARGET_LIST,"records":[
        {"id":"R_NAV_TARGET","list_id":TARGET_LIST,"position":0,"fields":[{"key":"NAME","text":LIST_CONTENT}],
        "navigation_opaque":{"keep":"separate target row"}}
    ]})
}
/// The original fixture backend owns finish_read; only this target uses these
/// separate raw responses. Original document/records are never swapped out.
pub(in crate::slack_lists) fn list_read(
    shell: &ShellState,
    attempt: &crate::slack_lists::ReadAttempt,
) -> Option<Result<crate::slack_lists::Loaded, Error>> {
    if !named() || attempt.id == LIST {
        return None;
    }
    Some((|| {
        if attempt.id != TARGET_LIST || !attempt.current(shell) || !enabled(shell) {
            return Err(refused());
        }
        let owner = session(shell)?;
        let client = &shell.core.client;
        let raw = list_document()?;
        let rows = list_records();
        let permission = json!({"ok":true,"prefs":{"allow_lists":"full"}});
        let personal = json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}});
        let experiments = json!({"ok":true,"assignments":{}});
        for (request, response) in [
            (
                api::file_info(client, &owner, TARGET_LIST),
                json!({"ok":true,"file":raw}),
            ),
            (
                api::lists_records_list(client, &owner, TARGET_LIST),
                rows.clone(),
            ),
            (api::list_allow_lists(client, &owner), permission.clone()),
            (
                api::list_personal_task_preferences(client, &owner),
                personal.clone(),
            ),
            (api::list_experiments(client, &owner), experiments.clone()),
        ] {
            record(shell, TARGET_LIST, request, response)?;
        }
        Ok(crate::slack_lists::Loaded {
            document: super_platinum_core::slack_lists::Document::parse(raw)
                .map_err(|_| refused())?,
            records: api::parse_list_records(rows, TARGET_LIST)?,
            allow_lists: api::parse_list_allow_lists(&permission)?,
            personal_task_lists: Some(api::parse_list_personal_task_lists(&personal)?),
            list_experiments: Some(api::parse_list_experiments(&experiments)?),
        })
    })())
}
pub(crate) use canvas::read_pair as canvas_read_pair;
