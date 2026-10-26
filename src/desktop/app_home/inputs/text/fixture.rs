//! Closed input-event fixture: records the real prepared blocks.actions request.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
pub(crate) mod admission;
pub(crate) mod check;
pub(crate) mod focus;

pub(crate) struct Harness {
    started: Instant,
    events: Vec<Value>,
    requests: Vec<Value>,
    responses: Vec<Value>,
    reply: Reply,
    pending: BTreeMap<u64, oneshot::Sender<Result<Value, Error>>>,
    gate: Option<oneshot::Sender<()>>,
    hold_gate: bool,
    gate_waiting: bool,
    admission: admission::State,
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
            Ok("app-home-text-dispatch" | "app-home-text-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "events" | "state" | "lifecycle" | "admission"
        )
}
fn recording() -> bool {
    named()
        || super::button::fixture::named()
        || focus::named()
        || crate::app_home::inputs::number_fixture::named()
        || crate::app_home::inputs::email_fixture::named()
        || crate::app_home::inputs::focus_fixture::named()
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_DISPATCH_CASE").unwrap_or_default()
}
pub(super) fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::fixture::install_inputs(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
      {"type":"input","block_id":"enter","dispatch_action":true,"label":{"type":"plain_text","text":"Release text"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Initial","min_length":2,"max_length":24}},
      {"type":"input","block_id":"characters","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Live notes"},"element":{"type":"plain_text_input","action_id":"value","multiline":true,"dispatch_action_config":{"trigger_actions_on":["on_character_entered"]}}},
      {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet sibling"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
      {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}}
    ]);
    if case() == "state" {
        raw["blocks"][0]["element"]["min_length"] = json!(1);
    }
    if case() == "admission" {
        raw["blocks"][3]["element"]["initial_date"] = json!("2026-10-25");
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    initialize_observation(s);
}
/// Allocate the existing closed observer ledger without installing a Text view.
pub(crate) fn initialize_observation(s: &mut ShellState) {
    s.app_home.text.fixture = Some(Harness {
        started: Instant::now(),
        events: vec![],
        requests: vec![],
        responses: vec![],
        reply: Reply::Accepted,
        pending: BTreeMap::new(),
        gate: None,
        hold_gate: false,
        gate_waiting: false,
        admission: admission::State::default(),
    });
}
pub(super) fn event(s: &mut ShellState, e: &Occurrence, k: &InputKey) {
    if !recording() {
        return;
    }
    if let Some(f) = s.app_home.text.fixture.as_mut() {
        f.events.push(
            json!({"field":k.block_id,"event":e,"elapsedMs":f.started.elapsed().as_millis()}),
        );
    }
}
pub(crate) fn number_event(s: &mut ShellState, event: &Value, key: &InputKey) {
    if !crate::app_home::inputs::number_fixture::named()
        && !crate::app_home::inputs::focus_fixture::named()
    {
        return;
    }
    if let Some(f) = s.app_home.text.fixture.as_mut() {
        f.events.push(
            json!({"field":key.block_id,"event":event,"elapsedMs":f.started.elapsed().as_millis()}),
        );
    }
}
pub(crate) fn email_event(s: &mut ShellState, event: &Value, key: &InputKey) {
    if !crate::app_home::inputs::email_fixture::named()
        && !crate::app_home::inputs::focus_fixture::named()
    {
        return;
    }
    if let Some(f) = s.app_home.text.fixture.as_mut() {
        f.events.push(
            json!({"field":key.block_id,"event":event,"elapsedMs":f.started.elapsed().as_millis()}),
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
        Reply::Rejected => {
            Ok(json!({"ok":false,"error":"invalid_arguments","message":"Please revise this text."}))
        }
        Reply::Uncertain => Err(Error::Api("text_fixture_unconfirmed".into())),
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
    let Some(f) = s.app_home.text.fixture.as_mut() else {
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
    f.requests.push(json!({"id":q.id,"method":r.method,"endpoint":r.url,"args":args,"action":q.action.action(),"queuedForMs":q.at.elapsed().as_millis(),"elapsedMs":f.started.elapsed().as_millis(),"inputRevision":s.app_home.input_revisions.get(&q.binding.form),"visit":q.binding.visit.visit,"revision":q.binding.visit.revision,"socket":q.binding.owner.rt}));
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
    if let Some(f) = s.app_home.text.fixture.as_mut() {
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
    s.app_home.text.fixture.as_mut().unwrap().reply = reply;
}
pub(crate) fn release(s: &mut ShellState, id: u64, reply: Reply) -> bool {
    s.app_home
        .text
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
            .text
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
    let f = s.app_home.text.fixture.as_ref();
    out["named"] = json!(named());
    out["case"] = json!(case());
    out["requests"] = json!(f.map(|f| &f.requests));
    out["responses"] = json!(f.map(|f| &f.responses));
    out["events"] = json!(f.map(|f| &f.events));
    out["pendingIds"] = json!(f.map(|f| f.pending.keys().copied().collect::<Vec<_>>()));
    out["gateWaiting"] = json!(f.is_some_and(|f| f.gate_waiting));
    out["cooling"] = json!(
        s.app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.owner.current(s) && cooling(s, &h.owner.ctx))
    );
    out["fields"]=json!(form(s).map(|f|f.fields.iter().map(|(k,v)|json!({"block":k.block_id,"action":k.action_id,"literal":match &v.value{InputValue::Text(t)=>Some(t),_=>None},"error":v.error,"dirty":v.dirty,"dispatch":format!("{:?}",f.dispatch(k))})).collect::<Vec<_>>()));
    out["queue"]=json!(s.app_home.text.queued.values().map(|q|json!({"id":q.id,"block":q.binding.key.block_id,"action":q.action.action(),"mount":q.mount,"current":q.current(s)})).collect::<Vec<_>>());
    out["textMounts"]=json!(s.app_home.text.mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"value":m.value,"block":m.binding.key.block_id,"current":m.binding.current(s)})).collect::<Vec<_>>());
    out["activity"]=json!(s.app_home.text.activity.values().map(|a|json!({"block":a.binding.key.block_id,"loading":a.loading,"error":a.error,"current":a.binding.current(s)&&a.route.current(s)})).collect::<Vec<_>>());
    out["context"] = context::Witness::capture(s)
        .ok()
        .map(|w| w.proof(s))
        .unwrap_or(Value::Null);
    super::button::fixture::augment(s, &mut out);
    if focus::named() {
        out["named"] = json!(true);
        out["case"] = json!(focus::case());
        out["focusDiagnostic"] = json!(true);
    }
    admission::augment(s, &mut out);
    out
}
