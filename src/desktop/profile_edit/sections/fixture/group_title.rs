//! Closed nested group titles use the actual shared group source and worker.
use super::*;
use crate::directory_member_fetches::Context;
use crate::usergroups::source::{self, Admission, NativeMembers, Operation, Origin};
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_GROUP_TITLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_GROUP_TITLE_CASE";
pub(crate) const GROUP: &str = "SGROUP001";
pub(crate) const ORIGINAL: &str = "Notes for @design";
pub(crate) const RENAMED: &str = "Notes for @renamed";
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() != Ok("list")
        && matches!(
            case().as_str(),
            "known" | "failure" | "cancel" | "retirement"
        )
        && [
            channel_title::FLAG,
            missing_channel::FLAG,
            user_title::FLAG,
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            completion::FLAG,
            group_recovery::FLAG,
        ]
        .iter()
        .all(|f| std::env::var(f).as_deref() != Ok("1"))
}
pub(crate) fn raw(renamed: bool) -> Value {
    json!({"id":GROUP,"team_id":"TINIT0001","handle":if renamed {"renamed"} else {"design"},
        "name":"Fixture group","date_update":if renamed {102} else {101},
        "date_delete":0,"is_visible":true,"fixture_metadata":{"keep":true}})
}
pub(crate) fn blocks() -> Value {
    json!([{"type":"rich_text","opaque":{"keep":1},"elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Notes for "},{"type":"usergroup","usergroup_id":GROUP,"style":{"bold":true},"opaque":{"keep":2}}
    ]}]}])
}
pub(crate) fn response(renamed: bool) -> Value {
    json!({"ok":true,"results":[raw(renamed)]})
}
fn seed_book() -> &'static Mutex<Value> {
    static BOOK: OnceLock<Mutex<Value>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(Value::Null))
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !mentions::enabled(&state.peek()) || !seed_book().lock().unwrap().is_null() {
        return Err("Fresh closed group title owner required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing group title owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Group title"},"self":user,"users":[user],
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Group title"},"workspaces":[{"id":owner.team}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    let context =
        Arc::new(Context::capture(&state.peek(), &owner.team).map_err(|e| e.to_string())?);
    let work = source::enqueue(
        &mut state.write(),
        context.clone(),
        Operation::Metadata {
            origin: Origin::Fetch,
            rows: vec![raw(false)],
            native_members: NativeMembers::Preserve,
        },
    );
    let Admission::Tracked(work) = work else {
        return Err("Missing shared group source admission".into());
    };
    work.run(state, |shell| context.current(shell))
        .await
        .map_err(|e| e.to_string())?;
    *seed_book().lock().unwrap() =
        json!({"boot":boot,"init":init,"rows":[raw(false)],"actualSourceSink":true});
    Ok(())
}
pub(crate) fn reconnect(shell: &mut ShellState) -> Result<Value, String> {
    group_recovery::known_freshness::reconnect(shell)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let shared = group_recovery::known_freshness::snapshot(shell);
    let group = shell
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.usergroups.get(GROUP));
    json!({"groupTitle":true,"case":case(),"seed":*seed_book().lock().unwrap(),
        "group":group,"label":group.and_then(|g|g.profile_label()),"shared":shared,
        "requests":group_recovery::requests(),"pending":group_recovery::pending(),"files":files::status(),
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),"networkCalls":0})
}
