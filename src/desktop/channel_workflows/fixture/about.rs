//! Fresh About producers after real sidebar/tab navigation; no owner installer.
use super::*;
use super_platinum_core::{
    block_actions::{app_eligibility, home, home::about::Info},
    slack::api,
};
pub(crate) fn about_response(s: &ShellState, request: &PreparedRequest) -> Result<Value, Error> {
    let model = capture(s)?;
    if s.core.active_channel.as_deref() != Some("CAPPHOME1")
        || s.channel_tab != ChannelTab::AppAbout
    {
        return Err(refused());
    }
    let ctx = Context::capture(s).ok_or_else(refused)?;
    let (method, fields) = parts(request)?;
    let (expected, raw) = match method {
        "conversations.info" => (
            home::fetch(&ctx.client, &ctx.session, "CAPPHOME1")?,
            json!({"ok":true,"channel":s.core.workspaces["TWORKFLOW"].channels["CAPPHOME1"],"app_home":{"app_id":"A_HOME","app_installed_team_id":"TWORKFLOW","home_tab_enabled":true,"messages_tab_enabled":true,"home_view_id":"V_HOME"}}),
        ),
        "apps.about.info" => (
            Info::request(&ctx.client, &ctx.session, "A_HOME")?,
            json!({"ok":true,"app_id":"A_HOME","name":"Review requests","owner_ids":["UWORKFLOW"]}),
        ),
        "apps.profile.get" => (
            app_eligibility::profile_request(&ctx.client, &ctx.session, "A_HOME")?,
            json!({"ok":true,"app_profile":{"id":"A_HOME","team_id":"TWORKFLOW","is_installed":true,"is_workflow_app":true}}),
        ),
        "experiments.getByUser" => (
            api::list_experiments(&ctx.client, &ctx.session),
            json!({"ok":true,"assignments":{"agent_about_page":{"group":"on"},"flexpane_content_parity":{"group":"on"}}}),
        ),
        _ => return Err(refused()),
    };
    endpoint::tab_request(request, &expected)?;
    model.0.lock().unwrap().about.push(json!({"method":method,"fields":fields.iter().filter(|(k,_)|!matches!(k.as_str(),"token"|"enterprise_token")).collect::<Vec<_>>(),"raw":raw,"channel":"CAPPHOME1","channelGeneration":s.channel_generation}));
    Ok(raw)
}
