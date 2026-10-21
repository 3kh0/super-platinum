//! Named, credential-free destination receipts through the ordinary reader.
use super::*;
use crate::canvas::checklist;
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
use tokio::sync::oneshot;

const ACCOUNT: &str = "147b5480-9f1a-4443-89da-836472cbb209";
const THREAD: &str = "canonical-destination-thread";
const DOCUMENT: &str = "canonical-destination-document";
const EXPORT: &str = "https://files.slack.com/files-tmb/FCANON002/destination.html";
pub(in crate::canvas) const TITLE: &str = "Canonical destination Canvas";
pub(in crate::canvas) const CONTENT: &str = "Native canonical Canvas destination content";
const HTML: &str = "<div class='quip-canvas-content'><h1>Canonical destination Canvas</h1><p>Native canonical Canvas destination content</p><p>Original rich reply remains saved. &amp; \u{4e2d}\u{1f98a}</p></div>";

#[derive(Default)]
struct Ledger {
    hold_next: bool,
    requests: Vec<Value>,
    pairs: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<(usize, oneshot::Sender<()>)>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn refused() -> Error {
    Error::Api("canonical_destination_canvas_unavailable".into())
}
pub(in crate::canvas) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-canonical-native-canvas" | "canvas-canonical-native-canvas-narrow")
        )
}
fn identity(shell: &ShellState) -> bool {
    shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.active_account.as_deref() == Some(ACCOUNT)
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.self_account.user_id == "U0"
        && shell
            .core
            .workspaces
            .get("T1")
            .is_some_and(|w| w.team_id == "T1" && w.self_user_id == "U0")
}
fn carrier(shell: &ShellState) -> bool {
    requested()
        && identity(shell)
        && shell.canvas.as_ref().is_some_and(|target| {
            matches!(target.id.as_str(), "Ffixture" | CANVAS)
                && crate::canvas::surface::visible(shell, &target.id)
        })
}
pub(in crate::canvas) fn enabled(shell: &ShellState, id: &str) -> bool {
    carrier(shell) && id == CANVAS && crate::canvas::surface::visible(shell, CANVAS)
}
pub(in crate::canvas) fn file() -> Value {
    json!({"id":CANVAS,"mode":"quip","filetype":"quip","name":"Canvas canonical name",
        "title":TITLE,"created":101,"is_deleted":false,"editable":false,
        "quip_thread_id":THREAD,"url_private":EXPORT,
        "permalink":"https://example.test/canonical/FCANON002",
        "destination_opaque":{"keep":[0,false,null]}})
}
pub(in crate::canvas) fn hold_next(shell: &ShellState) -> Result<(), Error> {
    if !carrier(shell) {
        return Err(refused());
    }
    let mut ledger = LEDGER.get_or_init(Default::default).lock().unwrap();
    if ledger.held.is_some() || ledger.hold_next {
        return Err(refused());
    }
    ledger.hold_next = true;
    Ok(())
}
pub(in crate::canvas) fn release(shell: &ShellState) -> Result<Value, Error> {
    if !carrier(shell) {
        return Err(refused());
    }
    let mut ledger = LEDGER.get_or_init(Default::default).lock().unwrap();
    let (serial, sender) = ledger.held.take().ok_or_else(refused)?;
    let proof = json!({"serial":serial,"delivered":sender.send(()).is_ok()});
    ledger.releases.push(proof.clone());
    Ok(proof)
}
pub(in crate::canvas) fn snapshot() -> Value {
    let ledger = LEDGER.get_or_init(Default::default).lock().unwrap();
    json!({"requests":ledger.requests,"pairs":ledger.pairs,
        "settlements":ledger.settlements,"releases":ledger.releases,
        "held":ledger.held.as_ref().map(|(serial,_)|serial),
        "networkCalls":0,"httpCapability":false})
}
struct Pending {
    serial: usize,
    settled: bool,
}
impl Pending {
    fn finish(&mut self, status: &str) {
        LEDGER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":self.serial,"status":status}));
        self.settled = true;
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.settled {
            self.finish("cancelled");
        }
    }
}
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U0".into(),
        enterprise_id: None,
        name: "Closed Canvas destination".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
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
fn responses() -> (Value, Vec<u8>) {
    let options = msg(&[s(1, "canonical-quip-user"), s(33, "U0"), s(34, "T1")]);
    let init = json!({"ok":true,"quip_user_id":"canonical-quip-user",
        "init_options":STANDARD.encode(options.raw())});
    let thread = msg(&[s(1, THREAD), n(2, 101), s(10, DOCUMENT)]);
    let document = msg(&[s(1, DOCUMENT), n(2, 101), s(6, THREAD)]);
    let payload = msg(&[m(3, &thread), m(6, &document)]);
    // Explicit complete empty canonical index; the export grants no edits.
    let index = msg(&[s(1, &format!("Section/{DOCUMENT}"))]);
    let response = msg(&[n(1, 1), m(2, &payload), m(3, &index)]);
    (init, msg(&[m(2, &response)]).raw().to_vec())
}
fn record(request: PreparedRequest, response: Value) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let RequestBody::Form(fields) = request.body else {
        return Err(refused());
    };
    let args = fields
        .into_iter()
        .filter(|(key, _)| key != "token")
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(
        json!({"target":CANVAS,"method":request.method,"path":url.path(),
        "args":args,"response":response}),
    )
}
pub(in crate::canvas) async fn read_pair(
    state: Signal<ShellState>,
    read: &checklist::Read,
) -> Result<(CanvasCollabSnapshot, (File, CanvasDocument)), Error> {
    let (init, bytes) = responses();
    let (mut pending, receiver) = {
        let shell = state.peek();
        if !enabled(&shell, &read.file) || !read.current(&shell) {
            return Err(refused());
        }
        let owner = owner();
        let client = &shell.core.client;
        let mut requests = vec![
            record(
                api::canvas_info(client, &owner, CANVAS),
                json!({"ok":true,"file":file()}),
            )?,
            record(api::canvas_controller_init(client, &owner), init.clone())?,
            record(
                api::canvas_collab_load_request(client, &owner, THREAD)?,
                json!({"base64":STANDARD.encode(&bytes)}),
            )?,
            record(
                api::canvas_info(client, &owner, CANVAS),
                json!({"ok":true,"file":file()}),
            )?,
            json!({"target":CANVAS,"method":"GET","path":EXPORT,"args":{},"response":{"html":HTML}}),
        ];
        let mut ledger = LEDGER.get_or_init(Default::default).lock().unwrap();
        if ledger.held.is_some() {
            return Err(refused());
        }
        let serial = ledger.pairs.len() + 1;
        let held = std::mem::take(&mut ledger.hold_next);
        for request in &mut requests {
            request["serial"] = json!(ledger.requests.len() + 1);
            request["pair"] = json!(serial);
            ledger.requests.push(request.clone());
        }
        ledger.pairs.push(json!({"serial":serial,"held":held}));
        let receiver = if held {
            let (sender, receiver) = oneshot::channel();
            ledger.held = Some((serial, sender));
            Some(receiver)
        } else {
            None
        };
        (
            Pending {
                serial,
                settled: false,
            },
            receiver,
        )
    };
    if let Some(receiver) = receiver {
        if !matches!(
            tokio::time::timeout(std::time::Duration::from_secs(45), receiver).await,
            Ok(Ok(()))
        ) {
            pending.finish("failed");
            return Err(refused());
        }
    }
    if !enabled(&state.peek(), &read.file) || !read.current(&state.peek()) {
        pending.finish("retired");
        return Err(refused());
    }
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
    let shell = state.peek();
    if !enabled(&shell, &read.file) || !read.current(&shell) {
        pending.finish("retired");
        return Err(refused());
    }
    pending.finish("ready");
    Ok((canonical, (file, document)))
}

#[cfg(test)]
mod tests;
