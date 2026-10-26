//! Closed missing nested groups use the production shared selector and FIFO.
use super::*;
use crate::{
    directory_member_fetches::Context,
    usergroups::{
        source::{self, Admission, NativeMembers, Operation, Origin},
        unknown,
    },
};
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MISSING_GROUP_TITLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_MISSING_GROUP_TITLE_CASE";
pub(crate) const GROUP: &str = "SGROUP001";
pub(crate) const ORIGINAL: &str = "Notes for @unknown-group";
pub(crate) const FOUND: &str = "Notes for @design";
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && [FLAG, mentions::FLAG, files::FLAG, files::BLOCKS_FLAG]
            .iter()
            .all(|f| std::env::var(f).as_deref() == Ok("1"))
        // Query only the raw mode flag here: files::list also consults the
        // group recovery gate, which delegates back to this named fixture.
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() != Ok("list")
        && matches!(
            case().as_str(),
            "positive" | "omission" | "failure" | "cancel" | "retirement" | "restricted"
        )
        && [
            group_title::FLAG,
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
pub(crate) fn final_label() -> &'static str {
    if matches!(case().as_str(), "positive" | "cancel") {
        FOUND
    } else {
        ORIGINAL
    }
}
pub(crate) fn raw() -> Value {
    group_title::raw(false)
}
pub(crate) fn blocks() -> Value {
    group_title::blocks()
}
pub(crate) fn response() -> Value {
    json!({"ok":true,"results":[raw()]})
}
fn book() -> &'static Mutex<Value> {
    static BOOK: OnceLock<Mutex<Value>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(Value::Null))
}
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !mentions::enabled(&state.peek()) || !book().lock().unwrap().is_null() {
        return Err("Fresh missing-group fixture required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"is_restricted":case()=="restricted","profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Missing group title"},"self":user,"users":[user],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Missing group title"},"workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    *book().lock().unwrap() = json!({"boot":boot,"init":init,"rows":[],"actualSourceSink":true});
    Ok(())
}
pub(crate) async fn publish_known(mut state: Signal<ShellState>) -> Result<(), String> {
    if !named() || case() != "restricted" {
        return Err("Restricted fixture only".into());
    }
    let team = state
        .peek()
        .core
        .active_team
        .clone()
        .ok_or("Missing team")?;
    let context = Arc::new(Context::capture(&state.peek(), &team).map_err(|e| e.to_string())?);
    let Admission::Tracked(work) = source::enqueue(
        &mut state.write(),
        context.clone(),
        Operation::Metadata {
            origin: Origin::Fetch,
            rows: vec![raw()],
            native_members: NativeMembers::Preserve,
        },
    ) else {
        return Err("Missing source admission".into());
    };
    work.run(state, |s| context.current(s))
        .await
        .map_err(|e| e.to_string())
}
pub(crate) fn reconnect(s: &mut ShellState) -> Result<Value, String> {
    group_recovery::known_freshness::reconnect(s)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let team = s.core.active_team.as_deref().unwrap_or("");
    let owner = Context::capture(s, team).ok();
    let w = s.core.workspaces.get(team);
    let source = owner.as_ref().and_then(|o| {
        w.and_then(|w| {
            w.directory_usergroups
                .snapshot(&o.account, &o.session, o.epoch, &w.usergroups)
                .ok()
        })
    });
    let selector=unknown::observed(s,team,GROUP).map(|v|json!({"kind":format!("{:?}",v.lookup().kind()),"transient":v.lookup().is_transient(),"current":v.current(s),"effective":v.lookup().effective(),"provenance":"shared-group-selector"}));
    json!({"missingGroupTitle":true,"case":case(),"seed":*book().lock().unwrap(),"source":source.as_ref().map(|v|json!({"rows":v.rows(),"membership":v.membership()})),
        "group":w.and_then(|w|w.usergroups.get(GROUP)),"selfRestricted":owner.as_ref().and_then(|o|w.and_then(|w|w.users.get(&o.session.user_id))).map(|u|u.is_restricted),
        "selector":selector,"unknown":unknown::status(s),"shared":group_recovery::known_freshness::snapshot(s),
        "requests":group_recovery::requests(),"pending":group_recovery::pending(),"files":files::status(),"account":s.core.active_account,"epoch":s.core.account_epoch,
        "socketGeneration":w.map(|w|w.rt_generation),"networkCalls":0})
}
