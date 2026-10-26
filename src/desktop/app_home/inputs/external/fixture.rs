//! Closed Home option endpoint adapter. All requests use the production query path.
use super::*;
use serde_json::json;
use super_platinum_core::{
    block_actions::external::ExternalSuggestions, slack::client::RequestBody,
};
pub(crate) mod capture;
pub(crate) mod check;
#[derive(Clone, Copy)]
pub(crate) enum Reply {
    Options,
    Offline,
    Empty,
    Replacement,
    EmptyValue,
    RateLimited,
}
pub(crate) struct Harness {
    reply: Reply,
    hold: bool,
    pending: Option<tokio::sync::oneshot::Sender<Result<ExternalSuggestions, Error>>>,
    pub(crate) requests: Vec<Value>,
    release: Vec<Value>,
}
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_APP_HOME_EXTERNAL_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-external-select" | "app-home-external-select-narrow")
        )
        && matches!(
            case().as_str(),
            "selection" | "failure" | "retirement" | "rate-limit"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_EXTERNAL_CASE").unwrap_or_default()
}
pub(crate) fn key() -> InputKey {
    InputKey {
        block_id: "home-choice".into(),
        action_id: "value".into(),
    }
}
pub(crate) fn option(label: &str, value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label},"value":value,
        "description":{"type":"plain_text","text":"App-provided release"},"opaque":{"nested":["kept",{"n":7}]},"url":"https://example.invalid/not-an-option-action"})
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::fixture::install_inputs(s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team, false);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(true);
    block["label"]["text"] = json!("Release option");
    block["element"] = json!({"type":"external_select","action_id":"value","min_query_length":3,
        "initial_option":option("Installed release","installed"),"placeholder":{"type":"plain_text","text":"Find release"}});
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::hydrate(s);
    s.app_home.external.fixture = Some(Harness {
        reply: Reply::Options,
        hold: false,
        pending: None,
        requests: vec![],
        release: vec![],
    });
}
fn reply_proof(reply: Reply) -> Value {
    match reply {
        Reply::Offline => json!({"error":"offline"}),
        Reply::RateLimited => json!({"error":"rate_limited","retryAfterSeconds":8}),
        Reply::EmptyValue => json!({"ok":true,"options":[option("Empty value release","")]}),
        Reply::Empty => json!({"ok":true,"options":[]}),
        Reply::Replacement => {
            let mut changed = option("Release Gamma", "alpha,desktop");
            changed["opaque"]["nested"] = json!(["changed", {"n": 8}]);
            json!({"ok":true,"options":[changed]})
        }
        Reply::Options => {
            json!({"ok":true,"option_groups":[{"label":{"type":"plain_text","text":"Stable"},"options":[option("Release Alpha","alpha,desktop")]},{"label":{"type":"plain_text","text":"Preview"},"options":[option("Release Beta","beta")]}]})
        }
    }
}

