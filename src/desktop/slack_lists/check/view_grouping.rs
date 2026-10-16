//! Nine captures of actual shared grouping writes and canonical readback retirement.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
mod model;
mod oracle;
use model::Proof;
use rename_fixture::grouping as adapter;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !adapter::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"failedLedger":rename_fixture::snapshot(&state.peek()).ok(),"failedDocument":document(state).ok(),"failedPopulation":population(state).ok(),"failedProjection":model::projection(state).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("slack-list-view-grouping-check: {receipt}\n").as_bytes())
                .expect("Group by terminal output failed");
        })
        .await
        .expect("Group by terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Group by did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held grouping {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let ledger = rename_fixture::snapshot(&state.peek())?;
        match ledger["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(ledger),
            Some(n) if n < count => {}
            _ => return Err("Group by completion count exceeded expected checkpoint".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Group by owner completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing grouping document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing grouping population".into())
}
fn find(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let v = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    Ok(
        json!({"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}),
    )
}

fn choice(value: &super_platinum_core::slack_lists::ViewGroupingChoice) -> Value {
    use super_platinum_core::slack_lists::ViewGroupingChoice;
    match value {
        ViewGroupingChoice::None => Value::Null,
        ViewGroupingChoice::Column { column_id } => json!(column_id),
    }
}
fn draft(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = rename::presentation(&shell).ok_or("Missing Group by presentation")?;
    let g = p.view_grouping.ok_or("Missing typed Group by choice")?;
    if p.view.as_deref() != Some("V_GROUP")
        || g.choice == 0
        || p.precision.is_some()
        || p.filter.is_some()
        || p.sort.is_some()
    {
        return Err("Wrong Group by presentation".into());
    }
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&shell),"selection":rename::selection(&shell),"choice":g.choice,"original":choice(&g.original),"desired":choice(&g.desired),"current":g.current.as_ref().map(choice),"choices":g.choices}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(
        d,
        &format!(
            "window.__listViewGrouping.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
    .map(|_| ())
}
async fn select(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<Value, String> {
    let old = draft(state)?;
    let action = checked(
        d,
        &format!("window.__listViewGrouping.select({})", json!(id)),
    )
    .await?;
    wait_dom(d,&format!("({{ok:document.getElementById('slack-list-grouping-field')?.value==={}&&document.getElementById('slack-list-grouping-field')?.dataset.groupingChoice!=={}}})",json!(id),json!(old["choice"].as_u64().unwrap().to_string()))).await?;
    let current = draft(state)?;
    if current["desired"]
        != if id.is_empty() {
            Value::Null
        } else {
            json!(id)
        }
        || current["choice"].as_u64() <= old["choice"].as_u64()
    {
        return Err("Grouping choice event did not settle".into());
    }
    let dom = checked(d, "window.__listViewGrouping.draft()").await?;
    if dom["choice"].as_str() != current["choice"].as_u64().map(|v| v.to_string()).as_deref() {
        return Err("Grouping DOM choice differs".into());
    }
    Ok(json!({"model":current,"dom":dom,"action":action}))
}
async fn open_choice(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    id: &str,
) -> Result<Value, String> {
    checked(d, "window.__listViewGrouping.rememberGrouping()").await?;
    click(d, "slack-list-grouping").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-grouping-field')===document.activeElement})",
    )
    .await?;
    select(state, d, id).await
}
async fn done(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    checked(d, "window.__listViewGrouping.rememberGrouping()").await?;
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listViewGrouping.returnedGrouping()").await
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("view_grouping.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-grouping')&&!document.getElementById('slack-list-rename')})").await?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial Find".into());
    }
    checked(d, "window.__listViewGrouping.query()").await?;
    let mut applied = false;
    for _ in 0..100 {
        let f = find(state)?;
        if f["input"] == "Launch item 0"
            && f["applied"] == f["input"]
            && f["revision"].as_u64().is_some_and(|r| r > 0)
        {
            applied = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if !applied {
        return Err("Actual Find debounce did not apply".into());
    }
    let first_find = checked(d, "window.__listViewGrouping.bindFind()").await?;
    let before = document(state)?;
    let raw_population = population(state)?;
    if before["list_metadata"]["views"][0]["id"] != "V_GROUP"
        || before["list_metadata"]["views"][0]["grouping"] != json!({"group_by":"done"})
        || before["list_metadata"]["views"][0]["filters"]
            != json!([{"key":"estimate","operator":"greater_than","values":["-4"]}])
        || before["list_metadata"]["views"][0]["sorts"]
            != json!([{"key":"estimate","ascending":true}])
        || before["list_metadata"]["views"][0]["info_column_filters"] != json!([])
        || raw_population["records"].as_array().map(Vec::len) != Some(73)
        || raw_population["records"][11]["parent_record_id"] != "R_LIST_010"
        || raw_population["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong Group by raw seed".into());
    }
    let projected = model::projection(state)?;
    let mut proof = Proof {
        board: projected["board"].as_bool().unwrap(),
        docs: oracle::documents(&before),
        population: raw_population,
        find: find(state)?,
        expanded: projected["expanded"].clone(),
        ids: [0; 3],
        stages: vec![],
        captures: Default::default(),
    };
    proof.stable(state, 0)?;
    let canceled = open_choice(state, d, "C_SELECT").await?;
    let cancel_focus = done(d).await?;
    let cancel_ledger = rename_fixture::snapshot(&state.peek())?;
    if cancel_ledger["requests"] != json!([])
        || cancel_ledger["responses"] != json!([])
        || cancel_ledger["finished"] != json!([])
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Cancel retained an operation".into());
    }
    proof.stable(state, 0)?;
    let first_choice = open_choice(state, d, "C_SELECT").await?;
    let none = select(state, d, "").await?;
    let chosen = select(state, d, "C_SELECT").await?;
    let serial = chosen["model"]["serial"].as_u64().unwrap();
    let visit = chosen["model"]["visit"].as_u64().unwrap();
    let old_callbacks = adapter::refuse_old_choice(
        &state.peek(),
        serial,
        visit,
        first_choice["model"]["choice"].as_u64().unwrap(),
    )?;
    proof.ids[0] = serial;
    if chosen["model"]["serial"] == canceled["model"]["serial"] {
        return Err("Canceled grouping identity reused".into());
    }
    proof.stage(state, d, "select-edit").await?;
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "select-saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    proof.stage(state, d, "uncertain").await?;
    let retirement_setup = model::setup_retirement(state)?;
    adapter::conflict_file(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    let conflict_retirement = model::retired_layout(state, &retirement_setup)?;
    proof.stage(state, d, "conflict").await?;
    rename_fixture::hold_read(&state.peek())?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&held, 8, 2, false, true, 1, &proof.docs, proof.ids)?;
    let held_custody = adapter::custody(&state.peek())?;
    let (loaded, saved_find, expanded, collapsed, old_visit, old_selection, old_epoch) = {
        let s = state.peek();
        let v = s.slack_lists.view.as_ref().unwrap();
        (
            v.loaded.clone().unwrap(),
            v.layout.find.clone(),
            v.expanded.clone(),
            v.layout.collapsed.clone(),
            v.generation,
            v.layout.selection_generation,
            s.core.account_epoch,
        )
    };
    click(d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Fixture epoch exhausted")?;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    adapter::seed_reopen(&mut state.write(), new_visit, loaded, saved_find)?;
    // Explicit replacement fixture setup preserves captured hierarchy/collapse, not a product reopen claim.
    {
        let mut s = state.write();
        let v = s.slack_lists.view.as_mut().unwrap();
        v.expanded = expanded;
        v.layout.collapsed = collapsed;
        crate::slack_lists::rebuild_rows(&mut s);
    }
    proof.stable(state, 2)?;
    let setup = json!({"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":model::projection(state)?,"ledger":rename_fixture::snapshot(&state.peek())?});
    let new_selection = rename::selection(&state.peek()).unwrap();
    if old_visit == new_visit || old_selection == new_selection || setup["ledger"] != held {
        return Err("Replacement owner/held read drift".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    let rebound_find = checked(d, "window.__listViewGrouping.bindFind()").await?;
    wait_dom(d, "window.__listViewGrouping.rememberBusy()").await?;
    rename_fixture::release(&state.peek(), true)?;
    let obsolete = completed(state, 3).await?;
    oracle::exact(&obsolete, 8, 3, false, false, 1, &proof.docs, proof.ids)?;
    phase(state, Stage::Uncertain).await?;
    proof.stable(state, 2)?;
    let retained = adapter::custody(&state.peek())?;
    for key in ["serial", "choice", "original", "intent", "account"] {
        if retained[key] != held_custody[key] {
            return Err("Obsolete Check replaced retained original intent".into());
        }
    }
    let retired = json!({"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"custodyBefore":held_custody,"setup":setup,"completed":obsolete,"document":document(state)?,"population":population(state)?,"find":find(state)?,"reboundFind":rebound_find,"retained":retained});
    proof.stage(state, d, "retired").await?;
    let before_select = model::projection(state)?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 1)?;
    let observed_select = draft(state)?;
    let select_revision = revision_changed(state, &before_select)?;
    let select_focus = done(d).await?;
    proof.stage(state, d, "select").await?;
    let checkbox = open_choice(state, d, "C_CHECK").await?;
    proof.ids[1] = checkbox["model"]["serial"].as_u64().unwrap();
    let checkbox_ack = save_positive(state, d, &proof, 1, 15, 4, 0).await?;
    let before_checkbox = model::projection(state)?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 0)?;
    let observed_checkbox = draft(state)?;
    let checkbox_revision = revision_changed(state, &before_checkbox)?;
    let checkbox_focus = done(d).await?;
    proof.stage(state, d, "checkbox").await?;
    let clear = open_choice(state, d, "").await?;
    proof.ids[2] = clear["model"]["serial"].as_u64().unwrap();
    proof.stage(state, d, "clear-edit").await?;
    let clear_ack = save_positive(state, d, &proof, 0, 22, 6, 3).await?;
    let before_clear = model::projection(state)?;
    click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    proof.stable(state, 3)?;
    let observed_clear = draft(state)?;
    let clear_revision = revision_changed(state, &before_clear)?;
    let clear_focus = done(d).await?;
    proof.stage(state, d, "cleared").await?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&ledger, 25, 8, false, false, 3, &proof.docs, proof.ids)?;
    if !adapter::registry_empty(&state.peek())? {
        return Err("Done retained entry/intent".into());
    }
    proof.stable(state, 3)?;
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"board":proof.board,"noSession":true,"noTransport":true,"documents":proof.docs,"population":proof.population,"find":proof.find,"firstFind":first_find,"attempts":proof.ids,"cancel":{"draft":canceled,"focus":cancel_focus,"ledger":cancel_ledger,"registryEmpty":true},"choices":{"select":chosen,"checkbox":checkbox,"clear":clear},"aba":{"first":first_choice,"none":none,"refused":old_callbacks},"conflictRetirement":conflict_retirement,"retiredCheck":retired,"observed":{"select":observed_select,"checkbox":observed_checkbox,"clear":observed_clear},"revisionChanges":[select_revision,checkbox_revision,clear_revision],"returnFocus":[select_focus,checkbox_focus,clear_focus],"positiveAcks":{"checkbox":checkbox_ack,"clear":clear_ack},"doneRegistryEmpty":true,"stages":proof.stages,"captures":proof.captures,"ledger":ledger}),
    )
}
fn revision_changed(state: Signal<ShellState>, before: &Value) -> Result<Value, String> {
    let after = model::projection(state)?;
    if after["layout"] == before["layout"]
        || after["groupingRevision"] == before["groupingRevision"]
        || after["findRevision"] != before["findRevision"]
        || after["expanded"] != before["expanded"]
        || after["collapsed"]["V_OTHER"] != before["collapsed"]["V_OTHER"]
    {
        return Err("Canonical grouping change failed ownership retirement".into());
    }
    Ok(json!({"before":before,"after":after}))
}
async fn save_positive(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &Proof,
    before: usize,
    count: usize,
    finished: usize,
    after: usize,
) -> Result<Value, String> {
    click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &held,
        count,
        finished,
        true,
        false,
        before,
        &proof.docs,
        proof.ids,
    )?;
    proof.stable(state, before)?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    let ack = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &ack,
        count,
        finished + 1,
        false,
        false,
        after,
        &proof.docs,
        proof.ids,
    )?;
    proof.stable(state, before)?;
    let retained =
        rename_fixture::retained_reply(&state.peek()).ok_or("Missing real positive receipt")?;
    if retained != ack["responses"][count - 1] {
        return Err("Positive ACK retention differs".into());
    }
    Ok(
        json!({"held":held,"ledger":ack,"retainedReply":retained,"documentBeforeReadback":document(state)?}),
    )
}
