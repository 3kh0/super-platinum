//! Real shared ordering worker, explicit independent readback and retired-read proof.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
use rename_fixture::view_order as adapter;
mod harness;
mod model;
mod oracle;
use model::Proof;
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
                .write_all(format!("slack-list-view-order-check: {receipt}\n").as_bytes())
                .expect("Ordering terminal output failed");
        })
        .await
        .expect("Ordering terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Ordering did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held ordering {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let l = rename_fixture::snapshot(&state.peek())?;
        match l["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(l),
            Some(n) if n < count => {}
            _ => return Err("Ordering completion count exceeded".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Ordering completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing ordering document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing ordering population".into())
}
fn find(state: Signal<ShellState>) -> Result<Value, String> {
    let s = state.peek();
    let v = s.slack_lists.view.as_ref().ok_or("Missing List")?;
    Ok(
        json!({"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision}),
    )
}
fn draft(state: Signal<ShellState>) -> Result<Value, String> {
    let s = state.peek();
    let p = rename::presentation(&s).ok_or("Missing ordering presentation")?;
    let ordering = p.reorder.ok_or("Wrong editor")?;
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&s),"selection":rename::selection(&s),
        "view":rename::selected_view(&s),"direction":ordering.direction,"before":ordering.before,
        "after":ordering.after,"current":ordering.current,"stage":format!("{:?}",p.stage),"canConfirm":p.can_confirm}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listViewOrder.click({})", json!(format!("#{id}"))),
    )
    .await
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    checked(d, "window.__listViewOrder.rememberMove('up')").await?;
    let action = click(d, "slack-list-view-move-up").await?;
    phase(state, Stage::Editing).await?;
    let model = draft(state)?;
    let dom = wait_dom(d, &format!("window.__listViewOrder.sameDraft({model})")).await?;
    Ok(json!({"model":model,"dom":dom,"action":action}))
}
async fn done(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listViewOrder.returnedMove()").await
}
async fn readiness(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    busy: bool,
) -> Result<Value, String> {
    let mut previous = None;
    for _ in 0..150 {
        if let Ok(now) = model::projection(state) {
            if previous.as_ref() == Some(&now) {
                let dom = wait_dom(d, &format!("window.__listViewOrder.ready({busy})")).await?;
                if model::projection(state)? == now {
                    return Ok(json!({"projection":now,"dom":dom,"stableSamples":2}));
                }
            }
            previous = Some(now);
        } else {
            previous = None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Ordering setup never reached the normalized selected view".into())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("view_order.js")).await?;
    let setup_ready = readiness(state, d, false).await?;
    let initial_setup = adapter::initial_readiness(&state.peek())?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial ordering Find".into());
    }
    let query_action = checked(d, "window.__listViewOrder.query()").await?;
    let mut ready = false;
    for _ in 0..100 {
        let f = find(state)?;
        if f["input"] == "Launch item 0"
            && f["applied"] == f["input"]
            && f["revision"].as_u64().is_some_and(|r| r > 0)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if !ready {
        return Err("Ordering Find debounce did not apply".into());
    }
    let first_find = checked(d, "window.__listViewOrder.bindFind()").await?;
    let before = document(state)?;
    let raw = population(state)?;
    if before["user"] != "U1"
        || before["access"] != "write"
        || raw["records"].as_array().map(Vec::len) != Some(73)
        || raw["records"][11]["parent_record_id"] != "R_LIST_010"
        || raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong ordering raw fixture".into());
    }
    let mut proof = Proof {
        docs: oracle::documents(&before)?,
        population: raw,
        find: find(state)?,
        expanded: model::projection(state)?["expanded"].clone(),
        serial: 0,
        stages: vec![],
        captures: Default::default(),
    };
    proof.stable(state, 0)?;
    let canceled_open = open(state, d).await?;
    let cancel_focus = done(d).await?;
    let cancel_ledger = rename_fixture::snapshot(&state.peek())?;
    if cancel_ledger["requests"] != json!([])
        || cancel_ledger["responses"] != json!([])
        || cancel_ledger["finished"] != json!([])
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Cancel retained ordering or sent a request".into());
    }
    proof.stable(state, 0)?;
    let opened = open(state, d).await?;
    proof.serial = opened["model"]["serial"].as_u64().ok_or("Missing serial")?;
    let stale = adapter::refuse_old_confirmation(
        &mut state.write(),
        canceled_open["model"]["serial"].as_u64().unwrap(),
    )?;
    proof.stage(state, d, "edit").await?;
    let confirm_action = click(d, "slack-list-rename-confirm").await?;
    phase(state, Stage::Saving).await?;
    pending(state, "writePending").await?;
    proof.stage(state, d, "saving").await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    completed(state, 1).await?;
    proof.stage(state, d, "uncertain").await?;
    adapter::conflict_file(&state.peek())?;
    let conflict_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    completed(state, 2).await?;
    proof.stage(state, d, "conflict").await?;
    rename_fixture::hold_read(&state.peek())?;
    let held_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&held, 8, 2, false, true, 1, &proof.docs, proof.serial)?;
    let old_custody = adapter::custody(&state.peek())?;
    let (loaded, saved_find, expanded, old_visit, old_selection, old_epoch) = {
        let s = state.peek();
        let v = s.slack_lists.view.as_ref().unwrap();
        (
            v.loaded.clone().unwrap(),
            v.layout.find.clone(),
            v.expanded.clone(),
            v.generation,
            v.layout.selection_generation,
            s.core.account_epoch,
        )
    };
    let leave_action = click(d, "slack-list-rename-leave").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    state.write().core.account_epoch = old_epoch.checked_add(1).ok_or("Fixture epoch exhausted")?;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    adapter::seed_reopen(&mut state.write(), new_visit, loaded, saved_find)?;
    {
        let mut s = state.write();
        s.slack_lists.view.as_mut().unwrap().expanded = expanded;
        crate::slack_lists::rebuild_rows(&mut s);
    }
    let replacement_ready = readiness(state, d, true).await?;
    proof.stable(state, 0)?;
    let new_selection = rename::selection(&state.peek()).unwrap();
    if old_visit == new_visit
        || old_selection == new_selection
        || rename_fixture::snapshot(&state.peek())? != held
    {
        return Err("Replacement ordering owner/ledger drift".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    let rebound = checked(d, "window.__listViewOrder.bindFind()").await?;
    let rebound_opener = checked(d, "window.__listViewOrder.rememberMove('up')").await?;
    let rebound_focus = wait_dom(d, "window.__listViewOrder.rememberBusy()").await?;
    let setup = json!({"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":model::projection(state)?,"ledger":rename_fixture::snapshot(&state.peek())?});
    rename_fixture::release(&state.peek(), true)?;
    completed(state, 3).await?;
    phase(state, Stage::Uncertain).await?;
    let retired_reads = adapter::retired_read_receipt(&state.peek(), &held)?;
    let retained = adapter::custody(&state.peek())?;
    for key in [
        "serial",
        "target",
        "direction",
        "original",
        "population",
        "intent",
        "account",
    ] {
        if retained[key] != old_custody[key] {
            return Err("Retired completion replaced ordering custody".into());
        }
    }
    proof.stage(state, d, "retired").await?;
    let observed_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    completed(state, 4).await?;
    proof.stage(state, d, "observed").await?;
    let observed = draft(state)?;
    let return_focus = done(d).await?;
    proof.stage(state, d, "table").await?;
    if !adapter::registry_empty(&state.peek())? {
        return Err("Done retained ordering intent".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&ledger, 11, 4, false, false, 1, &proof.docs, proof.serial)?;
    let history = checked(d, "window.__listViewOrder.history()").await?;
    proof.stable(state, 1)?;
    if rename_fixture::snapshot(&state.peek())? != ledger
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Ordering changed after final history witness".into());
    }
    Ok(
        json!({"ok":true,"schema":1,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"noSession":true,"noTransport":true,"documents":proof.docs,"population":proof.population,"find":proof.find,"initialSetup":initial_setup,"setupReady":setup_ready,"queryAction":query_action,"firstFind":first_find,"attempt":proof.serial,"cancel":{"open":canceled_open,"focus":cancel_focus,"ledger":cancel_ledger,"registryEmpty":true},"open":opened,"staleConfirmation":stale,"actions":{"confirm":confirm_action,"conflict":conflict_action,"held":held_action,"leave":leave_action,"observe":observed_action},"retiredCheck":{"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"custodyBefore":old_custody,"setup":setup,"ready":replacement_ready,"reboundFind":rebound,"reboundOpener":rebound_opener,"reboundFocus":rebound_focus,"retained":retained,"readReceipt":retired_reads},"observed":observed,"returnFocus":return_focus,"doneRegistryEmpty":true,"history":history,"stages":proof.stages,"captures":proof.captures,"ledger":ledger}),
    )
}
