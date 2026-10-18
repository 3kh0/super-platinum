//! Closed disconnected Home -> actual modal reducer -> submitted Home update.
use super::*;
use serde_json::{Value, json};
pub(crate) fn install(s: &mut ShellState) {
    let team = s.core.active_team.clone().unwrap();
    let user = s.core.workspaces[&team].self_user_id.clone();
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: team.clone(),
        user_id: user,
        enterprise_id: None,
        name: "Home fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic-home-token".into(),
    };
    s.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [(team.clone(), session)].into(),
    });
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
        fixture: true,
    });
}
fn home_view(h: &Home, saved: bool) -> Value {
    json!({"id":"V_HOME","type":"home","app_id":h.app,"app_installed_team_id":h.installed_team,"bot_id":"B_HOME","hash":if saved{"saved"}else{"initial"},"blocks":[
 {"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Release reviews"}},
 {"type":"section","block_id":"summary","text":{"type":"mrkdwn","text":if saved{"*Review received*\nThe app updated this Home after receiving your form."}else{"*Prepare your release*\nOpen a review form without changing your conversation draft."}}},
 {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review release"},"value":"review"}]}]})
}
pub(super) fn activate(s: &mut ShellState, b: &Button) {
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
    let Ok(req) = b.prepare(&owner.ctx.client, &owner.ctx.session, &view, &token.token) else {
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
    let event=RtEvent::Unknown(serde_json::from_value(json!({"type":"view_updated","view_id":"V_HOME","app_id":"A_HOME","view":home_view(&home,true)})).unwrap());
    super::realtime(s, &owner.ctx.session.team_id, owner.rt, &event);
    true
}
fn record(s: &mut ShellState, r: super_platinum_core::slack::PreparedRequest) {
    if let super_platinum_core::slack::client::RequestBody::Form(fields) = r.body {
        s.app_home.fixture_forms.push(json!({"method":r.url.rsplit('/').next(),"fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}))
    }
}
