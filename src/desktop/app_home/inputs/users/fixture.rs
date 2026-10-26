//! Named disconnected Home user fixture. Reads intercept the shared prepared request.
use super::*;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
mod capture;
pub(crate) mod check;
pub(crate) mod multiple;
pub(crate) const REMOTE: &str = "UHOMERMT1";
pub(crate) const ADA: &str = "UHOMEADA1";
pub(crate) const BLAIR: &str = "UHOMEBLR1";
pub(crate) const CASEY: &str = "UHOMECSY1";
#[derive(Clone, Copy)]
pub(crate) enum Reply {
    People,
    Offline,
    RateLimited,
}
pub(crate) struct Harness {
    reply: Reply,
    hold: bool,
    pending: Option<tokio::sync::oneshot::Sender<Result<Value, Error>>>,
    requests: Vec<Value>,
    releases: Vec<Value>,
}
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-user-select"
                | "app-home-user-select-narrow"
                | "app-home-multi-user-select"
                | "app-home-multi-user-select-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_USER_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-user-select" | "app-home-user-select-narrow")
        )
        && matches!(
            case().as_str(),
            "selection" | "props" | "failure" | "retirement"
        )
}
pub(crate) fn case() -> String {
    if multiple::requested() {
        return multiple::case();
    }
    std::env::var("SUPER_PLATINUM_APP_HOME_USER_SELECT_CASE").unwrap_or_default()
}
pub(crate) fn key() -> InputKey {
    InputKey {
        block_id: "home-choice".into(),
        action_id: "value".into(),
    }
}
pub(crate) fn members() -> Vec<Value> {
    [(ADA,"ada","Ada Lovelace"),(BLAIR,"blair","Blair Chen"),(CASEY,"casey","Casey Patel")].into_iter()
        .map(|(id,name,label)|json!({"id":id,"name":name,"real_name":label,"profile":{"display_name":label,"real_name":label},"opaque":{"fixture":"known-member"}})).collect()
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::context::fixture::install(s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team, case() == "retirement");
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(true);
    block["label"]["text"] = json!("Release reviewer");
    block["element"] = json!({"type":"users_select","action_id":"value","initial_user":ADA,"placeholder":{"type":"plain_text","text":"Find a person"}});
    if multiple::requested() {
        block["label"]["text"] = json!("Release reviewers");
        block["element"] = json!({"type":"multi_users_select","action_id":"value","initial_users":[ADA],"max_selected_items":2,"placeholder":{"type":"plain_text","text":"Find people"}});
        block["optional"] = json!(case() != "selection");
    }
    if case() == "props" && !multiple::requested() {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"users_select","selected_user":null}}}});
    }
    if case() == "props" && multiple::requested() {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_users_select","selected_users":[]}}}});
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    h.opened = true;
    super::super::hydrate(s);
    s.app_home.users.fixture = Some(Harness {
        reply: Reply::People,
        hold: false,
        pending: None,
        requests: vec![],
        releases: vec![],
    });
}
fn response(reply: Reply) -> Result<Value, Error> {
    match reply {
        Reply::People => Ok(
            json!({"ok":true,"results":[members()[0].clone(),members()[1].clone(),json!({"id":REMOTE,"name":"remote","real_name":"Remote Reviewer","profile":{"display_name":"Remote Reviewer","real_name":"Remote Reviewer"},"opaque":{"source":"remote-only"}})]}),
        ),
        Reply::Offline => Err(Error::Offline("Home member read unavailable".into())),
        Reply::RateLimited => Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }),
    }
}
fn proof(reply: Reply) -> Value {
    match reply {
        Reply::People => response(reply).unwrap(),
        Reply::Offline => json!({"error":"offline"}),
        Reply::RateLimited => json!({"error":"rate_limited","retryAfterSeconds":1}),
    }
}
pub(super) fn request(
    s: &mut ShellState,
    t: &Ticket,
    request: &PreparedRequest,
) -> Option<tokio::sync::oneshot::Receiver<Result<Value, Error>>> {
    if !crate::fixture::is_fixture() {
        return None;
    }
    let current =
        (named() || multiple::named() || super::dispatch::fixture::named()) && t.current(s);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let Some(f) = s.app_home.users.fixture.as_mut() else {
        let _ = tx.send(Err(Error::Offline("Home member read unavailable".into())));
        return Some(rx);
    };
    if !current {
        let _ = tx.send(Err(Error::Offline("Home member read unavailable".into())));
        return Some(rx);
    }
    let RequestBody::Json(body) = &request.body else {
        let _ = tx.send(Err(Error::Offline("Home member read unavailable".into())));
        return Some(rx);
    };
    let mut body = body.clone();
    if let Some(body) = body.as_object_mut() {
        body.remove("token");
        body.remove("enterprise_token");
    }
    f.requests.push(json!({"method":request.method,"endpoint":request.url.split('?').next(),"body":body,
        "accountTeam":t.owner.ctx.session.team_id,"enterprise":t.owner.ctx.session.enterprise_id,
        "homeContext":t.context.team_id(),"homeVisit":t.surface.visit,"homeRevision":t.surface.revision,
        "inputRevision":t.input_revision,"widgetRevision":t.widget_revision,"socket":t.owner.rt,"reply":proof(f.reply),"held":f.hold}));
    if f.hold {
        f.pending = Some(tx);
    } else {
        let _ = tx.send(response(f.reply));
    }
    Some(rx)
}
pub(crate) fn configure(s: &mut ShellState, reply: Reply, hold: bool) {
    let f = s.app_home.users.fixture.as_mut().unwrap();
    f.reply = reply;
    f.hold = hold;
}
pub(crate) fn pending_closed(s: &ShellState) -> bool {
    s.app_home
        .users
        .fixture
        .as_ref()
        .is_some_and(|f| f.pending.as_ref().is_some_and(|p| p.is_closed()))
}
pub(crate) fn release(s: &mut ShellState) -> Value {
    let f = s.app_home.users.fixture.as_mut().unwrap();
    let delivered = f
        .pending
        .take()
        .is_some_and(|p| p.send(response(f.reply)).is_ok());
    f.hold = false;
    let v = json!({"delivered":delivered,"reply":proof(f.reply)});
    f.releases.push(v.clone());
    v
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let mut v = super::super::external::fixture::snapshot(s);
    let f = s.app_home.users.fixture.as_ref();
    let form_owner = super::super::owner(s);
    let widget = form_owner
        .as_ref()
        .and_then(|o| s.app_home.users.widgets.get(&(o.clone(), key())));
    let p = s.app_home.users.picker.as_ref();
    let team = s.core.active_team.as_deref();
    v["named"] = json!(named() || multiple::named());
    v["case"] = json!(case());
    v["widget"]=widget.map(|w|json!({"visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,"retainedLabel":w.retained_label})).unwrap_or(Value::Null);
    if multiple::requested() {
        v["widget"]=form_owner.as_ref().and_then(|o|s.app_home.users.multiples.get(&(o.clone(),key())))
            .map(|w|json!({"visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,"retainedLabels":w.retained_labels,"invalid":w.invalid,
                "presentation":super::multiple::presentation(s,&key())})).unwrap_or(Value::Null);
    }
    v["requests"] = json!(f.map(|f| &f.requests));
    v["releases"] = json!(f.map(|f| &f.releases));
    v["pending"] = json!(f.is_some_and(|f| f.pending.as_ref().is_some_and(|p| !p.is_closed())));
    v["query"]=p.map(|p|json!({"current":p.ticket.current(s),"serial":p.serial,"revision":p.query_revision,"text":p.query,
        "loading":p.loading,"error":p.error,"rows":p.candidates.iter().map(|c|json!({"id":c.id,"label":c.label})).collect::<Vec<_>>()})).unwrap_or(Value::Null);
    v["members"] = json!(
        team.and_then(|t| s.core.workspaces.get(t))
            .map(|w| &w.users)
    );
    v["context"] = crate::app_home::context::Witness::capture(s)
        .ok()
        .map(|w| w.proof(s))
        .unwrap_or(Value::Null);
    v["cooling"] = json!(team.is_some_and(|t| directory::cooling(s, s.core.account_epoch, t)));
    v
}
pub(crate) fn refresh(
    s: &mut ShellState,
    optional: bool,
    initial: Option<Option<&str>>,
    saved: Option<Option<&str>>,
) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("user-refresh-{before}"));
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(optional);
    if let Some(initial) = initial {
        if let Some(id) = initial {
            block["element"]["initial_user"] = json!(id);
        } else {
            block["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_user");
        }
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"users_select","selected_user":saved}}}});
    }
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw.clone()}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    if !s.app_home.surface.as_ref().is_some_and(|h| {
        h.owner.current(s)
            && h.revision != before
            && h.view.as_ref().is_some_and(|v| v.raw() == &raw)
    }) {
        return Err("Owned users Home refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
