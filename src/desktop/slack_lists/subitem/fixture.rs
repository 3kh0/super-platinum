//! Named disconnected HTTP source. The actual builders/decoders/reducers run.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        api,
        client::{PreparedRequest, RequestBody},
    },
};
#[derive(Default)]
struct Ledger {
    document: Value,
    population: Value,
    requests: Vec<Value>,
    admissions: Vec<Value>,
    responses: Vec<Value>,
    finished: Vec<Value>,
    preflight: Option<tokio::sync::oneshot::Sender<()>>,
    write: Option<tokio::sync::oneshot::Sender<()>>,
    read: Option<tokio::sync::oneshot::Sender<()>>,
    hold_preflight: bool,
    hold_read: bool,
}
fn ledger() -> &'static Mutex<Ledger> {
    static L: OnceLock<Mutex<Ledger>> = OnceLock::new();
    L.get_or_init(Default::default)
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_SUBITEM_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-subitem-create"
                | "slack-list-subitem-create-narrow"
                | "slack-list-subitem-uncertain"
                | "slack-list-subitem-uncertain-narrow")
        )
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    check_enabled()
        && super::super::fixture::enabled(shell)
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
fn uncertain() -> bool {
    std::env::var("SUPER_PLATINUM_FIXTURE").is_ok_and(|s| s.contains("-uncertain"))
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let (document, population) = super::super::fixture::subitem::install(shell, name);
    *ledger().lock().unwrap() = Ledger {
        document,
        population,
        hold_preflight: true,
        ..Default::default()
    };
}
pub(super) fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    }
}
fn record(request: PreparedRequest, g: &Guard, stage: Stage) -> Result<String, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| Error::Api("fixture_url".into()))?;
    let RequestBody::Form(fields) = request.body else {
        return Err(Error::Api("fixture_form".into()));
    };
    if request.method != "POST"
        || url.host_str() != Some("example.test")
        || fields.first() != Some(&("token".into(), "fixture-not-a-token".into()))
    {
        return Err(Error::Api("fixture_owner".into()));
    }
    let path = url.path().to_owned();
    ledger()
        .lock()
        .unwrap()
        .requests
        .push(json!({"method":"POST","path":path,"fields":fields}));
    ledger().lock().unwrap().admissions.push(json!({"serial":g.serial,"job":g.job,"stage":format!("{:?}",stage),"list":g.key.list,"parent":g.proof.parent,"visit":g.proof.visit,"detail":g.proof.detail,"epoch":g.proof.epoch,"selection":g.proof.selection}));
    Ok(path)
}
fn decode(raw: &Value) -> Result<Value, Error> {
    super_platinum_core::slash_commands::codec::decode(
        &serde_json::to_vec(raw).unwrap(),
        1024 * 1024,
    )
    .map_err(|_| Error::Api("fixture_decode".into()))
}
async fn wait(rx: tokio::sync::oneshot::Receiver<()>) -> Result<(), Error> {
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Error::Transport("fixture_timeout".into()))?
        .map_err(|_| Error::Transport("fixture_closed".into()))
}
pub(super) async fn read(
    state: Signal<ShellState>,
    g: &Guard,
    stage: Stage,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !enabled(&state.peek()) || !g.current(&state.peek(), stage) {
        return Err(Error::Api("fixture_retired".into()));
    }
    let path = record(request, g, stage)?;
    let (raw, held, limit) = {
        let mut l = ledger().lock().unwrap();
        let raw = match path.as_str() {
            "/api/files.info" => json!({"ok":true,"file":l.document}),
            "/api/team.prefs.get" => json!({"ok":true,"prefs":{"allow_lists":"full"}}),
            "/api/users.prefs.get" => {
                json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}})
            }
            "/api/experiments.getByUser" => {
                json!({"ok":true,"assignments":{"lists_sub_tasks":{"group":"on"}}})
            }
            "/api/lists.records.info" => {
                let rows = l.population["records"].as_array().unwrap();
                let parent = rows.iter().find(|r| r["id"] == g.proof.parent).unwrap();
                let children = rows
                    .iter()
                    .filter(|r| {
                        r["parent_record_id"] == g.proof.parent
                            && (stage == Stage::Preparing || r["id"] == "R_SERVER_SUBITEM")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                json!({"ok":true,"list":l.document,"record":parent,"subtasks":children,"opaque_read":{"keep":true}})
            }
            _ => return Err(Error::Api("fixture_read_path".into())),
        };
        let mut limit = false;
        let held = if path == "/api/files.info" && stage == Stage::Preparing && l.hold_preflight {
            l.hold_preflight = false;
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.preflight = Some(tx);
            Some(rx)
        } else if path == "/api/files.info" && stage == Stage::Checking && l.hold_read {
            l.hold_read = false;
            limit = true;
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.read = Some(tx);
            Some(rx)
        } else {
            None
        };
        l.responses.push(if limit {
            json!({"syntheticFailure":"http_status","status":503,"retry_after_secs":3})
        } else {
            raw.clone()
        });
        (raw, held, limit)
    };
    if let Some(rx) = held {
        wait(rx).await?;
    }
    if limit {
        return Err(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(3),
        });
    }
    decode(&raw)
}
pub(super) async fn write(
    state: Signal<ShellState>,
    g: &Guard,
    permit: SubitemCreatePermit,
) -> Result<Value, ListSubitemFailure> {
    if !enabled(&state.peek())
        || !g.current(&state.peek(), Stage::Sending)
        || rename::creation_cooldown(&g.scope)
    {
        return Err(ListSubitemFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let timestamp = with(|r| r.entries[&g.key].timestamp.clone());
    let request =
        api::list_subitem_create(&state.peek().core.client, &session(), permit, &timestamp);
    record(request, g, Stage::Sending).map_err(ListSubitemFailure::NotRequested)?;
    let raw = json!({"id":"R_SERVER_SUBITEM","list_id":"F_LIST_FIXTURE","parent_record_id":"R_HYDRATE_PARENT","position":"0000000005","date_created":99,"created_by":"U1","fields":[{"key":"NAME","rich_text":[]},{"key":"done","checkbox":false}],"opaque_server":{"keep":true}});
    let response = json!({"ok":true,"record":raw,"opaque_ack":{"keep":true}});
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        if l.write.is_some() {
            return Err(ListSubitemFailure::Unconfirmed(Error::Transport(
                "fixture_duplicate".into(),
            )));
        }
        l.responses.push(if uncertain() {
            json!({"syntheticFailure":"response_lost"})
        } else {
            response.clone()
        });
        l.write = Some(tx);
    }
    wait(rx).await.map_err(ListSubitemFailure::Unconfirmed)?;
    {
        let mut l = ledger().lock().unwrap();
        l.document
            .as_object_mut()
            .unwrap()
            .remove("old_document_unknown");
        l.document["returned_document_unknown"] = json!({"keep":[1,2]});
        let parent = l.population["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == "R_HYDRATE_PARENT")
            .unwrap();
        parent.as_object_mut().unwrap().remove("old_parent_unknown");
        parent["returned_parent_unknown"] = json!({"keep":true});
        l.population["records"].as_array_mut().unwrap().push(raw);
    }
    if uncertain() {
        Err(ListSubitemFailure::Unconfirmed(Error::Transport(
            "fixture_lost".into(),
        )))
    } else {
        decode(&response).map_err(ListSubitemFailure::Unconfirmed)
    }
}
pub(super) fn finished(g: &Guard, phase: &str, applied: bool) {
    if check_enabled() {
        ledger()
            .lock()
            .unwrap()
            .finished
            .push(json!({"serial":g.serial,"phase":phase,"applied":applied}));
    }
}
pub(crate) fn release(which: &str) -> Result<(), String> {
    let mut l = ledger().lock().unwrap();
    let sender = match which {
        "preflight" => l.preflight.take(),
        "write" => l.write.take(),
        "read" => l.read.take(),
        _ => None,
    };
    sender
        .ok_or("No held response")?
        .send(())
        .map_err(|_| "Held receiver retired".into())
}
pub(crate) fn hold_read() {
    ledger().lock().unwrap().hold_read = true;
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) {
        return Err("Not disconnected subitem fixture".into());
    }
    let l = ledger().lock().unwrap();
    Ok(
        json!({"requests":l.requests,"admissions":l.admissions,"responses":l.responses,"finished":l.finished,"document":l.document,"serverPopulation":l.population,
        "held":{"preflight":l.preflight.is_some(),"write":l.write.is_some(),"read":l.read.is_some()},
        "retained":entry(shell).map(|e|json!({"serial":e.serial,"inFlight":e.in_flight,"timestamp":e.timestamp.to_string(),"stage":format!("{:?}",e.stage),"reply":e.reply.as_deref(),"read":e.read.as_deref(),"child":e.child.map(|r|r.raw),"failure":e.failure,"charge":e.charge})),
        "noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none()}),
    )
}

pub(crate) fn reopen(shell: &mut ShellState) -> Result<u64, String> {
    if !enabled(shell) {
        return Err("Disconnected fixture required".into());
    }
    super::super::fixture::subitem::reopen(shell)
}
