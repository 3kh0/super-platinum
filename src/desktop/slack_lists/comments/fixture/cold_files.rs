//! Closed singleton title reads. Responses never install canonical source or UI state.
use super::*;
use super_platinum_core::slack::{
    SlackClient, api::custom::texty::file_title_source::OrderedFileInfo,
};
#[cfg(test)]
mod tests;

pub(crate) use file_navigation::{CANVAS, TARGET_LIST};
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum Mode {
    Success,
    #[default]
    Held,
    Malformed,
    Offline,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Held => "held",
            Self::Malformed => "malformed",
            Self::Offline => "offline",
        }
    }
}
struct Held {
    serial: usize,
    id: String,
    sender: oneshot::Sender<String>,
    response: String,
}
#[derive(Default)]
pub(super) struct State {
    modes: BTreeMap<String, Mode>,
    future: BTreeMap<String, String>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    releases: Vec<Value>,
    settlements: Vec<Value>,
    invalidations: Vec<Value>,
    held: Vec<Held>,
    completed: usize,
    cancelled: usize,
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("cold-files" | "cold-files-recovery")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell) && completion::enabled(shell)
}
fn refused() -> Error {
    Error::Offline("Closed cold-file reader unavailable".into())
}
fn model(shell: &ShellState) -> Result<Model, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    completion::handle(shell)?;
    shell
        .slack_lists
        .comments
        .fixture
        .clone()
        .ok_or_else(refused)
}
fn valid(id: &str) -> bool {
    matches!(id, CANVAS | TARGET_LIST)
}
fn original_text(id: &str) -> Result<&'static str, Error> {
    match id {
        CANVAS => Ok(include_str!("cold_files/canvas-response.json")),
        TARGET_LIST => Ok(include_str!("cold_files/list-response.json")),
        _ => Err(refused()),
    }
}
fn parsed(text: &str) -> Result<Value, Error> {
    serde_json::from_str(text).map_err(|_| refused())
}
fn responses(future: &BTreeMap<String, String>) -> Value {
    json!(
        future
            .iter()
            .map(|(id, text)| (id.clone(), parsed(text).expect("literal response")))
            .collect::<BTreeMap<_, _>>()
    )
}
/// Before comments open: raw received row and future server receipts only.
/// Neither autocomplete nor a canonical file observation is seeded.
pub(crate) fn seed(shell: &mut ShellState) -> Result<Value, String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    if model.0.lock().unwrap().cold_files.is_some() {
        return Err("Cold files already seeded".into());
    }
    let navigation = file_navigation::seed(shell)?;
    let future = [CANVAS, TARGET_LIST]
        .into_iter()
        .map(|id| Ok((id.to_owned(), original_text(id)?.to_owned())))
        .collect::<Result<BTreeMap<_, _>, Error>>()
        .map_err(|e| e.to_string())?;
    let proof = json!({"navigation":navigation,"initialResponses":responses(&future),"initialResponseTexts":future,"canonicalSeeded":false,"searchRequests":0});
    model.0.lock().unwrap().cold_files = Some(State {
        future,
        ..Default::default()
    });
    Ok(proof)
}
pub(crate) fn configure(shell: &ShellState, id: &str, mode: Mode) -> Result<(), Error> {
    if !valid(id) {
        return Err(refused());
    }
    let model = model(shell)?;
    let mut l = model.0.lock().map_err(|_| refused())?;
    l.cold_files
        .as_mut()
        .ok_or_else(refused)?
        .modes
        .insert(id.into(), mode);
    Ok(())
}
fn renamed(state: &mut State, id: &str, title: &str) -> Result<Value, Error> {
    if !valid(id) || title.is_empty() || title.len() > 128 {
        return Err(refused());
    }
    let text = state.future.get_mut(id).ok_or_else(refused)?;
    let before = parsed(text)?;
    // These closed literals have compact scalar fields. Replace only exact
    // title/name/updated occurrences, retaining all original object key order.
    for (key, value) in [
        ("title", json!(title)),
        ("name", json!(title)),
        ("updated", json!(102)),
    ] {
        let old = format!("\"{key}\":{}", before["file"][key]);
        if text.matches(&old).count() != 1 {
            return Err(refused());
        }
        *text = text.replacen(&old, &format!("\"{key}\":{value}"), 1);
    }
    parsed(text)
}
/// Changes only future server response; the actual local invalidation revokes
/// a pending source even before an ID has a row. Held receipts remain immutable.
pub(crate) fn invalidate(shell: &mut ShellState, id: &str, title: &str) -> Result<Value, Error> {
    let model = model(shell)?;
    let future = {
        let mut l = model.0.lock().map_err(|_| refused())?;
        renamed(l.cold_files.as_mut().ok_or_else(refused)?, id, title)?
    };
    let before = source(shell);
    crate::file_events::invalidate(shell, "T1", id);
    let proof = json!({"kind":"accepted-local-file-invalidation","id":id,"futureResponse":future,
        "before":before,"after":source(shell)});
    model
        .0
        .lock()
        .unwrap()
        .cold_files
        .as_mut()
        .unwrap()
        .invalidations
        .push(proof.clone());
    Ok(proof)
}
fn source(shell: &ShellState) -> Value {
    crate::profile_edit::sections::rich::search::file_source::fixture_completion_proof(shell)
        .unwrap_or_else(|_| json!({"unavailable":true}))
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Ok(model) = model(shell) else {
        return json!({"enabled":false});
    };
    let source = source(shell);
    let identity = completion::snapshot(shell);
    let l = model.0.lock().unwrap();
    let Some(s) = &l.cold_files else {
        return json!({"enabled":false});
    };
    let held: Vec<_> = s
        .held
        .iter()
        .map(|h| json!({"serial":h.serial,"id":h.id,"receiverClosed":h.sender.is_closed()}))
        .collect();
    json!({"enabled":true,"named":true,"protectedFixture":cfg!(any(test,feature="protected-native-fixture")),
        "identityCurrent":identity["identityCurrent"],"account":identity["account"],"epoch":identity["epoch"],
        "team":identity["team"],"user":identity["user"],"networkCalls":0,"httpCapability":false,
        "requests":s.requests,"responses":s.responses,"releases":s.releases,"settlements":s.settlements,"invalidations":s.invalidations,
        "completed":s.completed,"cancelled":s.cancelled,"held":held,"futureResponses":responses(&s.future),"futureResponseTexts":s.future,"source":source})
}
fn expected(client: &SlackClient, session: &WorkspaceSession, id: &str) -> PreparedRequest {
    client.rest_form(
        session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    )
}
fn validate(
    client: &SlackClient,
    session: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<(String, Value), Error> {
    let RequestBody::Form(body) = &request.body else {
        return Err(refused());
    };
    let id = body
        .iter()
        .find(|(k, _)| k == "file")
        .map(|(_, v)| v.as_str())
        .filter(|id| valid(id))
        .ok_or_else(refused)?;
    if !completion::protocol::same_request(request, &expected(client, session, id)) {
        return Err(refused());
    }
    let args: BTreeMap<_, _> = body.iter().filter(|(k, _)| k != "token").cloned().collect();
    Ok((id.into(), json!(args)))
}
struct Finish {
    model: Model,
    serial: usize,
    completed: bool,
}
impl Drop for Finish {
    fn drop(&mut self) {
        if let Ok(mut l) = self.model.0.lock() {
            if let Some(s) = l.cold_files.as_mut() {
                s.completed += 1;
                s.settlements
                    .push(json!({"serial":self.serial,"cancelled":!self.completed}));
                if !self.completed {
                    s.cancelled += 1;
                }
            }
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<OrderedFileInfo, Error> {
    let (model, serial, receiver) = {
        let shell = state.peek();
        let model = model(&shell)?;
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get("T1"))
            .ok_or_else(refused)?;
        let (id, args) = validate(&shell.core.client, session, request)?;
        let mut l = model.0.lock().map_err(|_| refused())?;
        let s = l.cold_files.as_mut().ok_or_else(refused)?;
        let mode = s.modes.get(&id).copied().unwrap_or_default();
        let serial = s.requests.len() + 1;
        let response = match mode {
            Mode::Malformed => "{\"ok\":true,\"file\":false}".into(),
            Mode::Offline => "null".into(),
            _ => s.future.get(&id).cloned().ok_or_else(refused)?,
        };
        s.requests.push(
            json!({"serial":serial,"id":id,"method":"files.info","httpMethod":request.method,
            "path":"/api/files.info","args":args,"mode":mode.label(),"plannedResponse":parsed(&response)?,"responseText":response}),
        );
        s.responses
            .push(json!({"serial":serial,"id":id,"disposition":mode.label(),"raw":parsed(&response)?,"responseText":response}));
        if !matches!(mode, Mode::Held) {
            s.completed += 1;
            s.settlements
                .push(json!({"serial":serial,"cancelled":false}));
            return if matches!(mode, Mode::Offline) {
                Err(refused())
            } else {
                OrderedFileInfo::decode(response.as_bytes())
            };
        }
        let (sender, receiver) = oneshot::channel();
        s.held.push(Held {
            serial,
            id,
            sender,
            response,
        });
        drop(l);
        (model, serial, receiver)
    };
    let mut finish = Finish {
        model,
        serial,
        completed: false,
    };
    let result = receiver.await.map_err(|_| refused())?;
    finish.completed = true;
    OrderedFileInfo::decode(result.as_bytes())
}
pub(crate) fn release(shell: &ShellState, serial: usize) -> Result<Value, Error> {
    let model = model(shell)?;
    let held = {
        let mut l = model.0.lock().map_err(|_| refused())?;
        let s = l.cold_files.as_mut().ok_or_else(refused)?;
        let index = s
            .held
            .iter()
            .position(|h| h.serial == serial)
            .ok_or_else(refused)?;
        s.held.remove(index)
    };
    let delivered = held.sender.send(held.response).is_ok();
    let proof = json!({"serial":serial,"id":held.id,"delivered":delivered,"cancelledBeforeReply":!delivered});
    model
        .0
        .lock()
        .unwrap()
        .cold_files
        .as_mut()
        .unwrap()
        .releases
        .push(proof.clone());
    Ok(proof)
}
