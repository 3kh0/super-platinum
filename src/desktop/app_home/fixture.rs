//! Closed disconnected Home -> actual modal reducer -> submitted Home update.
use super::*;
use serde_json::{Value, json};
pub(crate) fn install(s: &mut ShellState) {
    s.core
        .active_account
        .get_or_insert_with(|| "00000000-0000-4000-8000-000000000091".into());
    let team = s.core.active_team.clone().unwrap();
    let user = s.core.workspaces[&team].self_user_id.clone();
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: team.clone(),
        user_id: user,
        enterprise_id: (crate::app_home::inputs::channels::fixture::requested()
            && (crate::app_home::inputs::channels::fixture::case() == "retirement"
                || (crate::app_home::inputs::channels::conversation::fixture::requested()
                    && matches!(
                        crate::app_home::inputs::channels::conversation::fixture::case().as_str(),
                        "filter" | "recovery"
                    ))))
        .then(|| "EHOME0001".into()),
        name: "Home fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic-home-token".into(),
    };
    s.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: [(team.clone(), false)].into(),
        d_cookie: "synthetic".into(),
        workspaces: [(team.clone(), session)].into(),
    });
    if crate::app_home::about::management::fixture::first_publish::confirmation::requested() {
        // Project the actual synthetic credential workspace before Home/About
        // capture, just as the closed Activity channel fixture does.
        s.workspaces = s
            .core
            .session
            .as_ref()
            .unwrap()
            .workspaces
            .values()
            .map(|w| crate::state::WorkspaceVm {
                id: w.team_id.clone(),
                name: w.name.clone(),
                initials: "W".into(),
            })
            .collect();
        s.active_workspace = 0;
    }
    s.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("synthetic").unwrap(),
    ));
    let channel = s.core.active_channel.clone().unwrap();
    let w = s.core.workspaces.get_mut(&team).unwrap();
    let bot =
        serde_json::from_value(json!({"id":"U_HOME_BOT","name":"Release reviews","is_bot":true}))
            .unwrap();
    w.users.insert("U_HOME_BOT".into(), bot);
    let c = w.channels.get_mut(&channel).unwrap();
    c.is_im = true;
    c.user = Some("U_HOME_BOT".into());
    s.channel_tab = ChannelTab::AppHome;
    s.overlay = None;
    s.app_modal = None;
    s.toast = None;
    let owner = Owner::capture(s).unwrap();
    let home = Home {
        channel,
        app: "A_HOME".into(),
        installed_team: team,
        enabled: true,
        messages: true,
        view_id: Some("V_HOME".into()),
    };
    let view = View::decode(&home, home_view(&home, false)).unwrap();
    s.app_home = Root::default();
    s.app_home.surface = Some(Surface {
        id: owner.home_visit,
        owner,
        revision: next(),
        loading: false,
        home: Some(home),
        view: Some(view),
        notice: Some("Offline app workflow — no Slack requests.".into()),
        opened: true,
        confirmation: None,
        calendar_confirmation: None,
        fixture: true,
    });
    inputs::hydrate(s);
}
pub(crate) fn install_inputs(s: &mut ShellState) {
    install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let home = h.home.clone().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let fields = serde_json::json!([
    {"type":"input","block_id":"home-name","label":{"type":"plain_text","text":"Release name"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Équipe 東京"}},
    {"type":"input","block_id":"home-notes","label":{"type":"plain_text","text":"Review notes"},"optional":true,"element":{"type":"plain_text_input","action_id":"value","multiline":true,"initial_value":"A quiet Home draft"}},
    {"type":"input","block_id":"home-day","label":{"type":"plain_text","text":"Review date"},"optional":true,"element":{"type":"datepicker","action_id":"value"}},
    {"type":"input","block_id":"home-choice","label":{"type":"plain_text","text":"Review team"},"element":{"type":"static_select","action_id":"value","options":[{"text":{"type":"plain_text","text":"Design"},"value":"design"},{"text":{"type":"plain_text","text":"Engineering"},"value":"engineering"}]}}
    ]);
    raw["blocks"]
        .as_array_mut()
        .unwrap()
        .splice(2..2, fields.as_array().unwrap().iter().cloned());
    h.view = Some(View::decode(&home, raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn home_view(h: &Home, saved: bool) -> Value {
    json!({"id":"V_HOME","type":"home","app_id":h.app,"app_installed_team_id":h.installed_team,"bot_id":"B_HOME","hash":if saved{"saved"}else{"initial"},"blocks":[
 {"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Release reviews"}},
 {"type":"section","block_id":"summary","text":{"type":"mrkdwn","text":if saved{"*Review received*\nThe app updated this Home after receiving your form."}else{"*Prepare your release*\nOpen a review form without changing your conversation draft."}}},
 {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review release"},"value":"review"}]}]})
}
/// Static Home entry preview. Buttons remain fixture-inert: no auth URL, Calendar
/// discovery, preference or event request can be dispatched from this surface.
pub(crate) fn install_calendar(s: &mut ShellState) {
    install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let home = h.home.clone().unwrap();
    let raw = json!({"id":"V_HOME","type":"home","app_id":home.app,"app_installed_team_id":home.installed_team,"hash":"calendar-preview","blocks":[
        {"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Calendar"}},
        {"type":"section","block_id":"description","text":{"type":"mrkdwn","text":"Create an event or connect your calendar while retaining your Home inputs."}},
        {"type":"input","block_id":"note","label":{"type":"plain_text","text":"Local note"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Unsent Équipe 東京 note"}},
        {"type":"actions","block_id":"calendar","elements":[
            {"type":"button","action_id":"create","value":"ADZ494LHY_CREATE_EVENT:tomorrow","text":{"type":"plain_text","text":"Create event"}},
            {"type":"button","action_id":"calendar_hermes_connect","value":json!({"app_id":"ADZ494LHY","provider_key":"gcal"}).to_string(),"text":{"type":"plain_text","text":"Connect calendar"}}]}]});
    h.view = Some(View::decode(&home, raw).unwrap());
    h.notice = Some("Offline Calendar Home preview — actions are disabled.".into());
    inputs::hydrate(s);
}
pub(super) fn activate(s: &mut ShellState, b: &Button) {
    activate_with_context(s, b, None)
}
pub(super) fn activate_with_context(s: &mut ShellState, b: &Button, context: Option<&str>) {
    let Some(h) = s
        .app_home
        .surface
        .as_ref()
        .filter(|h| h.fixture && h.owner.current(s))
    else {
        return;
    };
    let owner = h.owner.clone();
    let view = h.view.clone().unwrap();
    let Some(token) = crate::app_modal::register_action_with_context(s, "A_HOME", None, None)
    else {
        return;
    };
    let Ok(req) = b.prepare_with_context(
        &owner.ctx.client,
        &owner.ctx.session,
        &view,
        &token.token,
        context,
    ) else {
        return;
    };
    record(s, req);
    let modal = json!({"id":"V_HOME_FORM","type":"modal","app_id":"A_HOME","title":{"type":"plain_text","text":"Review release"},"submit":{"type":"plain_text","text":"Submit review"},"close":{"type":"plain_text","text":"Cancel"},"blocks":[{"type":"input","block_id":"name","label":{"type":"plain_text","text":"Release name"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Équipe 東京"}}]});
    let event=RtEvent::Unknown(serde_json::from_value(json!({"type":"view_opened","view_type":"modal","view_id":"V_HOME_FORM","app_id":"A_HOME","client_token":token.token,"view":modal})).unwrap());
    crate::app_modal::home_fixture_open(s, &owner.ctx.session.team_id, owner.rt, &event);
}
pub(crate) fn submit(s: &mut ShellState, payload: &Value, token: &str) -> bool {
    let Some(h) = s
        .app_home
        .surface
        .as_ref()
        .filter(|h| h.fixture && h.owner.current(s))
    else {
        return false;
    };
    if s.app_modal
        .as_ref()
        .and_then(|m| m.current_form())
        .is_none_or(|f| f.view_id != "V_HOME_FORM")
    {
        return false;
    }
    let owner = h.owner.clone();
    let home = h.home.clone().unwrap();
    let Ok(req) = api::block_view_submit(
        &owner.ctx.client,
        &owner.ctx.session,
        "V_HOME_FORM",
        token,
        payload,
    ) else {
        return true;
    };
    record(s, req);
    crate::app_modal::home_fixture_complete(
        s,
        token,
        api::BlockViewSubmission {
            action: api::BlockViewResponseAction::Clear,
            toast_message: Some("Review submitted".into()),
        },
    );
    let mut updated = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .raw()
        .clone();
    updated["hash"] = json!("saved");
    updated["blocks"][1] = home_view(&home, true)["blocks"][1].clone();
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","view_id":"V_HOME","app_id":"A_HOME","view":updated}),
        )
        .unwrap(),
    );
    super::realtime(s, &owner.ctx.session.team_id, owner.rt, &event);
    true
}
fn record(s: &mut ShellState, r: super_platinum_core::slack::PreparedRequest) {
    if let super_platinum_core::slack::client::RequestBody::Form(fields) = r.body {
        s.app_home.fixture_forms.push(json!({"method":r.url.rsplit('/').next(),"fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}))
    }
}
