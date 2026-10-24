//! Exact missing-source seed and shared acquisition diagnostics, offline only.
use super::*;
use crate::app_home::channel_fetches::{canonical, unknown};
use serde_json::json;
use std::sync::{Mutex, OnceLock};
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MISSING_CHANNEL_TITLE_CHECK";
pub(crate) const ORIGINAL: &str = "Notes for #unknown-channel";
pub(crate) const FOUND: &str = "Notes for #fetched-harbor";
pub(crate) const TERMINAL: &str = "Notes for #private-channel";
pub(crate) fn case() -> String {
    channel_title::case()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(mentions::FLAG).as_deref() == Ok("1")
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var(channel_title::FLAG).as_deref() == Ok("1")
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && !files::list()
        && matches!(
            case().as_str(),
            "positive" | "omission" | "failure" | "cancel" | "retirement"
        )
        && [
            channel_title::PRIVATE_FLAG,
            channel_title::NONMEMBER_FLAG,
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            user_title::FLAG,
            completion::FLAG,
            "SUPER_PLATINUM_PROFILE_FILE_COMPLETION_CHECK",
        ]
        .iter()
        .all(|f| std::env::var(f).as_deref() != Ok("1"))
}
pub(crate) fn final_label() -> &'static str {
    if matches!(case().as_str(), "omission" | "failure") {
        TERMINAL
    } else {
        FOUND
    }
}
fn book() -> &'static Mutex<Value> {
    static BOOK: OnceLock<Mutex<Value>> = OnceLock::new();
    BOOK.get_or_init(|| Mutex::new(Value::Null))
}
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !mentions::enabled(&state.peek()) {
        return Err("Closed missing-channel owner required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Channel title"},"self":user,"users":[user],
        "workspaces":[{"id":owner.team}],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Channel title"},"workspaces":[{"id":owner.team}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    *book().lock().unwrap() = json!({"boot":boot,"init":init,"account":owner.account,"team":owner.team,"user":owner.user});
    canonical::fixture_seed(state, boot, init).await
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let source = canonical::observed(shell, team)
        .ok()
        .and_then(|s| s.row(mentions::CHANNEL).cloned());
    let lookup=unknown::observed(shell,team,mentions::CHANNEL).map(|s|json!({"kind":format!("{:?}",s.lookup().kind()),"transient":s.lookup().is_transient(),"current":s.current(shell),"provenance":"sealed-shared-channel-selector"}));
    json!({"case":case(),"seed":*book().lock().unwrap(),"files":files::status(),"channel":source,"selector":lookup,
        "unknown":unknown::fixture::status(shell),"conditional":crate::app_home::channel_fetches::conditional::fixture::status(shell),
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,"socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),"networkCalls":0})
}

/// Read-only fixture data snapshot, never acquisition or current-row authority.
/// cached() includes every Saved pending Batch, not merely projected rows.
pub(crate) fn replacement_snapshot(shell: &ShellState) -> Result<Value, String> {
    if !named() || case() != "retirement" {
        return Err("Closed missing-channel fixture required".into());
    }
    let mut workspaces = serde_json::Map::new();
    for (team, w) in &shell.core.workspaces {
        let cached = w
            .directory_channels
            .cached()
            .ok_or("Replacement source absent")?;
        if w.channels.contains_key(mentions::CHANNEL)
            || cached["records"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["source"]["id"] == mentions::CHANNEL)
            || cached["pending"].as_array().into_iter().flatten().any(|b| {
                b["rows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|r| r["id"] == mentions::CHANNEL)
            })
        {
            return Err("Retired target appeared in replacement source".into());
        }
        workspaces.insert(
            team.clone(),
            json!({"team":w.team_id,"user":w.self_user_id,"generation":w.rt_generation,
            "pending":w.directory_channels.has_pending(),"saved":cached,"native":w.channels}),
        );
    }
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"team":shell.core.active_team,
        "targetAbsent":true,"workspaces":workspaces}),
    )
}
