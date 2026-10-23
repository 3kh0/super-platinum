//! Named offline own-edit receipts. No HTTP capability and no implicit readback.
use super::*;
use super_platinum_core::{
    slack::{api, transport::ListCommentUpdateFailure},
    slack_lists::comments::{OwnEditAccepted, OwnEditIntent},
    state::posting_metadata::authority::BootRead,
};
mod policy;
pub(crate) mod restart;
#[cfg(test)]
mod tests;
pub(crate) mod title_users;
pub(crate) use policy::policy;
pub(crate) const RECEIVED_TS: &str = "1781524801.000001";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Success,
    HeldSuccess,
    Rejected,
    Unconfirmed,
    NotRequested,
}
#[derive(Default)]
pub(super) struct State {
    received: Value,
    boot: Value,
    policy: Vec<Value>,
    mode: Mode,
    prepared: Vec<Value>,
    updates: Vec<Value>,
    responses: Vec<Value>,
    candidates: Vec<Value>,
    published: Option<Value>,
    held: Option<oneshot::Sender<()>>,
    releases: Vec<Value>,
}
pub(crate) fn named() -> bool {
    if super::plain::named()
        || super::ordered::named()
        || noop_named()
        || runs_named()
        || completion::title_users::own_edit_named()
        || completion::empty_canvas::own_edit_named()
    {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit"
                | "own-delete"
                | "own-delete-failure"
                | "own-delete-storage"
                | "own-delete-guards"
                | "own-delete-rate-limit"
                | "own-delete-restart-pending-exercise"
                | "own-delete-restart-pending-restore"
                | "own-delete-restart-unknown-exercise"
                | "own-delete-restart-unknown-restore"
                | "own-edit-custody"
                | "own-edit-recovery"
                | "own-edit-storage"
                | "own-edit-restart-unsent-exercise"
                | "own-edit-restart-unsent-restore"
                | "own-edit-restart-unknown-exercise"
                | "own-edit-restart-unknown-restore")
        )
}
pub(crate) fn noop_named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_NOOP_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-noop" | "own-edit-noop-storage")
        )
}
pub(crate) fn runs_named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_RUNS_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-runs")
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::enabled(shell) && completion::enabled(shell)
}
fn refused() -> Error {
    Error::Offline("Closed own-comment edit fixture unavailable".into())
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
pub(crate) fn received_message() -> Value {
    if super::plain::named() || super::ordered::named() {
        return super::ordered::initial_message();
    }
    let mut raw = json!({"type":"message","ts":RECEIVED_TS,"thread_ts":ROOT,"user":completion::SELF,
        "text":"Original own comment 中🙂.\nSecond editable line.",
        "edit_fixture_opaque":{"keep":"original receipt"},
        "reactions":[{"name":"wave","count":1,"users":[completion::members::FIRST]}],
        "blocks":[{"type":"rich_text","block_id":"own-edit-rich","elements":[
            {"type":"rich_text_section","elements":[{"type":"text","text":"Original own comment 中🙂.","style":{"bold":true}}]},
            {"type":"rich_text_section","elements":[{"type":"text","text":"Second editable line.","style":{"italic":true}}]}
        ]},{"type":"context","block_id":"own-edit-context","elements":[{"type":"plain_text","text":"Preserved original context"}],"edit_context_opaque":{"keep":true}}]});
    if title_users::named() {
        title_users::decorate(&mut raw);
    }
    raw
}
pub(crate) fn seed(mut state: Signal<ShellState>) -> Result<Value, String> {
    let mut shell = state.write();
    let model = model(&shell).map_err(|e| e.to_string())?;
    let mut ledger = model.0.lock().map_err(|_| "Edit fixture ledger poisoned")?;
    if ledger.own_edit.is_some() || !ledger.requests.is_empty() {
        return Err("Seed own edit before the first comment read".into());
    }
    let session = session(&shell).map_err(|e| e.to_string())?;
    if shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.unified_user_client_enabled.get("T1"))
        != Some(&false)
    {
        return Err("Original non-unified fixture session required".into());
    }
    let transport = shell
        .core
        .transport
        .clone()
        .ok_or("Missing disconnected fixture transport")?;
    let read = BootRead::begin(&mut shell.core, &transport, &session)
        .map_err(|e| format!("Own edit fixture boot unavailable: {e:?}"))?;
    let request = read
        .request(&shell.core)
        .map_err(|e| format!("Own edit fixture boot request unavailable: {e:?}"))?;
    let boot = json!({"ok":true,"team":{"id":"T1","plan":"plus"},
        "self":{"id":completion::SELF,"team_id":"T1"},
        "account_types":{"is_admin":[],"is_owner":[],"is_primary_owner":[]},
        "workspaces":[{"id":"T1"}],"read_only_channels":[],"thread_only_channels":[]});
    if !read
        .apply(&mut shell.core, boot.clone(), false)
        .map_err(|e| e.to_string())?
    {
        return Err("Original boot was not accepted".into());
    }
    let received = received_message();
    let boot = json!({"request":wire(&request).map_err(|e|e.to_string())?,"response":boot,"applyWorkspace":false});
    ledger.own_edit = Some(State {
        received: received.clone(),
        boot: boot.clone(),
        ..Default::default()
    });
    Ok(json!({"received":received,"boot":boot}))
}
pub(super) fn decorate(state: &Option<State>, response: &mut Value) {
    let Some(state) = state else { return };
    if let Some(row) = response["messages"]
        .as_array_mut()
        .and_then(|rows| rows.iter_mut().find(|r| r["ts"] == RECEIVED_TS))
    {
        *row = state.published.as_ref().unwrap_or(&state.received).clone();
    }
}
pub(crate) fn mode(shell: &ShellState, mode: Mode) -> Result<(), String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model.0.lock().map_err(|_| "Edit fixture ledger poisoned")?;
    let l = l.own_edit.as_mut().ok_or("Own edit fixture not seeded")?;
    if l.held.is_some() {
        return Err("Release original update first".into());
    }
    l.mode = mode;
    Ok(())
}
pub(crate) fn release(shell: &ShellState) -> Result<Value, String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model.0.lock().map_err(|_| "Edit fixture ledger poisoned")?;
    let l = l.own_edit.as_mut().ok_or("Own edit fixture not seeded")?;
    let tx = l.held.take().ok_or("No held original edit")?;
    let cancelled = tx.is_closed();
    let delivered = tx.send(()).is_ok();
    let proof = json!({"delivered":delivered,"cancelledBeforeReply":cancelled});
    l.releases.push(proof.clone());
    Ok(proof)
}
pub(crate) fn publish(shell: &ShellState) -> Result<(), String> {
    let model = model(shell).map_err(|e| e.to_string())?;
    let mut l = model.0.lock().map_err(|_| "Edit fixture ledger poisoned")?;
    let l = l.own_edit.as_mut().ok_or("Own edit fixture not seeded")?;
    l.published = Some(
        l.candidates
            .last()
            .ok_or("No independently prepared future row")?
            .clone(),
    );
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Ok(model) = model(shell) else {
        return json!({"enabled":false});
    };
    retained_snapshot(&model)
}
/// Inspect the original offline ledger after owner replacement. This grants no
/// current owner, request admission or network capability.
pub(crate) fn retained_snapshot(model: &Model) -> Value {
    let l = model.0.lock().unwrap();
    let Some(l) = &l.own_edit else {
        return json!({"enabled":false});
    };
    json!({"enabled":true,"received":l.received,"boot":l.boot,"policy":l.policy,
        "prepared":l.prepared,"updates":l.updates,"responses":l.responses,"futureRows":l.candidates,
        "published":l.published.is_some(),"publishedRow":l.published,"held":l.held.is_some(),
        "releases":l.releases,"networkCalls":0,"httpCapability":false})
}
pub(super) fn same(a: &PreparedRequest, b: &PreparedRequest) -> bool {
    a.method == b.method
        && request_url(&a.url)
            .zip(request_url(&b.url))
            .is_some_and(|(a, b)| a == b)
        && a.headers == b.headers
        && match (&a.body, &b.body) {
            (RequestBody::Form(a), RequestBody::Form(b)) => a == b,
            _ => false,
        }
}
// rest_pairs generates three fresh UUIDv4 trace values per factory invocation.
// Validate their exact shape; all other query fields remain exact and unique.
fn request_url(raw: &str) -> Option<(url::Url, BTreeMap<String, String>)> {
    let mut url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut query = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        let value = match key.as_ref() {
            "_x_id" | "_x_csid" | "fp" => {
                let bytes = value.as_bytes();
                if bytes.len() != 32
                    || !bytes
                        .iter()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                    || bytes[12] != b'4'
                    || !matches!(bytes[16], b'8' | b'9' | b'a' | b'b')
                {
                    return None;
                }
                String::new()
            }
            "_x_desktop_ia"
            | "_x_foreground"
            | "_x_frontend_build_type"
            | "_x_gantry"
            | "_x_num_retries"
            | "_x_version_ts"
            | "slack_route" => value.into_owned(),
            _ => return None,
        };
        if query.insert(key.into_owned(), value).is_some() {
            return None;
        }
    }
    if query.len() != 10 {
        return None;
    }
    url.set_query(None);
    Some((url, query))
}
pub(super) fn wire(request: &PreparedRequest) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    let args = fields
        .iter()
        .filter(|(k, _)| k != "token")
        .cloned()
        .collect::<BTreeMap<_, _>>();
    Ok(json!({"method":request.method,"path":url.path(),"args":args}))
}
pub(crate) async fn update(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    intent: OwnEditIntent,
) -> Result<OwnEditAccepted, ListCommentUpdateFailure> {
    use ListCommentUpdateFailure as Failure;
    let model = {
        let shell = state.peek();
        let session = session(&shell).map_err(Failure::NotRequested)?;
        let expected = api::list_record_comment_update(&shell.core.client, &session, &intent)
            .map_err(Failure::NotRequested)?;
        if !same(request, &expected)
            || intent.record().ts() != RECEIVED_TS
            || intent.record().target().channel_id() != "CCOMMENT01"
        {
            return Err(Failure::NotRequested(refused()));
        }
        model(&shell).map_err(Failure::NotRequested)?
    };
    let (mode, raw, wait) = {
        let mut l = model
            .0
            .lock()
            .map_err(|_| Failure::NotRequested(refused()))?;
        let l = l
            .own_edit
            .as_mut()
            .ok_or_else(|| Failure::NotRequested(refused()))?;
        let form = wire(request).map_err(Failure::NotRequested)?;
        l.prepared.push(form.clone());
        let mode = std::mem::take(&mut l.mode);
        if mode == Mode::NotRequested {
            l.responses
                .push(json!({"disposition":"not-requested","raw":null}));
            return Err(Failure::NotRequested(Error::Offline(
                "Closed edit was not dispatched".into(),
            )));
        }
        l.updates.push(form);
        if mode == Mode::Rejected {
            let raw = json!({"ok":false,"error":"cant_update_message"});
            l.responses
                .push(json!({"disposition":"rejected","raw":raw}));
            return Err(Failure::Rejected(raw));
        }
        let record = intent.record();
        let mut row = record.seed().original_raw().clone();
        row["blocks"] = json!(record.blocks());
        row["text"] = json!(record.text());
        row["edited"] = json!({"user":record.user(),"ts":"1781525800.000001"});
        let raw = json!({"ok":true,"channel":record.target().channel_id(),"ts":record.ts(),"message":row});
        l.candidates.push(row);
        l.responses.push(json!({"disposition":if mode==Mode::Unconfirmed {"unconfirmed"} else {"accepted"},"raw":if mode==Mode::Unconfirmed {Value::Null} else {raw.clone()}}));
        let wait = if mode == Mode::HeldSuccess {
            let (tx, rx) = oneshot::channel();
            l.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (mode, raw, wait)
    };
    if let Some(wait) = wait {
        wait.await.map_err(|_| Failure::Unconfirmed {
            error: refused(),
            receipt: None,
        })?;
    }
    if mode == Mode::Unconfirmed {
        return Err(Failure::Unconfirmed {
            error: Error::Transport("Closed edit acknowledgement lost".into()),
            receipt: None,
        });
    }
    intent
        .accept(raw.clone())
        .map_err(|_| Failure::Unconfirmed {
            error: refused(),
            receipt: Some(raw),
        })
}
