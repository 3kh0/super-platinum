//! Closed interception of the actual Home Button attempt/prepared request.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
pub(crate) mod check;
pub(crate) struct Harness {
    started: Instant,
    requests: Vec<Value>,
    responses: Vec<Value>,
    refusals: Vec<Value>,
    pending: BTreeMap<u64, oneshot::Sender<Result<Value, Error>>>,
    pub(super) hold_gate: bool,
    pub(super) waiting: bool,
    gate: Option<oneshot::Sender<()>>,
}
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-text-button-sync" | "app-home-text-button-sync-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_BUTTON_SYNC_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "admission" | "pending")
}
pub(crate) fn intercepts() -> bool {
    named()
        || inputs::static_select::wrappers::fixture::named()
        || inputs::static_select::multiple::fixture::named()
        || inputs::static_select::checkboxes::fixture::named()
        || inputs::static_select::radio::fixture::named()
        || inputs::native::date::fixture::named()
        || inputs::native::date::focus_fixture::named()
        || inputs::native::date::popup_fixture::named()
        || inputs::native::time::fixture::named()
        || inputs::native::time::focus_fixture::named()
        || inputs::native::datetime::fixture::named()
        || inputs::native::datetime::editing_fixture::named()
        || inputs::native::datetime::popup_fixture::named()
        || inputs::users::dispatch::fixture::named()
        || inputs::external::dispatch::fixture::named()
        || inputs::external::multi_dispatch_fixture::named()
        || inputs::number_fixture::named()
        || inputs::email_fixture::named()
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_BUTTON_SYNC_CASE").unwrap_or_default()
}
pub(crate) fn install(s: &mut ShellState) {
    super::super::fixture::install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]["min_length"] = json!(1);
    raw["blocks"].as_array_mut().unwrap().push(json!({"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}));
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    initialize(s);
}
pub(crate) fn initialize(s: &mut ShellState) {
    s.app_home.text.button_fixture = Some(Harness {
        started: Instant::now(),
        requests: vec![],
        responses: vec![],
        refusals: vec![],
        pending: BTreeMap::new(),
        hold_gate: false,
        waiting: false,
        gate: None,
    });
}
pub(crate) fn request(
    s: &mut ShellState,
    id: u64,
    key: actions::Key,
    r: &PreparedRequest,
) -> Option<oneshot::Receiver<Result<Value, Error>>> {
    if !crate::fixture::is_fixture() {
        return None;
    }
    let (tx, rx) = oneshot::channel();
    if !intercepts() {
        let _ = tx.send(Err(Error::Api("block_interaction_not_sent_offline".into())));
        return Some(rx);
    }
    let Some(f) = s.app_home.text.button_fixture.as_mut() else {
        let _ = tx.send(Err(Error::Api("block_interaction_not_sent_offline".into())));
        return Some(rx);
    };
    let RequestBody::Form(fields) = &r.body else {
        let _ = tx.send(Err(Error::Api("invalid_fixture_request".into())));
        return Some(rx);
    };
    let args = fields
        .iter()
        .filter(|(k, _)| {
            !matches!(k.as_str(), "token" | "client_token")
                && (!k.starts_with("_x_") || k == "_x_reason")
        })
        .map(|(k, v)| {
            (
                k.clone(),
                if matches!(k.as_str(), "actions" | "state" | "container") {
                    serde_json::from_str(v).unwrap()
                } else {
                    json!(v)
                },
            )
        })
        .collect::<serde_json::Map<_, _>>();
    f.requests.push(json!({"id":id,"method":r.method,"endpoint":r.url,"args":args,"visit":key.visit,"revision":key.revision,"elapsedMs":f.started.elapsed().as_millis()}));
    f.pending.insert(id, tx);
    Some(rx)
}
pub(crate) fn response(s: &mut ShellState, id: u64, r: &Result<api::BlockActionResponse, Error>) {
    if !intercepts() {
        return;
    }
    if let Some(f) = s.app_home.text.button_fixture.as_mut() {
        let outcome = match r {
            Ok(api::BlockActionResponse::Accepted) => "accepted",
            Ok(_) => "rejected",
            Err(Error::Api(v)) if v == "block_interaction_not_sent_offline" => "offline",
            Err(_) => "uncertain",
        };
        f.responses.push(json!({"id":id,"outcome":outcome}));
    }
}
pub(crate) fn refused(s: &mut ShellState, id: u64, reason: &str) {
    if !intercepts() {
        return;
    }
    if let Some(f) = s.app_home.text.button_fixture.as_mut() {
        f.refusals.push(json!({"id":id,"reason":reason}));
    }
}
pub(crate) fn release(s: &mut ShellState, accepted: bool) -> bool {
    let Some(f) = s.app_home.text.button_fixture.as_mut() else {
        return false;
    };
    let Some(id) = f.pending.keys().next().copied() else {
        return false;
    };
    f.pending
        .remove(&id)
        .unwrap()
        .send(if accepted {
            Ok(json!({"ok":true}))
        } else {
            Err(Error::Api("button_fixture_unconfirmed".into()))
        })
        .is_ok()
}
pub(crate) async fn after_native_lock(mut state: Signal<ShellState>) {
    if !named() {
        return;
    }
    let rx = {
        let mut s = state.write();
        s.app_home
            .text
            .button_fixture
            .as_mut()
            .filter(|f| f.hold_gate)
            .map(|f| {
                let (tx, rx) = oneshot::channel();
                f.waiting = true;
                f.gate = Some(tx);
                rx
            })
    };
    if let Some(rx) = rx {
        let _ = tokio::time::timeout(Duration::from_secs(8), rx).await;
    }
}
pub(super) fn release_gate(s: &mut ShellState) {
    if let Some(f) = s.app_home.text.button_fixture.as_mut() {
        f.hold_gate = false;
        f.waiting = false;
        if let Some(tx) = f.gate.take() {
            let _ = tx.send(());
        }
    }
}
pub(crate) fn augment(s: &ShellState, out: &mut Value) {
    if !intercepts() {
        return;
    }
    out["named"] = json!(true);
    if named() {
        out["case"] = json!(case());
    }
    let f = s.app_home.text.button_fixture.as_ref();
    out["buttonSync"] = json!({"requests":f.map(|f|&f.requests),"responses":f.map(|f|&f.responses),"refusals":f.map(|f|&f.refusals),"pendingIds":f.map(|f|f.pending.keys().copied().collect::<Vec<_>>()),"gateWaiting":f.is_some_and(|f|f.waiting),"attempts":actions::text_button_fixture_attempts(s),"correlationCount":s.app_home.correlations.len(),"notice":s.app_home.surface.as_ref().and_then(|h|h.notice.as_ref())});
}
