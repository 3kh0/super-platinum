//! Passive witnesses of real worker/local state. No invented successful callbacks.
use super::*;
pub(crate) fn state(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let view = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    let loaded = view.loaded.as_ref().ok_or("Missing Loaded")?;
    let local = view
        .layout
        .refinements
        .all_document(&loaded.document)
        .map_err(|e| e.to_string())?;
    let entry = entry(shell);
    let receipt = entry.as_ref().and_then(|e| e.refinement_receipt.as_ref());
    let visibility = entry
        .as_ref()
        .and_then(|e| super::super::super::visibility::presentation(shell, e));
    Ok(
        json!({"visit":view.generation,"read":view.read,"revision":view.revision,"selection":view.layout.selection_generation,"layout":view.layout.generation,
        "selected":view.layout.selected,"pending":view.layout.pending_refinement,"loading":view.loading,"stale":view.stale,"unavailable":view.unavailable,
        "document":loaded.document.raw,"population":loaded.records.raw,"localDocument":local.raw,"localEmpty":view.layout.refinements.is_empty(),
        "provider":view.layout.board_order.as_ref().map(|p|p.typed_order()),"find":{"input":view.layout.find.input,"applied":view.layout.find.applied,"revision":view.layout.find.revision},
        "stage":entry.as_ref().map(|e|format!("{:?}",e.stage)),"serial":entry.as_ref().map(|e|e.serial),"job":entry.as_ref().and_then(|e|e.job),
        "name":entry.as_ref().and_then(|e|e.input.name()),"visibility":visibility.map(|v|json!({"column":v.column,"field":v.field,"original":v.original,"desired":v.desired,"current":v.current,"choices":v.choices,"choice":v.choice})),
        "receipt":receipt.map(|r|json!({"token":r.token,"dismissed":r.dismissed_on_ack,"acknowledgedId":r.acknowledged_id,"preference":format!("{:?}",r.preference),"custodyCurrent":r.custody.as_ref().is_some_and(|c|c.current(shell))}))}),
    )
}
pub(crate) fn initial_readiness(shell: &ShellState) -> Result<Value, String> {
    let proof = state(shell)?;
    if proof["loading"] != false
        || proof["stale"] != false
        || proof["unavailable"] != false
        || proof["selected"] != "V_LANE"
        || proof["pending"] != Value::Null
        || proof["localEmpty"] != true
        || proof["provider"] != Value::Null
        || proof["stage"] != Value::Null
        || proof["population"]["records"].as_array().map(Vec::len) != Some(73)
        || proof["population"]["response_metadata"]["next_cursor"] != "precision-opaque+/="
        || !ledger().lock().unwrap().requests.is_empty()
    {
        return Err("Refinement fixture is not a clean controlled initial state".into());
    }
    Ok(json!({"provenance":"controlled-offline-setup","state":proof}))
}
pub(crate) fn refuse_old_confirmation(
    shell: &mut ShellState,
    old_serial: u64,
) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Editing && e.local_refinement.is_some() && e.serial != old_serial
        })
        .ok_or("No replacement local Fields editor")?;
    let before = state(shell)?;
    let ledger_before = snapshot(shell)?;
    let visible_before = super::super::super::presentation(shell)
        .ok_or("No Fields presentation")?
        .visibility;
    if ledger_before["requests"] != json!([]) || visible_before.is_none() {
        return Err("Old confirmation must precede all dispatch".into());
    }
    let prepared = super::super::super::prepare(shell, old_serial).is_some();
    let closed = super::super::super::close_entry(shell, old_serial);
    if prepared
        || closed
        || state(shell)? != before
        || snapshot(shell)? != ledger_before
        || super::super::super::presentation(shell).and_then(|p| p.visibility) != visible_before
        || entry(shell).is_none_or(|e| e.intent.is_some() || e.job.is_some())
    {
        return Err("Old confirmation altered current local draft or authority".into());
    }
    Ok(
        json!({"oldSerial":old_serial,"currentSerial":e.serial,"prepared":false,"closed":false,"requests":0,"state":before}),
    )
}
/// Called while the real preference future remains held. It proves completion
/// did not await the independent child and did not install ACK metadata.
pub(crate) fn parent_completed_while_child_held(shell: &ShellState) -> Result<Value, String> {
    let mode = case().ok_or("No fixture case")?;
    let proof = state(shell)?;
    let l = ledger().lock().unwrap();
    if mode == Case::Lost
        || l.refinement_child_pending.is_none()
        || proof["provider"] != Value::Null
        || proof["selected"] != mode.view_id()
        || proof["receipt"]["acknowledgedId"] != mode.view_id()
        || proof["receipt"]["dismissed"] != mode.created()
        || proof["localEmpty"] != mode.created()
        || proof["receipt"]["preference"] != "InFlight"
        || proof["receipt"]["custodyCurrent"] != true
    {
        return Err("Parent did not complete independently under current custody".into());
    }
    if mode.created()
        && (proof["pending"] != mode.view_id()
            || !l.held_read.is_some()
            || proof["document"] == l.server.clone().unwrap())
    {
        return Err("ACK content was installed or pending metadata read was not held".into());
    }
    Ok(
        json!({"state":proof,"events":l.refinement_events,"requests":l.requests.len(),"responses":l.responses.len()}),
    )
}
pub(crate) fn lost_reply_kept_local_state(shell: &ShellState) -> Result<Value, String> {
    let proof = state(shell)?;
    let l = ledger().lock().unwrap();
    if case() != Some(Case::Lost)
        || proof["stage"] != "Uncertain"
        || proof["selected"] != "V_LANE"
        || proof["pending"] != Value::Null
        || proof["provider"] == Value::Null
        || proof["localEmpty"] != false
        || proof["receipt"]["acknowledgedId"] != Value::Null
        || proof["receipt"]["dismissed"] != false
        || proof["receipt"]["preference"] != "NotRequested"
        || l.requests.len() != 5
        || l.refinement_child_pending.is_some()
    {
        return Err("Lost reply invented selection/child or discarded local state".into());
    }
    Ok(json!({"state":proof,"events":l.refinement_events}))
}
