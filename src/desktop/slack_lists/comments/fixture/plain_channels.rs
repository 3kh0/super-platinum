//! Closed channel Save observations; original producer receipts are append-only.
use super::*;
use crate::app_home::channel_fetches::{self, canonical, conditional};
use std::sync::OnceLock;

pub(crate) const TEXT: &str = "Known #project-harbor and #not-found";
pub(crate) const ID: &str = "CLISTLOCAL";
pub(crate) const NAME: &str = "project-harbor";
pub(crate) const FUTURE: &str = include_str!("plain/channel-future.json");
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHANNEL_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-channel")
}
fn ledger() -> &'static Mutex<Vec<Value>> {
    static ROWS: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    ROWS.get_or_init(|| Mutex::new(Vec::new()))
}
pub(crate) fn observe(observation: Value) {
    if !named() {
        return;
    }
    let mut rows = ledger().lock().unwrap();
    let serial = rows.len() + 1;
    rows.push(json!({"serial":serial,"observation":observation}));
}
pub(crate) fn snapshot() -> Value {
    if !named() {
        return json!({"enabled":false});
    }
    json!({"enabled":true,"lookups":*ledger().lock().unwrap()})
}
pub(crate) fn seed_boot(boot: &mut Value) {
    if !named() {
        return;
    }
    for row in boot["channels"].as_array_mut().unwrap() {
        if row["id"] == ID {
            row["name"] = json!(NAME);
        }
    }
}
pub(crate) fn source(shell: &ShellState) -> Value {
    if !named() {
        return Value::Null;
    }
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let observed = canonical::observed(shell, team).ok();
    let w = shell.core.workspaces.get(team);
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "team":team,"user":w.map(|w|&w.self_user_id),"socket":w.map(|w|w.rt_generation),
        "current":observed.as_ref().is_some_and(|v|v.current(shell)),
        "rows":observed.as_ref().map(|v|v.rows()),
        "pending":w.is_some_and(|w|w.directory_channels.has_pending()),
        "conditional":conditional::fixture::status(shell)})
}
pub(crate) fn hold(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !named() || !completion::enabled(&state.peek()) {
        return Err("Closed channel mode required".into());
    }
    let prior_source = source(&state.peek());
    let prior = canonical::observed(&state.peek(), "T1").map_err(|e| e.to_string())?;
    conditional::fixture::hold_drain(&mut state.write()).map_err(|e| e.to_string())?;
    let raw =
        json!({"type":"channel_rename","channel":{"id":ID,"name":NAME},"event_ts":"2000000001.0"});
    let work = {
        let mut shell = state.write();
        let team = shell
            .core
            .active_team
            .clone()
            .ok_or("Missing channel team")?;
        let generation = shell.core.workspaces[&team].rt_generation;
        let event = super_platinum_core::slack::events::RtEvent::Unknown(
            serde_json::from_value(raw.clone()).map_err(|_| "Invalid original channel event")?,
        );
        let before = channel_fetches::before_event(&shell, &team, generation, &event)
            .ok_or("Channel event admission refused")?;
        crate::realtime::apply_in_view(&mut shell.core, &team, generation, event, false);
        let work = channel_fetches::after_event(&mut shell, Some(before))
            .ok_or("Channel event publication refused")?;
        shell.refresh_from_core();
        work
    };
    channel_fetches::resume(state, Some(work));
    if source(&state.peek())["current"] != false || source(&state.peek())["pending"] != true {
        return Err("Held original source became canonical ready".into());
    }
    Ok(
        json!({"raw":raw,"before":prior_source,"originalCurrent":prior.current(&state.peek()),"source":source(&state.peek())}),
    )
}
pub(crate) fn release(state: Signal<ShellState>) -> Result<(), String> {
    if !named() {
        return Err("Closed channel mode required".into());
    }
    conditional::fixture::release_drain(state).map_err(|e| e.to_string())
}
