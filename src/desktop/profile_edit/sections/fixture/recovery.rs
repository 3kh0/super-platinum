//! Closed selective recovery acceptance; every refresh uses the shared scheduler.
use super::*;
use crate::app_home::channel_fetches::{self, canonical, conditional};
use serde_json::json;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CHANNEL_RECOVERY_CHECK";
pub(crate) const DRAFT: &str = "Recovery draft with ";
pub(crate) const RENAMED: &str = "recovered-harbor";
pub(crate) const QUEUED: &str = "conditional-harbor";
pub(crate) const SOCKET: &str = "socket-latest-harbor";
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-channel-recovery" | "profile-channel-recovery-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    mentions::install(shell);
    conditional::fixture::install(shell);
}
pub(crate) fn row(name: &str, updated: u64) -> Value {
    json!({"id":mentions::CHANNEL,"name":name,"updated":updated,
        "is_channel":true,"is_member":true,"fixture_metadata":{"retained":true}})
}
/// Initial boot only. Selective success/failure never uses this seed path.
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    let owner = Owner::capture(&state.peek()).ok_or("Missing recovery owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Channel recovery"},"self":user,"users":[user],
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true},row(mentions::LABEL,101)],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Channel recovery"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    canonical::fixture_seed(state, boot, init).await
}
pub(crate) fn snapshot(shell: &mut ShellState) -> Value {
    let profile = mentions::snapshot(shell);
    let team = shell.core.active_team.clone().unwrap_or_default();
    let source = canonical::capture(shell, &team)
        .ok()
        .and_then(|c| c.row(mentions::CHANNEL).cloned());
    let native = shell
        .core
        .workspaces
        .get(&team)
        .and_then(|w| w.channels.get(mentions::CHANNEL));
    json!({"profile":profile,"source":source,"nativeName":native.and_then(|c|c.name.as_ref()),
        "account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "socketGeneration":shell.core.workspaces.get(&team).map(|w|w.rt_generation),
        "idle":conditional::fixture::idle(shell),"conditional":conditional::fixture::status(shell)})
}
pub(crate) fn retire_socket(mut state: Signal<ShellState>) -> Result<Value, String> {
    let (before, after, work) = {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        let old = shell.core.workspaces[&team].rt_generation;
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = old + 1;
        let work = channel_fetches::connected(&mut shell, &team, old + 1)
            .ok_or("Replacement socket refused")?;
        (old, old + 1, work)
    };
    channel_fetches::resume(state, Some(work));
    Ok(json!({"before":before,"after":after}))
}
pub(crate) fn socket_rename(mut state: Signal<ShellState>) -> Result<(), String> {
    let work = {
        let mut shell = state.write();
        let team = shell
            .core
            .active_team
            .clone()
            .ok_or("Missing socket team")?;
        let generation = shell.core.workspaces[&team].rt_generation;
        let raw = serde_json::from_value(json!({"type":"channel_rename",
            "channel":{"id":mentions::CHANNEL,"name":SOCKET},"event_ts":"2000000001.0"}))
        .map_err(|_| "Invalid closed socket rename")?;
        let event = super_platinum_core::slack::events::RtEvent::Unknown(raw);
        let before = channel_fetches::before_event(&shell, &team, generation, &event)
            .ok_or("Socket rename source capture refused")?;
        crate::realtime::apply_in_view(&mut shell.core, &team, generation, event, false);
        let work = channel_fetches::after_event(&mut shell, Some(before))
            .ok_or("Socket rename source enqueue refused")?;
        shell.refresh_from_core();
        work
    };
    channel_fetches::resume(state, Some(work));
    Ok(())
}
