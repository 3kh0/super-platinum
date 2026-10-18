//! Closed explicit Retry cases. Source records enter only through initial boot;
//! all later updates come from the production known-member Read/Page adapter.
pub(crate) use super::member_recovery::{
    DRAFT, FLAG, LABEL, MEMBER, blocks, pending, release, requests,
};
use super::*;
use serde_json::json;

pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-member-known" | "profile-member-known-narrow")
        )
}
pub(crate) fn enabled() -> bool {
    named() && super::member_recovery::enabled()
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_KNOWN_CASE").unwrap_or_else(|_| "stale".into())
}
pub(crate) fn install(shell: &mut ShellState) {
    super::member_recovery::install(shell);
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !enabled() {
        return Err("Closed known member fixture required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing known fixture owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let mut target = success(&owner.team)["results"][0].clone();
    if case() == "terminal" {
        target["isNonExistent"] = json!(true);
    }
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Member Retry"},"self":user,"users":[user,target],
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Member Retry"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    if case() != "terminal" {
        super::member_recovery::invalidate(&mut state.write());
    }
    Ok(())
}
pub(crate) fn success(team: &str) -> Value {
    let mut response = super::member_recovery::success(team);
    response["results"][0]["updated"] = json!(if case() == "terminal" { 0 } else { 101 });
    response
}
/// An existing typed row is deliberately damaged; canonical source is untouched.
pub(crate) fn native_drift(shell: &mut ShellState) -> Result<(), String> {
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let user = shell
        .core
        .workspaces
        .get_mut(&team)
        .and_then(|w| w.users.get_mut(MEMBER))
        .ok_or("Missing existing native member")?;
    let mut raw = serde_json::to_value(&*user).map_err(|_| "Serialize native fixture member")?;
    raw["profile"]["display_name"] = json!("Native drift must be repaired");
    *user = serde_json::from_value(raw).map_err(|_| "Decode native fixture member")?;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let mut out = super::member_recovery::snapshot(shell);
    let cache = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .and_then(|w| w.directory_members.cached());
    let record = cache
        .as_ref()
        .and_then(|c| c["records"].as_array())
        .and_then(|rows| rows.iter().find(|r| r["source"]["id"] == MEMBER));
    out["source"] = record.map(|r| r["source"].clone()).unwrap_or(Value::Null);
    out["interaction"] = record
        .map(|r| r["interaction"].clone())
        .unwrap_or(Value::Null);
    out["disconnected"] = cache
        .as_ref()
        .map(|c| c["disconnected"].clone())
        .unwrap_or(Value::Null);
    out["invalidated"] = json!(
        cache
            .as_ref()
            .and_then(|c| c["invalidated"].as_array())
            .is_some_and(|ids| ids.iter().any(|id| id == MEMBER))
    );
    out["driver"] = crate::directory_member_fetches::known::fixture_status(shell);
    out["absentDriver"] = crate::directory_member_fetches::driver::fixture_status(shell);
    out
}
pub(crate) fn replace_account(state: Signal<ShellState>) -> Value {
    let before = snapshot(&state.peek());
    let replaced = super::member_recovery::replace_account(state);
    json!({"before":before,"after":snapshot(&state.peek()),"actualInstallLoaded":replaced["actualInstallLoaded"]})
}
