//! Named disconnected Home channel fixture. Reads intercept the shared prepared request.
use super::*;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
pub(super) mod bundle;
pub(super) mod capture;
pub(crate) mod check;
pub(super) mod exchange;
pub(crate) mod multiple;
#[cfg(test)]
pub(super) mod test_support;
pub(crate) const REMOTE: &str = "CCHANRMT1";
pub(crate) const ALPHA: &str = "CCHANAL01";
pub(crate) const BETA: &str = "CCHANBE01";
pub(crate) const GAMMA: &str = "CCHANGA01";
#[derive(Clone, Copy)]
pub(crate) enum Reply {
    Channels,
    Conversations,
    Offline,
    RateLimited,
}
pub(crate) struct Harness {
    reply: Reply,
    hold: bool,
    pending: Option<tokio::sync::oneshot::Sender<Result<Value, Error>>>,
    pending_owner: Option<u64>,
    lane: std::sync::Arc<tokio::sync::Mutex<()>>,
    requests: Vec<Value>,
    releases: Vec<Value>,
}
pub(crate) fn is_multiple() -> bool {
    multiple::requested() || conversation::fixture::multiple::requested()
}
pub(crate) fn requested() -> bool {
    if crate::app_home::inputs::users::dispatch::fixture::channel_requested()
        || multiple::requested()
        || conversation::fixture::requested()
    {
        return true;
    }
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-channel-select" | "app-home-channel-select-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_CHANNEL_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-channel-select" | "app-home-channel-select-narrow")
        )
        && matches!(
            case().as_str(),
            "selection" | "props" | "failure" | "retirement"
        )
}
pub(crate) fn case() -> String {
    if crate::app_home::inputs::users::dispatch::fixture::channel_requested() {
        return crate::app_home::inputs::users::dispatch::fixture::case();
    }
    if conversation::fixture::requested() {
        return conversation::fixture::case();
    }
    if multiple::requested() {
        return multiple::case();
    }
    std::env::var("SUPER_PLATINUM_APP_HOME_CHANNEL_SELECT_CASE").unwrap_or_default()
}
pub(crate) fn key() -> InputKey {
    InputKey {
        block_id: "home-choice".into(),
        action_id: "value".into(),
    }
}
pub(crate) fn channels() -> Vec<Value> {
    if conversation::fixture::requested() {
        return conversation::fixture::channels();
    }
    let mut rows:Vec<_>=[(ALPHA,"alpha",vec!["TWORK0002"]),(BETA,"beta",vec!["TWORK0002","TWORK0003"]),(GAMMA,"gamma",vec!["TWORK0003"])]
        .into_iter().map(|(id,name,teams)|json!({"id":id,"name":name,"is_channel":true,"is_member":true,"internal_team_ids":teams,"opaque":{"fixture":"known-public-channel"}})).collect();
    rows.push(json!({"id":"CCHANARC1","name":"archived","is_channel":true,"is_archived":true,"internal_team_ids":["TWORK0002"]}));
    rows.push(json!({"id":"GCHANPRV1","name":"private","is_group":true,"is_private":true,"internal_team_ids":["TWORK0002"]}));
    if case() == "retirement" {
        rows.push(json!({"id":"CCHANCON1","name":"connected-only","is_channel":true,"connectedTeamIds":["TWORK0002"]}));
        rows.push(json!({"id":"CCHANGLB1","name":"org-global","is_channel":true,"is_org_shared":true,"is_global_shared":true}));
        rows.push(json!({"id":"CCHANALS1","name":"alias-empty","is_channel":true,"internalTeamIds":[],"internal_team_ids":["TWORK0002"]}));
    }
    rows
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::context::fixture::install(s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(
            team,
            matches!(case().as_str(), "selection" | "retirement")
                || (conversation::fixture::requested()
                    && matches!(case().as_str(), "filter" | "recovery")),
        );
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(true);
    block["label"]["text"] = json!("Release channel");
    block["element"] = json!({"type":"channels_select","action_id":"value","initial_channel":ALPHA,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find a channel"}});
    if multiple::requested() {
        block["optional"] = json!(case() != "selection");
        block["label"]["text"] = json!("Release channels");
        block["element"] = json!({"type":"multi_channels_select","action_id":"value","initial_channels":[ALPHA],"max_selected_items":2,"placeholder":{"type":"plain_text","text":"Find channels"}});
        if case() == "props" {
            raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_channels_select","selected_channels":[]}}}});
        }
    } else if conversation::fixture::requested() {
        conversation::fixture::field(block);
        if conversation::fixture::multiple::requested() && case() == "props" {
            raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_conversations_select","selected_conversations":[]}}}});
        }
    } else if case() == "props" {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"channels_select","selected_channel":null,"response_url_enabled":true}}}});
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    h.opened = true;
    super::super::hydrate(s);
    s.app_home.channels.fixture = Some(Harness {
        reply: if conversation::fixture::requested() {
            Reply::Conversations
        } else {
            Reply::Channels
        },
        hold: false,
        pending: None,
        pending_owner: None,
        lane: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        requests: vec![],
        releases: vec![],
    });
}
fn response(reply: Reply) -> Result<Value, Error> {
    match reply {
        Reply::Conversations => Ok(conversation::fixture::response()),
        Reply::Channels => Ok(
            json!({"ok":true,"results":[channels()[0].clone(),channels()[1].clone(),json!({"id":REMOTE,"name":"remote-release","is_channel":true,"internal_team_ids":["TWORK0002","TWORK0003"],"opaque":{"source":"remote-only"}})]}),
        ),
        Reply::Offline => Err(Error::Offline("Home channel read unavailable".into())),
        Reply::RateLimited => Err(Error::RateLimited {
            retry_after_secs: Some(1),
        }),
    }
}
fn proof(reply: Reply) -> Value {
    match reply {
        Reply::Channels | Reply::Conversations => response(reply).unwrap(),
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
    request_closed(s, t, request)
}
fn request_closed(
    s: &mut ShellState,
    t: &Ticket,
    request: &PreparedRequest,
) -> Option<tokio::sync::oneshot::Receiver<Result<Value, Error>>> {
    let current = (named()
        || multiple::named()
        || crate::app_home::inputs::users::dispatch::fixture::channel_named())
        && t.current(s);
    let (tx, rx) = tokio::sync::oneshot::channel();
    if pending_open(s) {
        let _ = tx.send(Err(Error::Offline(
            "Home fixture read already pending".into(),
        )));
        return Some(rx);
    }
    let mode = t.mode.as_ref().map(|v| v.proof(s)).unwrap_or(Value::Null);
    let Some(f) = s.app_home.channels.fixture.as_mut() else {
        let _ = tx.send(Err(Error::Offline("Home channel read unavailable".into())));
        return Some(rx);
    };
    if !current {
        let _ = tx.send(Err(Error::Offline("Home channel read unavailable".into())));
        return Some(rx);
    }
    let RequestBody::Json(body) = &request.body else {
        let _ = tx.send(Err(Error::Offline("Home channel read unavailable".into())));
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
        "inputRevision":t.input_revision,"widgetRevision":t.widget_revision,"channelPolicy":t.policy.request_fields(),"mode":mode,"socket":t.owner.rt,"reply":proof(f.reply),"held":f.hold}));
    if f.hold {
        f.pending_owner = None;
        f.pending = Some(tx);
    } else {
        let _ = tx.send(response(f.reply));
    }
    Some(rx)
}
/// Closed interceptor records the actual concurrent aggregate request inventory.
pub(super) fn request_conversations(
    s: &mut ShellState,
    t: &Ticket,
    requests: &directory::aggregate::Requests,
) -> Option<tokio::sync::oneshot::Receiver<Result<Value, Error>>> {
    if !crate::fixture::is_fixture() {
        return None;
    }
    request_conversations_closed(s, t, requests)
}
fn request_conversations_closed(
    s: &mut ShellState,
    t: &Ticket,
    requests: &directory::aggregate::Requests,
) -> Option<tokio::sync::oneshot::Receiver<Result<Value, Error>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if pending_open(s) {
        let _ = tx.send(Err(Error::Offline(
            "Home fixture read already pending".into(),
        )));
        return Some(rx);
    }
    let mut parts = Vec::new();
    let mut valid = (conversation::fixture::named()
        || conversation::fixture::multiple::named()
        || crate::app_home::inputs::users::dispatch::fixture::conversation_named())
        && t.current(s);
    for (kind, request) in [
        ("users", &requests.users),
        ("channels", &requests.channels),
        ("mpims", &requests.mpims),
    ] {
        match request {
            Ok(Some(r)) => {
                if let RequestBody::Json(mut body) = r.body.clone() {
                    body.as_object_mut().unwrap().remove("token");
                    body.as_object_mut().unwrap().remove("enterprise_token");
                    parts.push(json!({"kind":kind,"method":r.method,"endpoint":r.url.split('?').next(),"body":body}));
                } else {
                    valid = false
                }
            }
            Ok(None) => {}
            Err(_) => valid = false,
        }
    }
    let Some(f) = s.app_home.channels.fixture.as_mut().filter(|_| valid) else {
        let _ = tx.send(Err(Error::Offline(
            "Home conversation read unavailable".into(),
        )));
        return Some(rx);
    };
    f.requests.push(json!({"parts":parts,"accountTeam":t.owner.ctx.session.team_id,"enterprise":t.owner.ctx.session.enterprise_id,
  "homeContext":t.context.team_id(),"homeVisit":t.surface.visit,"homeRevision":t.surface.revision,"inputRevision":t.input_revision,"widgetRevision":t.widget_revision,"socket":t.owner.rt,"reply":proof(f.reply),"held":f.hold}));
    if f.hold {
        f.pending_owner = None;
        f.pending = Some(tx)
    } else {
        let _ = tx.send(response(f.reply));
    }
    Some(rx)
}
fn pending_open(s: &ShellState) -> bool {
    s.app_home
        .channels
        .fixture
        .as_ref()
        .and_then(|f| f.pending.as_ref())
        .is_some_and(|sender| !sender.is_closed())
}
pub(crate) fn configure(s: &mut ShellState, reply: Reply, hold: bool) {
    let f = s.app_home.channels.fixture.as_mut().unwrap();
    f.reply = reply;
    f.hold = hold;
}
pub(crate) fn pending_closed(s: &ShellState) -> bool {
    s.app_home
        .channels
        .fixture
        .as_ref()
        .is_some_and(|f| f.pending.as_ref().is_some_and(|p| p.is_closed()))
}
pub(crate) fn release(s: &mut ShellState) -> Value {
    let f = s.app_home.channels.fixture.as_mut().unwrap();
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
    let f = s.app_home.channels.fixture.as_ref();
    let form_owner = super::super::owner(s);
    let widget = form_owner
        .as_ref()
        .and_then(|o| s.app_home.channels.widgets.get(&(o.clone(), key())));
    let p = s.app_home.channels.picker.as_ref();
    let team = s.core.active_team.as_deref();
    v["named"] = json!(
        named()
            || multiple::named()
            || conversation::fixture::named()
            || conversation::fixture::multiple::named()
    );
    v["case"] = json!(case());
    v["widget"]=widget.map(|w|json!({"visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,"retainedChoice":w.retained_choice.as_ref().map(|c|json!({"id":c.id,"label":c.label,"canonical":c.canonical}))})).unwrap_or(Value::Null);
    if is_multiple() {
        let widget = form_owner
            .as_ref()
            .and_then(|o| s.app_home.channels.multiples.get(&(o.clone(), key())));
        v["widget"] = widget.map(|w|json!({"visit":w.visit,"revision":w.revision,"initial":w.initial,"visible":w.visible,"invalid":w.invalid,
          "retainedChoices":w.retained_choices.iter().map(|(id,c)|(id.clone(),json!({"id":c.id,"label":c.label,"canonical":c.canonical}))).collect::<BTreeMap<_,_>>(),
          "presentation":super::multiple::presentation(s,&key())})).unwrap_or(Value::Null);
    }
    v["requests"] = json!(f.map(|f| &f.requests));
    v["releases"] = json!(f.map(|f| &f.releases));
    v["pending"] = json!(f.is_some_and(|f| f.pending.as_ref().is_some_and(|p| !p.is_closed())));
    v["query"]=p.map(|p|json!({"current":p.ticket.current(s),"serial":p.serial,"revision":p.query_revision,"text":p.query,
        "loading":p.loading,"error":p.error,"rows":p.candidates.iter().map(|c|json!({"id":c.id,"label":c.label,"canonical":c.canonical})).collect::<Vec<_>>()})).unwrap_or(Value::Null);
    v["channels"] = json!(
        team.and_then(|t| s.core.workspaces.get(t))
            .map(|w| &w.channels)
    );
    v["context"] = crate::app_home::context::Witness::capture(s)
        .ok()
        .map(|w| w.proof(s))
        .unwrap_or(Value::Null);
    v["mode"] = s
        .app_home
        .surface
        .as_ref()
        .and_then(|h| mode::Observation::capture(s, &h.owner))
        .map(|m| m.proof(s))
        .unwrap_or(Value::Null);
    if multiple::requested() || conversation::fixture::requested() {
        v["mode"] = Value::Null;
    }
    v["channelPolicy"] = Ticket::capture(s, &key())
        .map(|t| t.policy.request_fields())
        .unwrap_or(Value::Null);
    if conversation::fixture::requested() {
        v["members"] = json!(
            team.and_then(|t| s.core.workspaces.get(t))
                .map(|w| &w.users)
        );
        v["channelPolicy"] = Value::Null;
        v["conversationFilter"]=Ticket::capture(s,&key()).and_then(|t|t.conversation).map(|f|json!({"include":f.include,"excludeBots":f.exclude_bot_users,"excludeExternal":f.exclude_external_shared_channels,"onlyFromTeam":f.only_from_team_id,"enterprise":f.enterprise_id,"defaultChannels":f.default_channels})).unwrap_or(Value::Null);
    }
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
            block["element"]["initial_channel"] = json!(id);
        } else {
            block["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_channel");
        }
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"channels_select","selected_channel":saved,"response_url_enabled":true}}}});
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
        return Err("Owned channels Home refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
