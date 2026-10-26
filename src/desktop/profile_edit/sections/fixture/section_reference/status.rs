//! Exact current event composition after an actual private files.info receipt.
use super::*;
use crate::directory_member_fetches::Context;
use crate::profile_edit::sections::rich::search::file_source::reference;
use super_platinum_core::slack::events::{RawEvent, RtEvent};

pub(crate) fn delete(shell: &mut ShellState) -> Result<Value, String> {
    if !named() || case() != "status" || !mentions::enabled(shell) {
        return Err("Closed section status fixture required".into());
    }
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let owner = Context::capture(shell, &team).map_err(|e| e.to_string())?;
    let before = reference::fixture_proof(shell, &team, &[files::ID]);
    let raw = json!({"type":"file_deleted","file_id":files::ID,"opaque":{"sectionStatus":true}});
    let event: RawEvent = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    if !owner.current(shell) {
        return Err("Section status event owner retired".into());
    }
    let event = RtEvent::Unknown(event);
    if crate::file_events::prepare(shell, &team, owner.realtime_generation(), &event).is_some() {
        return Err("Deleted section scheduled metadata read".into());
    }
    crate::realtime::apply_in_view(
        &mut shell.core,
        &team,
        owner.realtime_generation(),
        event,
        true,
    );
    shell.refresh_from_core();
    let after = reference::fixture_proof(shell, &team, &[files::ID]);
    if before.is_null() || after.is_null() || before == after {
        return Err("Section status event omitted its source transition proof".into());
    }
    Ok(
        json!({"raw":raw,"acceptedOwner":true,"generation":owner.realtime_generation(),"before":before,"after":after,"scheduledRead":false}),
    )
}
