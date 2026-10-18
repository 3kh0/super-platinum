//! Passive exact-gated observer around the existing one-use card move worker.
use super::*;

fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected Board card move checker required".into())
    }
}
fn target(t: &Target) -> bool {
    case().is_some_and(|mode|matches!(t,Target::CardOrder(a) if a.drag.view=="V_LANE"&&a.drag.record==mode.row()&&a.drag.source=="OPT_PROGRESS"&&a.destination==mode.destination()&&a.before.as_deref()==mode.before()))
}
pub(crate) fn registry_empty(shell: &ShellState) -> Result<bool, String> {
    require(shell)?;
    Ok(entry(shell).is_none() && super::super::super::presentation(shell).is_none())
}
pub(crate) fn initial_readiness(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let v = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Missing initial List")?;
    let l = v.loaded.as_ref().ok_or("Missing initial Loaded")?;
    if v.loading
        || v.stale
        || v.unavailable
        || selected_view(shell) != Some("V_LANE")
        || !registry_empty(shell)?
        || l.records.records.len() != 73
        || l.records.raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Card fixture was not normalized".into());
    }
    Ok(
        json!({"provenance":"controlled-offline-seed","loading":false,"stale":false,"unavailable":false,"selected":"V_LANE","document":l.document.raw,"population":l.records.raw}),
    )
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| target(&e.target))
        .ok_or("Missing card move custody")?;
    if e.input != Input::CardOrder {
        return Err("Wrong card move input".into());
    }
    let Target::CardOrder(a) = &e.target else {
        return Err("Wrong card move target".into());
    };
    let l = ledger().lock().unwrap();
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"serial":e.serial,"target":e.target.id(),
        "original":e.original.raw,"population":e.original_records.as_ref().map(|p|&p.raw),
        "source":a.drag.source,"record":a.drag.record,"destination":a.destination,"before":a.before,
        "serverPopulation":l.board_records,
        "intent":e.intent.as_ref().map(|i|match i {Intent::CardOrder(i)=>json!({"view":i.view_id(),"record":i.record_id(),"position":i.desired_position(),"cells":i.cells()}),_=>Value::Null})}),
    )
}
pub(crate) fn refuse_stale_action(shell: &mut ShellState) -> Result<Value, String> {
    require(shell)?;
    if !registry_empty(shell)? || snapshot(shell)?["requests"] != json!([]) {
        return Err("Stale card probe must precede dispatch".into());
    }
    let mode = case().ok_or("Missing exact extended case")?;
    let instance = json!(["OPT_PROGRESS", mode.row()]).to_string();
    let drag = super::super::super::card_order::begin(shell, "OPT_PROGRESS", &instance)
        .ok_or("Missing current card drag")?;
    let action = super::super::super::card_order::destination(
        shell,
        &drag,
        mode.destination(),
        mode.before(),
    )
    .ok_or("Missing current card destination")?;
    let before = snapshot(shell)?;
    let find = shell.slack_lists.view.as_ref().unwrap().layout.find.clone();
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .find
        .revision = find.revision.checked_add(1).ok_or("Find exhausted")?;
    let rebound = super::super::super::card_order::destination(
        shell,
        &drag,
        mode.destination(),
        mode.before(),
    )
    .is_some();
    let opened = super::super::super::card_order::open_shell(shell, action);
    shell.slack_lists.view.as_mut().unwrap().layout.find = find;
    if rebound || opened || !registry_empty(shell)? || snapshot(shell)? != before {
        return Err("Stale card action admitted".into());
    }
    Ok(
        json!({"kind":"stale-find-revision-admission","destination":false,"opened":false,"registryEmpty":true,"requests":0,"restoredFind":true}),
    )
}
pub(crate) fn seed_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking && target(&e.target) && e.intent.is_some() && e.job.is_some()
        })
        .ok_or("No retained card move Check")?;
    {
        let l = ledger().lock().unwrap();
        if l.held_read.is_none() || e.original_records.as_deref() != Some(&loaded.records) {
            return Err("Wrong held Check or fixture population".into());
        }
    }
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.view_selection == selection(shell)
        || case().is_none_or(|mode| find.input != mode.query())
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong accepted conflict or replacement Find".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement List")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.find.applied.is_empty()
    {
        return Err("Replacement List already populated".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
pub(crate) fn retired_read_receipt(shell: &ShellState, held: &Value) -> Result<Value, String> {
    require(shell)?;
    let now = snapshot(shell)?;
    if held["requests"].as_array().map(Vec::len) != Some(10)
        || held["responses"].as_array().map(Vec::len) != Some(10)
        || held["finished"].as_array().map(Vec::len) != Some(2)
        || held["requests"][9]["path"] != "/api/files.info"
        || held["readPending"] != true
        || held["writePending"] != false
        || now["requests"] != held["requests"]
        || now["responses"] != held["responses"]
        || now["readPending"] != false
        || now["writePending"] != false
        || now["finished"].as_array().map(Vec::len) != Some(3)
        || now["finished"][0] != held["finished"][0]
        || now["finished"][1] != held["finished"][1]
        || now["finished"][2]["kind"] != "check"
        || now["finished"][2]["serial"] != held["finished"][0]["serial"]
        || now["finished"][2]["applied"] != false
        || now["server"] != held["server"]
    {
        return Err("Retired card move read issued later calls or applied stale data".into());
    }
    Ok(
        json!({"firstPath":"/api/files.info","requestsBefore":10,"requestsAfter":10,"laterReads":0,"applied":false}),
    )
}