pub(crate) fn response(reply: Reply) -> Result<ExternalSuggestions, Error> {
    match reply {
        Reply::RateLimited => Err(Error::RateLimited {
            retry_after_secs: Some(8),
        }),
        Reply::Offline => Err(Error::Offline("Closed fixture failure".into())),
        _ => api::decode_external_suggestions(&reply_proof(reply)),
    }
}
pub(crate) async fn suggestions(
    mut state: Signal<ShellState>,
    owner: &QueryOwner,
    query: &str,
) -> Option<Result<ExternalSuggestions, Error>> {
    let QueryOwner::Home(ticket) = owner else {
        return None;
    };
    if !named()
        && !crate::app_home::context::fixture::named()
        && !multi_fixture::named()
        && !super::dispatch::fixture::named()
        && !super::multi_dispatch_fixture::named()
    {
        return state
            .peek()
            .app_home
            .surface
            .as_ref()
            .is_some_and(|h| h.fixture)
            .then(|| Err(Error::Offline("Home fixture lookup is disabled".into())));
    }
    let prepared = {
        let s = state.peek();
        if !ticket.current(&s) || s.app_home.external.fixture.is_none() {
            return Some(Err(Error::Api("closed_home_owner_changed".into())));
        }
        api::block_suggestions(
            &ticket.home.ctx.client,
            &ticket.home.ctx.session,
            &ticket.route,
            query,
        )
    };
    let request = match prepared {
        Ok(r) => r,
        Err(e) => return Some(Err(e)),
    };
    let endpoint = request.url.split('?').next().unwrap_or_default().to_owned();
    let retry_safe = request.retry_safe();
    let RequestBody::Form(fields) = request.body else {
        return Some(Err(Error::Api("closed_home_not_form".into())));
    };
    let (receiver, reply) = {
        let mut s = state.write();
        let f = s.app_home.external.fixture.as_mut().unwrap();
        f.requests.push(json!({"query":query,"method":request.method,"endpoint":endpoint,"retrySafe":retry_safe,
            "fields":fields.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
            "inputRevision":ticket.inputs_revision,"homeVisit":ticket.surface.visit,"homeRevision":ticket.surface.revision,"configuredReply":reply_proof(f.reply)}));
        let receiver = if f.hold {
            let (tx, rx) = tokio::sync::oneshot::channel();
            f.pending = Some(tx);
            Some(rx)
        } else {
            None
        };
        (receiver, f.reply)
    };
    Some(match receiver {
        Some(rx) => rx
            .await
            .unwrap_or_else(|_| Err(Error::Api("closed_home_reply_retired".into()))),
        None => response(reply),
    })
}
pub(crate) fn configure(s: &mut ShellState, reply: Reply, hold: bool) {
    let f = s.app_home.external.fixture.as_mut().unwrap();
    f.reply = reply;
    f.hold = hold;
}
pub(super) fn pending_closed(s: &ShellState) -> bool {
    s.app_home
        .external
        .fixture
        .as_ref()
        .is_some_and(|f| f.pending.as_ref().is_some_and(|p| p.is_closed()))
}
pub(crate) fn release(s: &mut ShellState) -> Value {
    let f = s.app_home.external.fixture.as_mut().unwrap();
    let delivered = f
        .pending
        .take()
        .is_some_and(|p| p.send(response(f.reply)).is_ok());
    f.hold = false;
    let proof = json!({"delivered":delivered});
    f.release.push(proof.clone());
    proof
}
pub(crate) fn refresh(s: &mut ShellState) -> bool {
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    let team = h.owner.ctx.session.team_id.clone();
    let rt = h.owner.rt;
    let previous_revision = h.revision;
    let previous_state = super::super::form(s).and_then(|form| form.state().ok());
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!("compatible-refresh");
    raw["blocks"][1]["text"]["text"] =
        json!("Your selected release is retained after this Home refresh.");
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw.clone()}),
        )
        .unwrap(),
    );
    // The return value asks the caller to fetch a missing payload; accepted
    // inline updates return false. Verify the actual committed source instead.
    crate::app_home::realtime(s, &team, rt, &event);
    s.app_home.surface.as_ref().is_some_and(|h| {
        h.owner.current(s)
            && h.revision != previous_revision
            && h.view.as_ref().is_some_and(|view| view.raw() == &raw)
    }) && previous_state.is_some()
        && super::super::form(s).and_then(|form| form.state().ok()) == previous_state
}
pub(crate) struct Detached {
    home: Home,
    view: View,
}
pub(crate) fn depart(s: &mut ShellState) -> Detached {
    let source = cached_source(s);
    crate::app_home::leave(s);
    s.channel_tab = ChannelTab::Messages;
    source
}
/// Read-only source capture for fixtures that exercise real tab navigation.
pub(crate) fn cached_source(s: &ShellState) -> Detached {
    let h = s.app_home.surface.as_ref().unwrap();
    Detached {
        home: h.home.clone().unwrap(),
        view: h.view.clone().unwrap(),
    }
}
pub(crate) fn return_home(s: &mut ShellState, source: Detached) {
    s.channel_tab = ChannelTab::AppHome;
    let owner = super::super::super::Owner::capture(s).unwrap();
    s.app_home.surface = Some(Surface {
        id: owner.home_visit,
        owner,
        revision: next(),
        loading: false,
        home: Some(source.home),
        view: Some(source.view),
        notice: Some("Returned to the app's Home.".into()),
        opened: true,
        confirmation: None,
        calendar_confirmation: None,
        fixture: true,
    });
    super::super::hydrate(s);
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let f = s.app_home.external.fixture.as_ref();
    let form = super::super::form(s);
    let picker = s.app_home.external.picker.as_ref();
    let h = s.app_home.surface.as_ref();
    let authority=h.map(|h|json!({"team":h.owner.ctx.session.team_id,"user":h.owner.ctx.session.user_id,"accountEpoch":s.core.account_epoch,"socket":s.core.workspaces.get(&h.owner.ctx.session.team_id).map(|w|w.rt_generation),"capturedSocket":h.owner.rt,"homeVisit":h.id,"homeRevision":h.revision,"homeCurrent":h.owner.current(s)}));
    json!({"authority":authority,"retainedStates":s.app_home.forms.values().map(|f|json!({"rawView":f.raw_view(),"state":f.state().ok()})).collect::<Vec<_>>(),"named":named(),"case":case(),"requests":f.map(|f|&f.requests),"releases":f.map(|f|&f.release),
        "pending":f.is_some_and(|f|f.pending.as_ref().is_some_and(|p|!p.is_closed())),
        "rawView":form.map(|f|f.raw_view()),"formState":form.and_then(|f|f.state().ok()),
        "inputRevision":super::super::revision(s),"validated":validated(s),"ready":super::super::ready(s),
        "query":picker.map(|p|json!({"current":p.ticket.current(s),"revision":p.query.revision,"text":p.query.text,"loading":p.query.loading,"retryable":p.query.retryable(),"minimum":p.query.minimum,"error":p.query.error,"rows":p.query.rows.iter().map(|r|json!({"group":r.group,"option":r.option.raw()})).collect::<Vec<_>>()})),
        "actions":s.app_home.fixture_forms,"overlay":s.overlay.as_ref().map(|_|"open"),
        "accountEpoch":s.core.account_epoch,"sessionMode":s.core.session.as_ref().map(|a|&a.unified_user_client_enabled)})
}
