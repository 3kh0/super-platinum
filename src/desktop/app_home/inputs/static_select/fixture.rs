//! Closed input-event fixture: records the real prepared blocks.actions request.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
pub(crate) mod check;

pub(crate) struct Harness {
    started: Instant,
    events: Vec<Value>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    refusals: Vec<Value>,
    reply: Reply,
    pending: BTreeMap<u64, oneshot::Sender<Result<Value, Error>>>,
    gate: Option<oneshot::Sender<()>>,
    hold_gate: bool,
    gate_waiting: bool,
}
#[derive(Clone, Copy)]
pub(crate) enum Reply {
    Accepted,
    Held,
    Offline,
    RateLimited,
    Rejected,
    Uncertain,
}
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-static-dispatch" | "app-home-static-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_STATIC_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "selection" | "pending" | "retirement")
}
fn recording() -> bool {
    named()
        || check::remount::named()
        || wrappers::fixture::named()
        || multiple::fixture::named()
        || checkboxes::fixture::named()
        || radio::fixture::named()
        || super::super::native::date::fixture::named()
        || super::super::native::date::focus_fixture::named()
        || super::super::native::date::popup_fixture::named()
        || super::super::native::time::fixture::named()
        || super::super::native::time::focus_fixture::named()
        || super::super::native::datetime::fixture::named()
        || super::super::native::datetime::editing_fixture::named()
        || super::super::native::datetime::popup_fixture::named()
        || super::super::users::dispatch::fixture::named()
        || super::super::external::dispatch::fixture::named()
        || super::super::external::multi_dispatch_fixture::named()
}
pub(crate) fn case() -> String {
    if super::super::native::datetime::popup_fixture::named() {
        return super::super::native::datetime::popup_fixture::case();
    }
    if super::super::native::date::popup_fixture::named() {
        return super::super::native::date::popup_fixture::case();
    }
    if super::super::native::datetime::editing_fixture::named() {
        return super::super::native::datetime::editing_fixture::case();
    }
    if super::super::native::time::focus_fixture::named() {
        return super::super::native::time::focus_fixture::case();
    }
    if super::super::native::date::focus_fixture::named() {
        return super::super::native::date::focus_fixture::case();
    }
    if super::super::external::multi_dispatch_fixture::named() {
        return super::super::external::multi_dispatch_fixture::case();
    }
    if super::super::external::dispatch::fixture::named() {
        return super::super::external::dispatch::fixture::case();
    }
    if super::super::users::dispatch::fixture::named() {
        return super::super::users::dispatch::fixture::case();
    }
    if super::super::native::datetime::fixture::named() {
        return super::super::native::datetime::fixture::case();
    }
    if super::super::native::time::fixture::named() {
        return super::super::native::time::fixture::case();
    }
    if super::super::native::date::fixture::named() {
        return super::super::native::date::fixture::case();
    }
    if radio::fixture::named() {
        return radio::fixture::case();
    }
    if checkboxes::fixture::named() {
        return checkboxes::fixture::case();
    }
    if multiple::fixture::named() {
        return multiple::fixture::case();
    }
    if wrappers::fixture::named() {
        return wrappers::fixture::case();
    }
    if check::remount::named() {
        return "remount".into();
    }
    std::env::var("SUPER_PLATINUM_APP_HOME_STATIC_DISPATCH_CASE").unwrap_or_default()
}
pub(super) fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(super) fn option(value: &str, label: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label,"emoji":true},"value":value,"description":{"type":"plain_text","text":format!("{label} release")},"opaque":{"keep":[value,{"source":"static"}]}})
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::fixture::install_inputs(s);
    let a = option("alpha", "Alpha");
    let b = option("beta", "Beta");
    let g = option("gamma", "Gamma");
    let empty = option("", "No release");
    let select = |initial: Value| {
        json!({"type":"static_select","action_id":"value","placeholder":{"type":"plain_text","text":"Choose release"},"initial_option":initial,
   "option_groups":[{"label":{"type":"plain_text","text":"Releases"},"options":[a.clone(),b.clone(),g.clone(),empty.clone()]}]})
    };
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
    {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release choice"},"element":select(a.clone())},
    {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required choice"},"element":select(a.clone())},
    {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet choice"},"element":select(a.clone())},
    {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
    {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
    {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    initialize(s);
}
pub(crate) fn initialize(s: &mut ShellState) {
    s.app_home.static_select.fixture = Some(Harness {
        started: Instant::now(),
        events: vec![],
        requests: vec![],
        responses: vec![],
        refusals: vec![],
        reply: Reply::Accepted,
        pending: BTreeMap::new(),
        gate: None,
        hold_gate: false,
        gate_waiting: false,
    });
}
pub(crate) fn event(s: &mut ShellState, e: &impl Serialize, k: &InputKey) {
    if !recording() {
        return;
    }
    if let Some(f) = s.app_home.static_select.fixture.as_mut() {
        f.events.push(
            json!({"field":k.block_id,"event":e,"elapsedMs":f.started.elapsed().as_millis()}),
        );
    }
}
fn result(reply: Reply) -> Result<Value, Error> {
    match reply {
        Reply::Accepted | Reply::Held => Ok(json!({"ok":true})),
        Reply::Offline => Err(Error::Api("block_interaction_not_sent_offline".into())),
        Reply::RateLimited => Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }),
        Reply::Rejected => Ok(
            json!({"ok":false,"error":"invalid_arguments","message":"Please revise this selection."}),
        ),
        Reply::Uncertain => Err(Error::Api("static_fixture_unconfirmed".into())),
    }
}
pub(super) fn request(
    s: &mut ShellState,
    q: &Queued,
    r: &PreparedRequest,
) -> Option<oneshot::Receiver<Result<Value, Error>>> {
    if !crate::fixture::is_fixture() {
        return None;
    }
    let (tx, rx) = oneshot::channel();
    if !recording() {
        let _ = tx.send(Err(Error::Api("block_interaction_not_sent_offline".into())));
        return Some(rx);
    }
    let Some(f) = s.app_home.static_select.fixture.as_mut() else {
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
    f.requests.push(json!({"id":q.id,"method":r.method,"endpoint":r.url,"args":args,"action":q.action.action(),"eventCount":f.events.len(),"queuedForMs":q.at.elapsed().as_millis(),"edge":q.edge,"elapsedMs":f.started.elapsed().as_millis(),"inputRevision":s.app_home.input_revisions.get(&q.binding.form),"visit":q.binding.visit.visit,"revision":q.binding.visit.revision,"socket":q.binding.owner.rt}));
    if matches!(f.reply, Reply::Held) {
        f.pending.insert(q.id, tx);
    } else {
        let _ = tx.send(result(f.reply));
    }
    Some(rx)
}
pub(super) fn response(s: &mut ShellState, id: u64, r: &Result<api::BlockActionResponse, Error>) {
    if !recording() {
        return;
    }
    if let Some(f) = s.app_home.static_select.fixture.as_mut() {
        let outcome = match r {
            Ok(api::BlockActionResponse::Accepted) => "accepted",
            Ok(api::BlockActionResponse::Rejected(_)) => "rejected",
            Err(Error::RateLimited { .. }) => "rate-limited",
            Err(Error::Api(v)) if v == "block_interaction_not_sent_offline" => "offline",
            Err(_) => "uncertain",
        };
        f.responses
            .push(json!({"id":id,"outcome":outcome,"elapsedMs":f.started.elapsed().as_millis()}));
    }
}
pub(crate) fn configure(s: &mut ShellState, reply: Reply) {
    s.app_home.static_select.fixture.as_mut().unwrap().reply = reply;
}
pub(crate) fn release(s: &mut ShellState, id: u64, reply: Reply) -> bool {
    s.app_home
        .static_select
        .fixture
        .as_mut()
        .and_then(|f| f.pending.remove(&id))
        .is_some_and(|tx| tx.send(result(reply)).is_ok())
}
pub(super) async fn before_dispatch(mut state: Signal<ShellState>) {
    if !recording() {
        return;
    }
    let rx = {
        let mut s = state.write();
        s.app_home
            .static_select
            .fixture
            .as_mut()
            .filter(|f| f.hold_gate)
            .map(|f| {
                let (tx, rx) = oneshot::channel();
                f.gate_waiting = true;
                f.gate = Some(tx);
                rx
            })
    };
    if let Some(rx) = rx {
        let _ = tokio::time::timeout(Duration::from_secs(8), rx).await;
    }
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let mut out = super::super::external::fixture::snapshot(s);
    let f = s.app_home.static_select.fixture.as_ref();
    out["named"] = json!(recording());
    out["case"] = json!(case());
    out["requests"] = json!(f.map(|f| &f.requests));
    out["responses"] = json!(f.map(|f| &f.responses));
    out["events"] = json!(f.map(|f| &f.events));
    out["refusals"] = json!(f.map(|f| &f.refusals));
    out["pendingIds"] = json!(f.map(|f| f.pending.keys().copied().collect::<Vec<_>>()));
    out["gateWaiting"] = json!(f.is_some_and(|f| f.gate_waiting));
    out["cooling"] = json!(
        s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.owner.current(s) && cooling(s, &h.owner.ctx))
    );
    out["fields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)|json!({"block":k.block_id,"action":k.action_id,"value":match &v.value{InputValue::Selected(v)=>json!(v),InputValue::Text(v)=>json!(v),InputValue::Date(v)=>json!(v),_=>Value::Null},"error":v.error,"dirty":v.dirty,"dispatch":format!("{:?}",f.dispatch(k))})).collect::<Vec<_>>()));
    out["widgets"]=json!(owner(s).map(|o|s.app_home.static_select.widgets.iter().filter(|((f,_),_)|f==&o).map(|((_,k),w)|json!({"block":k.block_id,"visit":w.visit,"initial":w.initial,"options":w.options,"visible":w.visible})).collect::<Vec<_>>()));
    out["windows"]=json!(s.app_home.static_select.windows.values().map(|w|json!({"id":w.id,"block":w.binding.key.block_id,"current":w.binding.current(s),"trailing":w.trailing.as_ref().map(|q|json!({"id":q.id,"edge":q.edge,"action":q.action.action(),"mount":q.mount,"current":q.current(s)}))})).collect::<Vec<_>>());
    out["mounts"]=json!(s.app_home.static_select.mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"value":m.value,"block":m.binding.key.block_id,"current":m.binding.current(s)})).collect::<Vec<_>>());
    out["activity"]=json!(s.app_home.static_select.activity.values().map(|a|json!({"block":a.binding.key.block_id,"loading":a.loading,"error":a.error,"current":a.binding.current(s)&&a.route.current(s)})).collect::<Vec<_>>());
    if radio::fixture::named() {
        let proof = radio::proof(s);
        out["radioWidgets"] = proof["widgets"].clone();
        out["radioMounts"] = proof["mounts"].clone();
    }
    if checkboxes::fixture::named() {
        let proof = checkboxes::proof(s);
        out["checkboxWidgets"] = proof["widgets"].clone();
        out["checkboxMounts"] = proof["mounts"].clone();
    }
    if multiple::fixture::named() {
        let proof = multiple::proof(s);
        out["multiWidgets"] = proof["widgets"].clone();
        out["multiMounts"] = proof["mounts"].clone();
    }
    out["context"] = context::Witness::capture(s)
        .ok()
        .map(|w| w.proof(s))
        .unwrap_or(Value::Null);
    out
}

pub(super) fn refused(s: &mut ShellState, q: &Queued, reason: &str) {
    if !recording() {
        return;
    }
    let current = q.current(s);
    if let Some(f) = s.app_home.static_select.fixture.as_mut() {
        f.refusals.push(
            json!({"id":q.id,"block":q.binding.key.block_id,"reason":reason,"current":current}),
        );
    }
}

pub(crate) fn arm_gate(s: &mut ShellState) {
    let f = s.app_home.static_select.fixture.as_mut().unwrap();
    f.hold_gate = true;
    f.gate_waiting = false;
}
pub(crate) fn release_gate(s: &mut ShellState) {
    let f = s.app_home.static_select.fixture.as_mut().unwrap();
    f.hold_gate = false;
    f.gate_waiting = false;
    if let Some(tx) = f.gate.take() {
        let _ = tx.send(());
    }
}
