//! Known shared source inputs for a mixed preview; no title-local directory.
use super::*;
use crate::directory_member_fetches::Context;
use crate::usergroups::source::{self, Admission, NativeMembers, Operation, Origin};
use std::sync::Arc;
pub(super) const USER: &str = "USECTION01";
pub(super) const CHANNEL: &str = "CSECTION01";
pub(super) const GROUP: &str = "SSECTION01";
fn book() -> &'static Mutex<Value> {
    static BOOK: OnceLock<Mutex<Value>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(json!({"seed":null,"policy":[]})))
}
pub(crate) fn mixed() -> Value {
    json!({"type":"rich_text","opaque":{"mixed":true},"elements":[{"type":"rich_text_section","elements":[
        {"type":"user","user_id":USER,"style":{"bold":true}}, {"type":"text","text":" "},
        {"type":"channel","channel_id":CHANNEL}, {"type":"text","text":" "},
        {"type":"usergroup","usergroup_id":GROUP}, {"type":"text","text":" "},
        {"type":"emoji","name":"wave"}, {"type":"text","text":" "},
        {"type":"canvas","file_id":files::ID,"section_id":"nested-section-is-stripped","style":{"italic":true},"opaque":{"retain":true}}
    ]}]})
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !named() || case() != "success" || !mentions::enabled(&state.peek()) {
        return Err("Closed mixed-preview owner required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing owner")?;
    let self_user =
        json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let user = json!({"id":USER,"name":"section-user","team_id":owner.team,"profile":{"display_name":"U","real_name":"Preview User"}});
    let channel = json!({"id":CHANNEL,"name":"c","is_channel":true,"is_member":true,"updated":101});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Section preview"},"self":self_user,"users":[self_user,user],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true},channel],"workspaces":[{"id":owner.team}],"prefs":{}});
    let init = json!({"ok":true,"self":self_user,"team":{"id":owner.team,"name":"Section preview"},"workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    let context =
        Arc::new(Context::capture(&state.peek(), &owner.team).map_err(|e| e.to_string())?);
    let row = json!({"id":GROUP,"team_id":owner.team,"handle":"g","name":"Section group","date_update":101,"date_delete":0,"is_visible":true});
    let Admission::Tracked(work) = source::enqueue(
        &mut state.write(),
        context.clone(),
        Operation::Metadata {
            origin: Origin::Fetch,
            rows: vec![row.clone()],
            native_members: NativeMembers::Preserve,
        },
    ) else {
        return Err("Missing mixed group source".into());
    };
    work.run(state, |shell| context.current(shell))
        .await
        .map_err(|e| e.to_string())?;
    book().lock().unwrap()["seed"] =
        json!({"boot":boot,"init":init,"group":row,"actualSourceSink":true});
    Ok(())
}
pub(crate) async fn policy_read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    if !named() || case() != "success" || !mentions::enabled(&state.peek()) {
        return Err(Error::Api("closed_section_policy".into()));
    }
    let shell = state.peek();
    let owner = Context::capture(
        &shell,
        shell
            .core
            .active_team
            .as_deref()
            .ok_or_else(|| Error::Api("closed_section_policy".into()))?,
    )?;
    let expected = super_platinum_core::block_actions::name_policy::request::user(
        &owner.client,
        &owner.session,
    );
    if !files::validate(request, &expected) {
        return Err(Error::Api("closed_section_policy_factory".into()));
    }
    let raw = json!({"ok":true,"prefs":{"display_real_names_override":1,"hidden_users":""},"opaque_policy":"complete-section-policy"});
    let mut book = book().lock().unwrap();
    let rows = book["policy"].as_array_mut().unwrap();
    let serial = rows.len() + 1;
    rows.push(json!({"serial":serial,"method":"users.prefs.get","body":{},"account":owner.account,"team":owner.session.team_id,"user":owner.session.user_id,"epoch":owner.epoch,"raw":raw}));
    Ok(raw)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    if case() != "success" {
        return Value::Null;
    }
    let team = shell.core.active_team.as_deref().unwrap_or_default();
    let workspace = shell.core.workspaces.get(team);
    json!({"input":*book().lock().unwrap(),"nativeUser":workspace.and_then(|w|w.users.get(USER)),"nativeChannel":workspace.and_then(|w|w.channels.get(CHANNEL)),"nativeGroup":workspace.and_then(|w|w.usergroups.get(GROUP)),"groupFreshness":crate::usergroups::freshness::snapshot(shell,team),"conditionalFixtureTransportExercised":false})
}
