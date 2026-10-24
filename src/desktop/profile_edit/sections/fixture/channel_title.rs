//! Closed ordinary channel title inputs; channel publication uses the shared adapter.
use super::*;
use crate::app_home::channel_fetches::{canonical, conditional::fixture as transport};
use serde_json::json;
use std::sync::{Mutex, OnceLock};

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CHANNEL_TITLE_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_CHANNEL_TITLE_CASE";
pub(crate) const PRIVATE_FLAG: &str = "SUPER_PLATINUM_PROFILE_PRIVATE_CHANNEL_TITLE_CHECK";
pub(crate) const NONMEMBER_FLAG: &str = "SUPER_PLATINUM_PROFILE_NONMEMBER_CHANNEL_TITLE_CHECK";
pub(crate) const PLACEHOLDER: &str = "Notes for #private-channel";
pub(crate) const ORIGINAL: &str = "Notes for #project-harbor";
pub(crate) const RENAMED: &str = "Notes for #recovered-harbor";
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn private() -> bool {
    std::env::var(PRIVATE_FLAG).as_deref() == Ok("1")
}
pub(crate) fn nonmember() -> bool {
    std::env::var(NONMEMBER_FLAG).as_deref() == Ok("1")
}
pub(crate) fn initial_nonmember() -> bool {
    nonmember() && case() != "membership"
}
pub(crate) fn original_label() -> &'static str {
    if initial_nonmember() {
        PLACEHOLDER
    } else {
        ORIGINAL
    }
}
pub(crate) fn renamed_label() -> &'static str {
    if initial_nonmember() {
        PLACEHOLDER
    } else {
        RENAMED
    }
}
pub(crate) fn original_name() -> &'static str {
    if initial_nonmember() {
        "must-not-leak"
    } else {
        mentions::LABEL
    }
}
pub(crate) fn renamed_name() -> &'static str {
    if initial_nonmember() {
        "renamed-must-not-leak"
    } else {
        recovery::RENAMED
    }
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(super::missing_channel::FLAG).as_deref() != Ok("1")
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(FLAG).as_deref() == Ok("1")
        && (!nonmember() || private())
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && !files::list()
        && (matches!(
            case().as_str(),
            "known" | "failure" | "cancel" | "retirement"
        ) || (private() && case() == "membership"))
        && [
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            user_title::FLAG,
            completion::FLAG,
            "SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK",
        ]
        .iter()
        .all(|flag| std::env::var(flag).as_deref() != Ok("1"))
}
pub(crate) fn row(name: &str, updated: u64) -> Value {
    let mut row = recovery::row(name, updated);
    if private() {
        row["is_private"] = json!(true);
    }
    if initial_nonmember() {
        row["is_member"] = json!(false);
    }
    row
}
pub(crate) fn blocks() -> Value {
    json!([{"type":"rich_text","opaque":{"keep":1},"elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Notes for "},
        {"type":"channel","channel_id":mentions::CHANNEL,"style":{"bold":true},"opaque":{"keep":2}}
    ]}]}])
}
fn seed_book() -> &'static Mutex<Value> {
    static BOOK: OnceLock<Mutex<Value>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(Value::Null))
}
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !mentions::enabled(&state.peek()) {
        return Err("Closed channel title owner required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Channel title"},"self":user,"users":[user],
        "workspaces":[{"id":owner.team}],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true},row(original_name(),101)],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Channel title"},"workspaces":[{"id":owner.team}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    *seed_book().lock().unwrap() = json!({"boot":boot,"init":init,"account":owner.account,"team":owner.team,"user":owner.user});
    canonical::fixture_seed(state, boot, init).await
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let source = canonical::observed(shell, team)
        .ok()
        .and_then(|s| s.row(mentions::CHANNEL).cloned());
    let native = shell
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.channels.get(mentions::CHANNEL));
    let mut value = json!({"case":case(),"seed":*seed_book().lock().unwrap(),"files":files::status(),"channel":source,
        "nativeName":native.and_then(|c|c.name.as_ref()),"conditional":transport::status(shell),"idle":transport::idle(shell),
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,"socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),"networkCalls":0});
    if private() {
        value["privateChannelTitle"] = json!(true);
    }
    if nonmember() {
        value["nonmemberChannelTitle"] = json!(true);
    }
    value
}

/// Closed source event, through the same before/reducer/after/drain path as RT.
/// No title observation or admitted label is manufactured by this adapter.
pub(crate) fn restore_membership(mut state: Signal<ShellState>) -> Result<Value, String> {
    use crate::app_home::channel_fetches;
    if !named() || !private() || case() != "membership" || !mentions::enabled(&state.peek()) {
        return Err("Closed private membership fixture required".into());
    }
    let raw = json!({"type":"channel_joined","channel":{"id":mentions::CHANNEL},"event_ts":"2000000002.0"});
    let work = {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        let source = canonical::observed(&shell, &team).map_err(|e| e.to_string())?;
        let row = source
            .row(mentions::CHANNEL)
            .ok_or("Missing private source")?;
        if row["is_private"] != true || row["is_member"] != false || row["updated"] != 102 {
            return Err("Membership restoration lost exact revoked predecessor".into());
        }
        let generation = shell.core.workspaces[&team].rt_generation;
        let event = super_platinum_core::slack::events::RtEvent::Unknown(
            serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?,
        );
        let before = channel_fetches::before_event(&shell, &team, generation, &event)
            .ok_or("Membership source event capture refused")?;
        crate::realtime::apply_in_view(&mut shell.core, &team, generation, event, false);
        let work = channel_fetches::after_event(&mut shell, Some(before))
            .ok_or("Membership source event publication refused")?;
        shell.refresh_from_core();
        work
    };
    channel_fetches::resume(state, Some(work));
    Ok(raw)
}
