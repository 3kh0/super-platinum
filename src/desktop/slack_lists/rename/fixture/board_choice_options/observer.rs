//! Passive exact-gated observer around the existing one-use choice worker.
use super::*;
use super_platinum_core::slack_lists::BoardChoiceAction;

fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected Board choice checker required".into())
    }
}
fn target(t: &Target) -> bool {
    matches!(t,Target::BoardChoice(c) if c.view=="V_LANE" && case().is_some_and(|mode| c.kind==mode.kind() && c.value.as_deref()==if mode==Case::Add {None}else{Some("OPT_PROGRESS")}))
}
pub(crate) fn action_value(action: &BoardChoiceAction) -> Value {
    match action {
        BoardChoiceAction::Add {
            value,
            label,
            color,
        } => json!({"kind":"add","value":value,"label":label,"color":color}),
        BoardChoiceAction::Color { value, color } => {
            json!({"kind":"color","value":value,"label":null,"color":color})
        }
        _ => Value::Null,
    }
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
        return Err("Choice fixture was not normalized".into());
    }
    Ok(
        json!({"provenance":"controlled-offline-seed","loading":false,"stale":false,"unavailable":false,"selected":"V_LANE","document":l.document.raw,"population":l.records.raw}),
    )
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| target(&e.target))
        .ok_or("Missing choice custody")?;
    let Input::BoardChoice(action) = &e.input else {
        return Err("Wrong choice draft".into());
    };
    let l = ledger().lock().unwrap();
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"serial":e.serial,"target":e.target.id(),
        "original":e.original.raw,"fixturePopulation":l.board_records,"input":action_value(action),
        "intent":e.intent.as_ref().map(|i|match i {Intent::BoardChoice(i)=>json!({"view":i.view_id(),"options":i.options()}),_=>Value::Null})}),
    )
}
pub(crate) fn refuse_old_confirmation(
    shell: &mut ShellState,
    old_serial: u64,
) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Editing && target(&e.target) && e.serial != old_serial)
        .ok_or("No replaced confirmation")?;
    if snapshot(shell)?["requests"] != json!([]) {
        return Err("Stale confirmation must precede all dispatch".into());
    }
    let before = custody(shell)?;
    let prepared = super::super::super::prepare(shell, old_serial).is_some();
    let closed = super::super::super::close_entry(shell, old_serial);
    if prepared
        || closed
        || custody(shell)? != before
        || entry(shell).is_none_or(|e| e.intent.is_some() || e.job.is_some())
    {
        return Err("Old choice confirmation changed current authority".into());
    }
    Ok(
        json!({"oldSerial":old_serial,"currentSerial":e.serial,"prepared":false,"closed":false,"writes":0}),
    )
}
pub(crate) fn conflict_file(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && target(&e.target)
                && matches!(e.intent, Some(Intent::BoardChoice(_)))
        })
        .ok_or("No uncertain choice")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
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
        .ok_or("No retained choice Check")?;
    {
        let l = ledger().lock().unwrap();
        if l.held_read.is_none() || l.board_records.as_ref() != Some(&loaded.records.raw) {
            return Err("Wrong held Check or fixture population".into());
        }
    }
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
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
    if held["requests"].as_array().map(Vec::len) != Some(9)
        || held["responses"].as_array().map(Vec::len) != Some(9)
        || held["finished"].as_array().map(Vec::len) != Some(2)
        || held["requests"][8]["path"] != "/api/files.info"
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
        return Err("Retired choice read issued later calls or applied stale data".into());
    }
    Ok(
        json!({"firstPath":"/api/files.info","requestsBefore":9,"requestsAfter":9,"laterReads":0,"applied":false}),
    )
}
