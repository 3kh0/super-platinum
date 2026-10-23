//! Exact returned-A fixture pair. No session, transport or production readiness bypass.
use super::*;
use crate::canvas::checklist::{self, scope::Scope};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use super_platinum_core::{
    canvas::{
        CanvasDocument,
        wire::{EncodedField as F, LoadDataResponse, RawMessage},
    },
    config::WorkspaceSession,
    slack::{
        api::{CanvasCollabSnapshot, CanvasControllerInit},
        client::{PreparedRequest, RequestBody},
        models::File,
    },
};

const FILE: &str = "Ffixture";
const THREAD: &str = "returned-a-thread";
const DOCUMENT: &str = "returned-a-document";
const EXPORT: &str = "https://files.slack.com/files-tmb/Ffixture/returned-a.html";
pub(in crate::canvas) const TITLE: &str = "Returned Project handbook";
pub(in crate::canvas) const CONTENT: &str =
    "Current returned-account body. Original rich reply stays saved.";
const HTML: &str = "<div class='quip-canvas-content'><h1>Returned Project handbook</h1><p>Current returned-account body. Original rich reply stays saved.</p><p>Read-only acquired body &amp; \u{4e2d}\u{1f98a}</p></div>";

#[derive(Default)]
struct Ledger {
    owner: Option<(Scope, u64)>,
    identity: Value,
    pairs: Vec<Value>,
    requests: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn refused() -> Error {
    Error::Api("returned_body_fixture_unavailable".into())
}
pub(in crate::canvas) fn requested() -> bool {
    super::super::super::reply::fixture::replacement::requested()
        && matches!(
            std::env::var("SUPER_PLATINUM_CANVAS_RETURN_BODY_CHECK").as_deref(),
            Ok("success" | "failure")
        )
}
pub(in crate::canvas) fn failure() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_RETURN_BODY_CHECK").as_deref() == Ok("failure")
}
fn identity(s: &ShellState) -> bool {
    requested()
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.cache.is_none()
        && s.core.active_account.as_deref() == Some(super::super::super::reply::fixture::ACCOUNT)
        && s.core.active_team.as_deref() == Some("T1")
        && s.self_account.user_id == "U0"
        && s.canvas.as_ref().is_some_and(|c| c.id == FILE)
        && crate::canvas::surface::visible(s, FILE)
}
pub(in crate::canvas) fn arm(s: &ShellState) -> Result<(), String> {
    if !identity(s) {
        return Err("Returned body owner unavailable".into());
    }
    let scope = Scope::capture(s).ok_or("Returned body scope unavailable")?;
    let view = checklist::presentation(s).ok_or("Returned body visit missing")?;
    if view.read != 0 || view.loading || !view.stale || view.snapshot.is_some() {
        return Err("Returned body was already acquired".into());
    }
    let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
    if l.owner.is_some() {
        return Err("Returned body fixture already armed".into());
    }
    l.identity = json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,
        "user":s.self_account.user_id,"visit":s.shortcut_generation,"file":FILE});
    l.owner = Some((scope, s.shortcut_generation));
    Ok(())
}
pub(in crate::canvas) fn enabled(s: &ShellState, id: &str) -> bool {
    identity(s)
        && id == FILE
        && LEDGER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .owner
            .as_ref()
            .is_some_and(|(scope, visit)| scope.current(s) && *visit == s.shortcut_generation)
}
pub(in crate::canvas) fn snapshot(s: &ShellState) -> Value {
    let l = LEDGER.get_or_init(Default::default).lock().unwrap();
    let current = l.owner.as_ref().is_some_and(|(scope, visit)| {
        identity(s) && scope.current(s) && *visit == s.shortcut_generation
    });
    let view=checklist::presentation(s).map(|v|json!({"visit":v.visit,"read":v.read,"loading":v.loading,
        "stale":v.stale,"error":v.error,"snapshot":v.snapshot.as_ref().map(|c|json!({"file":c.file_id(),
        "thread":c.thread_id(),"document":c.document_id(),"sections":c.sections.len()}))}));
    json!({"case":if failure(){"failure"}else{"success"},"armed":l.owner.is_some(),"current":current,
        "owner":l.identity,"pairs":l.pairs,"requests":l.requests,"presentation":view,
        "networkCalls":0,"httpCapability":false})
}
pub(super) fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U0".into(),
        enterprise_id: None,
        name: "Returned Canvas fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
