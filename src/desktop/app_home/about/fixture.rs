//! Named disconnected About surfaces; production transport is never invoked.
use super::*;
use serde_json::json;
pub(crate) fn install(s: &mut ShellState, unknown: bool) {
    super::super::fixture::install(s);
    s.channel_tab = ChannelTab::AppAbout;
    let owner = Owner::capture(s).unwrap();
    let home = s.app_home.surface.as_ref().unwrap().home.clone().unwrap();
    let id = next();
    let revision = next();
    let info=Info::decode(&home.app,json!({"ok":true,"app_id":home.app,"name":"Calendar","is_directory_approved":true,
        "long_description":"*Plan a review* with your team.\nKeep _event details_ and discussion together.\n<https://example.test/calendar|Read the guide>.\nUse `local time` for your meeting.\n> Keep everyone informed.\nReview attendees.\nChoose a calendar.\nConfirm room details.\nKeep the agenda close by.",
        "shortcuts":{"global_shortcuts":[{"name":"Create event","description":"Schedule a team event"}],"message_shortcuts":[{"name":"Create event from message","description":"Use the selected discussion"}]},
        "slash_commands":[{"name":"/calendar","description":"Open your calendar"}],
        "scope_info":{"content":{"heading":"Calendar will be able to view","secondary_groups":{"channel_content":{"heading":"Channels","scopes_info":[{"description":"Read the channels you select"}]}}},"actions":{"heading":"Calendar will be able to do","secondary_groups":{"user_actions":{"heading":"Events","scopes_info":[{"description":"Create events when you ask","is_optional":true}]}}}},
        "security_compliance":{"company_developer_name":"Synthetic provider","security_review":"Reviewed fixture"},"security_token_rotation_enabled":true,
        "supported_languages":["English","日本語"],"app_user":{"id":"U_HOME_BOT","username":"calendar"},
        "support_url":"https://example.test/support","privacy_link":"https://example.test/privacy"})).unwrap();
    let connection = Connection::decode(
        &home.app,
        json!({"ok":true,"connections":{home.app.clone():{"status":"disconnected"}}}),
    )
    .unwrap();
    s.app_home.about = State::default();
    s.app_home.about.surface = Some(Surface {
        owner: owner.clone(),
        id,
        revision,
        home: Some(home.clone()),
        profile: Some(Profile::decode(&home.app,json!({"ok":true,"app_profile":{"id":home.app,"team_id":owner.ctx.session.team_id,"is_distributed":true,"is_installed":true}})).unwrap()),
        flags: Some(Flags{connection:true,about:true,parity:true}),
        info: Some(info),
        connection: Some(connection.clone()),
        connection_enabled: Some(true),
        loading: false,
        notice: Some("Offline About fixture — no Slack requests.".into()),
        fixture: true,
    });
    if unknown {
        let token = Arc::new(
            crate::app_modal::register_action_with_context(s, &home.app, None, None).unwrap(),
        );
        s.app_home
            .about
            .correlations
            .push((token.token.clone(), owner.clone()));
        s.app_home
            .about
            .attempts
            .push(actions::Attempt::unknown_fixture(
                owner,
                home.app,
                Key { id, revision },
                connection,
                token,
            ));
    }
}
pub(super) fn connect(s: &mut ShellState, key: Key) {
    if !crate::fixture::is_fixture() && !cfg!(test) {
        return;
    }
    let Some(h) = s
        .app_home
        .about
        .surface
        .as_ref()
        .filter(|h| h.fixture && current(s, key))
    else {
        return;
    };
    let owner = h.owner.clone();
    let connection = h.connection.clone().unwrap();
    let home = h.home.clone().unwrap();
    let Some(token) = crate::app_modal::register_action_with_context(s, &home.app, None, None)
    else {
        return;
    };
    let request = connection
        .submit(&owner.ctx.client, &owner.ctx.session, &token.token, true)
        .unwrap();
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    s.app_home
        .fixture_forms
        .push(json!({"url":request.url,"fields":fields}));
    s.app_home
        .about
        .correlations
        .push((token.token.clone(), owner.clone()));
    let event=RtEvent::Unknown(serde_json::from_value(json!({"type":"view_opened","view_type":"modal","view_id":"V_HOME_FORM","app_id":home.app,"client_token":token.token,"view":{"id":"V_HOME_FORM","type":"modal","app_id":home.app,"title":{"type":"plain_text","text":"Connect Calendar"},"close":{"type":"plain_text","text":"Cancel"},"blocks":[{"type":"section","block_id":"connection","text":{"type":"plain_text","text":"This is the app's owned connection form. No request was sent to Slack."}}]}})).unwrap());
    crate::app_modal::home_fixture_open(s, &owner.ctx.session.team_id, owner.rt, &event);
}

pub(crate) fn install_modal(s: &mut ShellState) {
    install(s, false);
    let h = s.app_home.about.surface.as_ref().unwrap();
    let key = Key {
        id: h.id,
        revision: h.revision,
    };
    connect(s, key);
}

// Only the channel-navigation case owns closed responses for a fresh About visit.
// Other named fixtures never fall back to an authenticated transport.
pub(super) fn read_enabled(s: &ShellState) -> bool {
    management::activity::fixture::channel::enabled(s)
        || crate::channel_workflows::fixture::enabled(s)
        || management::fixture::first_publish::confirmation::share::drafts::enabled(s)
}
pub(super) fn read_response(
    s: &ShellState,
    request: &super_platinum_core::slack::PreparedRequest,
) -> Result<serde_json::Value, Error> {
    if management::fixture::first_publish::confirmation::share::drafts::enabled(s) {
        return management::fixture::first_publish::confirmation::share::drafts::about_response(
            s, request,
        );
    }
    if crate::channel_workflows::fixture::enabled(s) {
        return crate::channel_workflows::fixture::about_response(s, request);
    }
    management::activity::fixture::channel::about_response(s, request)
}
