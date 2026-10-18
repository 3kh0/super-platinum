//! Closed synthetic request executor for the mounted production workflow path.
//! No fallback to HTTP, session discovery, arbitrary callbacks or live URLs.
use super::*;
use serde_json::json;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
mod authentication;
pub(crate) mod check;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Accepted,
    Held,
    Unconfirmed,
    RateLimited,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingMode {
    Known,
    AllDay,
    TitleOnly,
    StatusUnavailable,
    Disconnected,
}
pub(crate) struct Harness {
    origin: Origin,
    mode: Mode,
    meeting: Option<MeetingMode>,
    authentication: Option<authentication::State>,
    ledger: Vec<Value>,
    held: Option<tokio::sync::oneshot::Sender<Result<Value, Error>>>,
    started: bool,
}

pub(crate) fn install(s: &mut ShellState, mode: Mode) {
    super::super::fixture::install(s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team.clone(), true);
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .url = "https://fixture.slack.com".into();
    s.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::disconnected_fixture("synthetic"),
    ));
    let owner = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    h.revision = next();
    let home = h.home.clone().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Release workflow"}},
        {"type":"section","block_id":"intro","text":{"type":"plain_text","text":"Choose people and conversations, review the inputs, then start the workflow."}},
        {"type":"actions","block_id":"workflow","elements":[{"type":"workflow_button","action_id":"run","text":{"type":"plain_text","text":"Run release workflow"},"workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}}]}]);
    h.view = Some(View::decode(&home, raw).unwrap());
    h.notice = Some("Disconnected workflow fixture. No Slack requests.".into());
    inputs::hydrate(s);
    let h = s.app_home.surface.as_ref().unwrap();
    let origin = Origin {
        owner: h.owner.clone(),
        key: Key {
            visit: h.id,
            revision: h.revision,
        },
        workflow: Workflow::capture(
            h.view.as_ref().unwrap(),
            "workflow",
            "run",
            &h.owner.ctx.session.user_id,
        )
        .unwrap(),
        input_revision: inputs::revision(s),
    };
    let workspace = s.core.workspaces.get_mut(&team).unwrap();
    for raw in users() {
        let user: super_platinum_core::slack::models::User = serde_json::from_value(raw).unwrap();
        workspace.users.insert(user.id.clone(), user);
    }
    for raw in [
        json!({"id":"DWORKFLOW","is_im":true,"user":"UWORKADA1","is_open":false}),
        json!({"id":"GWORKFLOW","is_mpim":true,"members":["UWORKADA1","UWORKGRACE"],"name":"mpdm-ada-grace"}),
    ] {
        let channel: super_platinum_core::slack::models::Channel =
            serde_json::from_value(raw).unwrap();
        workspace.channels.insert(channel.id.clone(), channel);
    }
    s.app_home.workflow_fixture = Some(Harness {
        origin,
        mode,
        meeting: None,
        authentication: None,
        ledger: Vec::new(),
        held: None,
        started: false,
    });
}
pub(crate) fn install_authentication(s: &mut ShellState, mode: Mode) {
    install(s, Mode::Accepted);
    s.app_home.workflow_fixture.as_mut().unwrap().authentication =
        Some(authentication::State::new(mode));
}
pub(crate) fn install_meeting(s: &mut ShellState, mode: MeetingMode) {
    install(s, Mode::Accepted);
    s.app_home.workflow_fixture.as_mut().unwrap().meeting = Some(mode);
    let w = s
        .core
        .workspaces
        .get_mut(s.core.active_team.as_ref().unwrap())
        .unwrap();
    let raw = json!({"id":"CMEETING1","is_channel":true,"is_member":true,"name":"mc:x:release-review-2026-10-17-0930",
        "properties":{"meetings_channel":{"is_meeting_channel":true,"event_link_id":"LWORKFLOW"}}});
    let channel: super_platinum_core::slack::models::Channel = serde_json::from_value(raw).unwrap();
    w.channels.insert(channel.id.clone(), channel);
}
fn users() -> Vec<Value> {
    vec![
        json!({"id":"UWORKADA1","name":"ada","profile":{"display_name":"Ada","real_name":"Ada Lovelace"}}),
        json!({"id":"UWORKGRACE","name":"grace","profile":{"display_name":"Grace","real_name":"Grace Hopper"}}),
    ]
}
fn review() -> Value {
    json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":{
        "third_party_auths":{"is_shown":false,"is_satisfied":true,"auths":[]},
        "missing_context_parameters":{"is_shown":false,"context_parameters":[]},
        "missing_customizable_input_parameters":{"is_shown":true,"customizable_input_parameters":[
            {"name":"title","type":"string","title":"Release title"},
            {"name":"person","type":"slack#/types/user_id","title":"Reviewer"},
            {"name":"destination","type":"slack#/types/channel_id","title":"Conversation"}]},
        "scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}}})
}

enum Reply {
    Ready(Value),
    Held(tokio::sync::oneshot::Receiver<Result<Value, Error>>),
}
fn ready(value: Value) -> Result<Reply, Error> {
    Ok(Reply::Ready(value))
}

pub(super) async fn execute(
    mut state: Signal<ShellState>,
    origin: &Origin,
    request: PreparedRequest,
    once: bool,
) -> Result<Value, Error> {
    if !crate::fixture::is_fixture() {
        return Err(refused());
    }
    let reply = begin(&mut state.write(), origin, request, once)?;
    match reply {
        Reply::Ready(raw) => Ok(raw),
        Reply::Held(pending) => pending
            .await
            .map_err(|_| Error::Api("synthetic_held_result_dropped".into()))?,
    }
}
// Private synchronous half of the same executor, also used by focused tests.
// It never creates an HTTP client or bypasses owner/request admission.
fn begin(
    s: &mut ShellState,
    origin: &Origin,
    request: PreparedRequest,
    once: bool,
) -> Result<Reply, Error> {
    if !origin.current(s) {
        return Err(refused());
    }
    let fixture = s.app_home.workflow_fixture.as_mut().ok_or_else(refused)?;
    if !fixture.origin.workflow.same_action(&origin.workflow)
        || fixture.origin.key != origin.key
        || !Arc::ptr_eq(
            &fixture.origin.owner.ctx.transport,
            &origin.owner.ctx.transport,
        )
    {
        return Err(refused());
    }
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    if request.method != "POST"
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.host_str(),
            Some("fixture.slack.com" | "edgeapi.slack.com")
        )
    {
        return Err(refused());
    }
    let expected_url = if url.host_str() == Some("fixture.slack.com") {
        let endpoint = url.path().strip_prefix("/api/").ok_or_else(refused)?;
        origin
            .owner
            .ctx
            .client
            .rest_form(&origin.owner.ctx.session, endpoint, vec![])
            .url
    } else {
        let prefix = format!("/cache/{}/", origin.owner.ctx.session.team_id);
        let path = url.path().strip_prefix(&prefix).ok_or_else(refused)?;
        if !matches!(path, "users/search" | "channels/search") {
            return Err(refused());
        }
        origin
            .owner
            .ctx
            .client
            .edge_json(&origin.owner.ctx.session, path, json!({}))
            .map_err(|_| refused())?
            .url
    };
    // The actual client owns xparams; permit precisely its generated URL,
    // rather than rejecting legitimate parameters or accepting arbitrary ones.
    if request.url != expected_url {
        return Err(refused());
    }
    let body = match &request.body {
        RequestBody::Form(values) => {
            let mut map = serde_json::Map::new();
            for (key, value) in values {
                if map.insert(key.clone(), json!(value)).is_some() {
                    return Err(refused());
                }
            }
            Value::Object(map)
        }
        RequestBody::Json(raw) => raw.clone(),
    };
    if body["token"] != "synthetic-home-token" {
        return Err(refused());
    }
    let mut recorded = body.clone();
    recorded.as_object_mut().unwrap().remove("token");
    recorded.as_object_mut().unwrap().remove("enterprise_token");
    fixture
        .ledger
        .push(json!({"path":url.path(),"once":once,"body":recorded}));
    if fixture.authentication.is_some()
        && matches!(
            url.path(),
            "/api/apps.auth.external.authMapping.update" | "/api/workflows.triggers.trip"
        )
    {
        return authentication::begin(fixture, url.path(), &body, once);
    }
    match (url.path(), once) {
        ("/api/client.gantryBoot", false)
            if body["include_channels"] == "false" && body["_x_reason"] == "fetch-client-data" =>
        {
            return ready(json!({"ok":true,
                "team":{"id":origin.owner.ctx.session.team_id},
                "self":{"id":origin.owner.ctx.session.user_id,"team_id":origin.owner.ctx.session.team_id,"name":"reviewer","profile":{"real_name":"Fixture reviewer","display_name":"Reviewer"}},
                "workspaces":[{"id":origin.owner.ctx.session.team_id,"allow_external_users":false,"prefs":{"display_real_names":false}}],
                "default_workspace":origin.owner.ctx.session.team_id}));
        }
        ("/api/client.userBoot", false) => {
            let w = s
                .core
                .workspaces
                .get(&origin.owner.ctx.session.team_id)
                .ok_or_else(refused)?;
            return ready(
                json!({"ok":true,"team":{"id":origin.owner.ctx.session.team_id,"plan":"std"},
                "self":{"id":origin.owner.ctx.session.user_id,"team_id":origin.owner.ctx.session.team_id},
                "workspaces":[{"id":origin.owner.ctx.session.team_id,"allow_external_users":false,"plan":"std"}],
                "prefs":{},"read_only_channels":[],"thread_only_channels":[],
                "channels":w.channels.values().collect::<Vec<_>>(),"users":w.users.values().collect::<Vec<_>>() }),
            );
        }
        ("/api/users.prefs.get", false) => {
            return ready(
                json!({"ok":true,"prefs":{"display_real_names_override":-1,"hidden_users":"","frecency":"{}"}}),
            );
        }
        ("/api/team.prefs.get", false)
            if body.get("target_team").is_none()
                && origin.owner.ctx.session.team_id.starts_with('T') =>
        {
            return ready(json!({"ok":true,"prefs":{"display_real_names":false}}));
        }

        ("/api/workflows.triggers.info", false) if body["trigger_id"] == "Ft12345678" => {
            return ready(
                json!({"ok":true,"trigger":{"id":"Ft12345678","name":"Release workflow","owning_team_id":origin.owner.ctx.session.team_id,"workflow":{"workflow_id":"WWORKFLOW","app_id":"A_WORKFLOW"}}}),
            );
        }
        ("/api/api.getFlannelHttpUrl", false) => {
            return ready(
                json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{}",origin.owner.ctx.session.team_id)}),
            );
        }
        ("/api/experiments.getByUser", false) => {
            return ready(json!({"ok":true,"assignments":{}}));
        }
        ("/api/search.autocomplete.model", false)
            if body["_x_reason"] == "AutocompleteModelCache" =>
        {
            // An actual closed response with an absent model selects the
            // retained source fallback; this is not an unobserved model.
            return ready(json!({"ok":true,"model":null}));
        }
        ("/api/search.autocomplete.offlineFeatures", false)
            if body["_x_reason"] == "search-offline-features" =>
        {
            return ready(json!({"ok":true}));
        }
        ("/api/calendar.user.status", false) if fixture.meeting.is_some() => {
            if fixture.meeting == Some(MeetingMode::StatusUnavailable) {
                return Err(Error::Api("synthetic_calendar_status_unavailable".into()));
            }
            return ready(
                json!({"ok":true,"status":[{"cal_app_type":"google","user_status":if fixture.meeting==Some(MeetingMode::Disconnected) {"disconnected"} else {"connected"}}]}),
            );
        }
        ("/api/calendar.event.getByEventLinkId", false)
            if fixture.meeting.is_some() && body["event_link_id"] == "LWORKFLOW" =>
        {
            if matches!(
                fixture.meeting,
                Some(MeetingMode::StatusUnavailable | MeetingMode::Disconnected)
            ) {
                return Err(refused());
            }
            let mut event = json!({"id":"EWORKFLOW","event_link_id":"LWORKFLOW","title":"Release &amp; Review","is_all_day":fixture.meeting==Some(MeetingMode::AllDay)});
            if fixture.meeting != Some(MeetingMode::TitleOnly) {
                event["start"] = json!(1792243800);
            }
            return ready(json!({"ok":true,"event":event}));
        }
        ("/api/conversations.info", false) => {
            let id = body["channel"].as_str().ok_or_else(refused)?;
            let channel = s
                .core
                .workspaces
                .get(&origin.owner.ctx.session.team_id)
                .and_then(|w| w.channels.get(id))
                .ok_or_else(refused)?;
            return ready(json!({"ok":true,"channel":channel}));
        }
        (path, false) if path.ends_with("/users/search") => {
            return ready(json!({"results":users()}));
        }
        (path, false) if path.ends_with("/channels/search") => {
            return ready(json!({"results":[]}));
        }
        ("/api/workflows.triggers.trip", true) => {
            if body["url"] != "https://slack.com/shortcuts/Ft12345678"
                || body["client_token"].as_str().is_none_or(str::is_empty)
            {
                return Err(refused());
            }
            if body["run_precheck"] == "true" {
                return ready(review());
            }
            if fixture.started || body.get("run_precheck").is_some() {
                return Err(refused());
            }
            let context: Value =
                serde_json::from_str(body["context"].as_str().ok_or_else(refused)?)
                    .map_err(|_| refused())?;
            if !valid_start(&context) {
                return Err(refused());
            }
            fixture.started = true;
            match fixture.mode {
                Mode::Accepted => return ready(json!({"ok":true})),
                Mode::Unconfirmed => {
                    return Err(Error::Api("synthetic_response_lost_after_dispatch".into()));
                }
                Mode::RateLimited => {
                    fixture.started = false;
                    fixture.mode = Mode::Accepted;
                    return Err(Error::RateLimited {
                        retry_after_secs: Some(2),
                    });
                }
                Mode::Held => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    fixture.held = Some(tx);
                    Ok(Reply::Held(rx))
                }
            }
        }
        _ => return Err(refused()),
    }
}
pub(crate) fn release(s: &mut ShellState) -> bool {
    s.app_home
        .workflow_fixture
        .as_mut()
        .and_then(|f| f.held.take())
        .is_some_and(|tx| tx.send(Ok(json!({"ok":true}))).is_ok())
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    s.app_home.workflow_fixture.as_ref().map(|f|json!({
        "ledger":f.ledger,"started":f.started,"held":f.held.is_some(),
        "authentication":f.authentication.as_ref().map(authentication::State::snapshot),
        "current":f.origin.current(s),"cooling":cooling(s,&f.origin.owner.ctx),
        "attempts":s.app_home.workflow_attempts.iter().filter(|a|a.origin.workflow.same_action(&f.origin.workflow)).map(|a|json!({
            "id":a.id.to_string(),"cancelled":a.cancelled,"phase":match a.phase {Phase::Queued=>"queued",Phase::Sent=>"sent",Phase::Uncertain=>"uncertain"}
        })).collect::<Vec<_>>(),
        "dialog":s.app_home.workflow.as_ref().filter(|d|d.origin.workflow.same_action(&f.origin.workflow)).map(|d|json!({
            "id":d.id.to_string(),"dismissed":d.dismissed,"busy":d.busy,"notice":d.notice,
            "ready":d.review.ready(),"entitiesReady":entity_picker::ready(s,d.id,&d.review),
            "title":d.review.value("title",true),"person":d.review.value("person",true),
            "destination":d.review.value("destination",true),
            "connections":d.review.connections().iter().map(|a|json!({"function":a.function,"provider":a.provider,"tokens":a.tokens.iter().map(|t|json!({"id":t.id,"default":t.default})).collect::<Vec<_>>()})).collect::<Vec<_>>()
        }))
    })).unwrap_or(Value::Null)
}
fn refused() -> Error {
    Error::Api("block_interaction_not_sent_offline".into())
}

fn valid_start(context: &Value) -> bool {
    let Some(values) = context["customizable_input_parameters"].as_array() else {
        return false;
    };
    context["location"] == "speedbump"
        && context["speedbump_input_parameters"] == json!({})
        && values.len() == 3
        && values[0]["name"] == "title"
        && values[0]["value"]
            .as_str()
            .is_some_and(|v| !v.trim().is_empty())
        && values[1]["name"] == "person"
        && matches!(
            values[1]["value"].as_str(),
            Some("UWORKADA1" | "UWORKGRACE")
        )
        && values[2]["name"] == "destination"
        && matches!(values[2]["value"].as_str(), Some("DWORKFLOW" | "GWORKFLOW"))
}

#[cfg(test)]
mod tests;

/// Fresh private fixture history. Never reads the real account's browser store.
pub(super) fn navigation_owner(s: &ShellState) -> Option<String> {
    let h = s.app_home.workflow_fixture.as_ref()?;
    h.origin
        .current(s)
        .then(|| format!("fixture-navigation-{}", h.origin.key.visit))
}