pub(super) fn file() -> Value {
    json!({"id":FILE,"mode":"quip","filetype":"quip","title":TITLE,"name":TITLE,
    "editable":false,"quip_thread_id":THREAD,"url_private":EXPORT})
}
fn msg(fields: &[F]) -> RawMessage {
    RawMessage::from_fields(fields).unwrap()
}
fn s(tag: u32, value: &str) -> F {
    F::bytes(tag, value.as_bytes()).unwrap()
}
fn n(tag: u32, value: u64) -> F {
    F::uint(tag, value).unwrap()
}
fn m(tag: u32, value: &RawMessage) -> F {
    F::message(tag, value).unwrap()
}
pub(super) fn responses() -> (Value, Vec<u8>) {
    let options = msg(&[s(1, "returned-quip-user"), s(33, "U0"), s(34, "T1")]);
    let init = json!({"ok":true,"quip_user_id":"returned-quip-user","init_options":STANDARD.encode(options.raw())});
    let thread = msg(&[s(1, THREAD), n(2, 102), s(10, DOCUMENT)]);
    let document = msg(&[s(1, DOCUMENT), n(2, 102), s(6, THREAD)]);
    let payload = msg(&[m(3, &thread), m(6, &document)]);
    let index = msg(&[s(1, &format!("Section/{DOCUMENT}"))]);
    (
        init,
        msg(&[m(2, &msg(&[n(1, 1), m(2, &payload), m(3, &index)]))])
            .raw()
            .to_vec(),
    )
}
pub(super) fn record(request: PreparedRequest, response: Value) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let RequestBody::Form(fields) = request.body else {
        return Err(refused());
    };
    let args = fields
        .into_iter()
        .filter(|(k, _)| k != "token")
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(
        json!({"target":FILE,"method":request.method,"path":url.path(),"args":args,"response":response}),
    )
}
pub(in crate::canvas) async fn read_pair(
    state: Signal<ShellState>,
    read: &checklist::Read,
) -> Result<(CanvasCollabSnapshot, (File, CanvasDocument)), Error> {
    let (init, bytes) = responses();
    let (serial, failed) = {
        let s = state.peek();
        if !enabled(&s, &read.file) || !read.current(&s) {
            return Err(refused());
        }
        let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
        let serial = l.pairs.len() + 1;
        let failed = failure() && serial == 1;
        if serial > if failure() { 2 } else { 1 } {
            return Err(refused());
        }
        let client = &s.core.client;
        let owner = owner();
        let mut requests = vec![
            record(
                api::canvas_info(client, &owner, FILE),
                json!({"ok":true,"file":file()}),
            )?,
            record(api::canvas_controller_init(client, &owner), init.clone())?,
            record(
                api::canvas_collab_load_request(client, &owner, THREAD)?,
                json!({"base64":STANDARD.encode(&bytes)}),
            )?,
            record(
                api::canvas_info(client, &owner, FILE),
                json!({"ok":true,"file":file()}),
            )?,
            json!({"target":FILE,"method":"GET","path":EXPORT,"args":{},"response":if failed{json!({"error":"fixture_return_body_failed"})}else{json!({"html":HTML})}}),
        ];
        for request in &mut requests {
            request["serial"] = json!(l.requests.len() + 1);
            request["pair"] = json!(serial);
            l.requests.push(request.clone());
        }
        let owner = l.identity.clone();
        let read_serial = checklist::presentation(&s).ok_or_else(refused)?.read;
        l.pairs
            .push(json!({"serial":serial,"owner":owner,"read":read_serial,"status":"pending"}));
        (serial, failed)
    };
    let file: File = serde_json::from_value(file()).map_err(|_| refused())?;
    let canonical = CanvasCollabSnapshot::from_responses(
        file.clone(),
        CanvasControllerInit::parse(init, &owner())?,
        LoadDataResponse::decode(&bytes).map_err(|_| refused())?,
    )?;
    let document = tokio::task::spawn_blocking(|| CanvasDocument::parse(HTML))
        .await
        .map_err(|_| refused())?
        .map_err(|_| refused())?;
    let current = {
        let s = state.peek();
        enabled(&s, &read.file) && read.current(&s)
    };
    LEDGER.get_or_init(Default::default).lock().unwrap().pairs[serial - 1]["status"] =
        json!(if !current {
            "retired"
        } else if failed {
            "failed"
        } else {
            "ready"
        });
    if !current {
        return Err(refused());
    }
    if failed {
        return Err(Error::Api("fixture_return_body_failed".into()));
    }
    Ok((canonical, (file, document)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn returned_body_actual_responses_decode_readonly_complete_empty_canonical() {
        let (init, bytes) = responses();
        let file: File = serde_json::from_value(file()).unwrap();
        let snapshot = CanvasCollabSnapshot::from_responses(
            file.clone(),
            CanvasControllerInit::parse(init, &owner()).unwrap(),
            LoadDataResponse::decode(&bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.file_id(), FILE);
        assert_eq!(snapshot.thread_id(), THREAD);
        assert_eq!(snapshot.document_id(), DOCUMENT);
        assert!(snapshot.sections.is_empty());
        assert_eq!(file.extra["editable"], false);
        assert!(CanvasDocument::parse(HTML).is_ok());
    }
    #[test]
    fn returned_body_factories_bind_original_file_and_do_not_log_credentials() {
        let client = super_platinum_core::slack::SlackClient::default();
        let info = record(
            api::canvas_info(&client, &owner(), FILE),
            json!({"ok":true}),
        )
        .unwrap();
        assert_eq!(info["path"], "/api/files.info");
        assert_eq!(info["args"], json!({"file":FILE}));
        assert!(!info.to_string().contains("fixture-only"));
        let load = record(
            api::canvas_collab_load_request(&client, &owner(), THREAD).unwrap(),
            json!({}),
        )
        .unwrap();
        assert_eq!(load["path"], "/canvas/-/load-data");
        assert!(!load.to_string().contains("fixture-only"));
    }
}
