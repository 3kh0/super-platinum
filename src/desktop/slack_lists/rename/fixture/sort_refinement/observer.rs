//! Passive observations and explicit offline replacement setup only.
use super::*;
pub(crate) fn state(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let v = shell.slack_lists.view.as_ref().ok_or("Missing Sort List")?;
    let loaded = v.loaded.as_ref().ok_or("Missing Sort metadata")?;
    let effective =
        super::super::super::super::refinement::document(v).map_err(|e| e.to_string())?;
    let selected = effective
        .views
        .iter()
        .find(|v| v["id"] == "V_SORT")
        .ok_or("Missing Sort view")?;
    let p = super::super::super::presentation(shell);
    let e = entry(shell);
    Ok(
        json!({"visit":v.generation,"selection":v.layout.selection_generation,"epoch":shell.core.account_epoch,
        "selected":v.layout.selected,"expanded":v.expanded,"document":loaded.document.raw,"population":loaded.records.raw,
        "find":{"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision},
        "effectiveSorts":selected["sorts"],"localEmpty":v.layout.refinements.is_empty(),
        "local":p.as_ref().map(|p|p.local_refinement),"stage":p.as_ref().map(|p|format!("{:?}",p.stage)),
        "serial":p.as_ref().map(|p|p.serial),"message":p.as_ref().map(|p|&p.message),
        "preference":p.as_ref().and_then(|p|p.refinement_save.as_ref().map(|p|p.preference)),
        "refinement":e.as_ref().is_some_and(|e|e.target.is_refinement()),
        "original":e.as_ref().map(|e|&e.original.raw),"rawReply":e.as_ref().and_then(|e|e.raw_reply.as_ref()),
        "pending":v.layout.pending_refinement,"provider":v.layout.board_order.is_some()}),
    )
}
pub(crate) fn stale_file(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Uncertain && e.target.is_refinement())
        .ok_or("No uncertain refinement")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
pub(crate) fn stale_confirmation(shell: &mut ShellState, serial: u64) -> Result<Value, String> {
    require(shell)?;
    let before = state(shell)?;
    let ledger_before = snapshot(shell)?;
    if entry(shell).is_none_or(|e| {
        e.stage != Stage::Editing || e.serial == serial || e.local_refinement.is_none()
    }) {
        return Err("Distinct current local editor required".into());
    }
    if super::super::super::prepare(shell, serial).is_some()
        || super::super::super::close_entry(shell, serial)
        || state(shell)? != before
        || snapshot(shell)? != ledger_before
    {
        return Err("Stale Sort confirmation was admitted".into());
    }
    Ok(json!({"refused":true,"before":before,"after":state(shell)?,"ledger":ledger_before}))
}
pub(crate) fn seed_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Checking && e.target.is_refinement() && e.job.is_some())
        .ok_or("Missing held refinement")?;
    if ledger().lock().unwrap().held_read.is_none()
        || generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong captured Sort replacement".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement")?;
    if v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.refinements.is_empty()
    {
        return Err("Replacement not empty".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
