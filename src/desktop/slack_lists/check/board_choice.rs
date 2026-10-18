//! Real shared Board choice worker, explicit independent readback and retired-read proof.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
use rename_fixture::board_choice as adapter;
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
                .write_all(format!("slack-list-board-choice-check: {receipt}\n").as_bytes())
                .expect("Board choice terminal output failed");
        })
        .await
        .expect("Board choice terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Board choice did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held Board choice {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let l = rename_fixture::snapshot(&state.peek())?;
        match l["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(l),
            Some(n) if n < count => {}
            _ => return Err("Board choice completion count exceeded".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Board choice completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing Board choice document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing Board choice population".into())
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
    let p = rename::presentation(&s).ok_or("Missing choice presentation")?;
    let choice = p.board_choice.ok_or("Wrong editor")?;
    let super_platinum_core::slack_lists::BoardChoiceAction::Rename { value, label } =
        choice.action
    else {
        return Err("Wrong choice action".into());
    };
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&s),"selection":rename::selection(&s),"view":rename::selected_view(&s),
        "value":value,"label":label,"before":p.before,"current":if matches!(p.stage,Stage::Conflict|Stage::Observed){choice.current}else{None},"stage":format!("{:?}",p.stage),"canConfirm":p.can_confirm&&choice.ready,"canEdit":p.can_confirm}),
    )
}

async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listBoardChoice.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
}
async fn done(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listBoardChoice.returnedMove()").await
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
                let dom = wait_dom(d, &format!("window.__listBoardChoice.ready({busy})")).await?;
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
    Err("Board choice setup never reached the normalized selected view".into())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("board_choice.js")).await?;
    let setup_ready = readiness(state, d, false).await?;
    let initial_setup = adapter::initial_readiness(&state.peek())?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial Board choice Find".into());
    }
    let query_action = checked(d, "window.__listBoardChoice.query()").await?;
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
        return Err("Board choice Find debounce did not apply".into());
    }
    let first_find = checked(d, "window.__listBoardChoice.bindFind()").await?;
    let before = document(state)?;
    let raw = population(state)?;
    if before["user"] != "U1"
        || before["access"] != "write"
        || raw["records"].as_array().map(Vec::len) != Some(73)
        || raw["records"][11]["parent_record_id"] != "R_LIST_010"
        || raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong Board choice raw fixture".into());
    }
    let mut proof = Proof {
        docs: oracle::documents(&before)?,
        population: oracle::populations(&raw)?,
        find: find(state)?,
        expanded: model::projection(state)?["expanded"].clone(),
        serial: 0,
        stages: vec![],
        captures: Default::default(),
    };
    proof.stable(state, 0, 0)?;
    checked(d, "window.__listBoardChoice.rememberOpen()").await?;
    let cancel_open = checked(d, "window.__listBoardChoice.open()").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(d, "window.__listBoardChoice.draft()").await?;
    let cancelled = draft(state)?;
    let cancel_action = click(d, "slack-list-rename-close").await?;
    let cancel_return = wait_dom(d, "window.__listBoardChoice.returnedCancel()").await?;
    if !adapter::registry_empty(&state.peek())? {
        return Err("Cancel kept a choice entry".into());
    }
    let cancelled_ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &cancelled_ledger,
        0,
        0,
        false,
        false,
        0,
        &proof.docs,
        &proof.population,
        0,
    )?;
    checked(d, "window.__listBoardChoice.rememberOpen()").await?;
    let open_action = checked(d, "window.__listBoardChoice.open()").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(d, "window.__listBoardChoice.draft()").await?;
    proof.serial = draft(state)?["serial"]
        .as_u64()
        .ok_or("Missing choice serial")?;
    let stale = adapter::refuse_old_confirmation(
        &mut state.write(),
        cancelled["serial"]
            .as_u64()
            .ok_or("Missing Cancel serial")?,
    )?;
    let input_action = checked(d, "window.__listBoardChoice.setLabel('In motion')").await?;
    for _ in 0..100 {
        if draft(state)?["label"] == "In motion" && draft(state)?["canConfirm"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if draft(state)?["label"] != "In motion" || draft(state)?["canConfirm"] != true {
        return Err("Choice input did not reach owned model".into());
    }
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
    let partial_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Conflict).await?;
    completed(state, 2).await?;
    proof.stage(state, d, "conflict").await?;
    rename_fixture::hold_read(&state.peek())?;
    let held_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Checking).await?;
    pending(state, "readPending").await?;
    let held = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &held,
        9,
        2,
        false,
        true,
        1,
        &proof.docs,
        &proof.population,
        proof.serial,
    )?;
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
    proof.stable(state, 0, 0)?;
    let new_selection = rename::selection(&state.peek()).unwrap();
    if old_visit == new_visit
        || old_selection == new_selection
        || rename_fixture::snapshot(&state.peek())? != held
    {
        return Err("Replacement Board choice owner/ledger drift".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    let rebound = checked(d, "window.__listBoardChoice.bindFind()").await?;
    let rebound_opener = checked(d, "window.__listBoardChoice.rememberOpen()").await?;
    let rebound_focus = wait_dom(d, "window.__listBoardChoice.rememberBusy()").await?;
    let setup = json!({"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":model::projection(state)?,"ledger":rename_fixture::snapshot(&state.peek())?});
    rename_fixture::release(&state.peek(), true)?;
    completed(state, 3).await?;
    phase(state, Stage::Uncertain).await?;
    let retired_reads = adapter::retired_read_receipt(&state.peek(), &held)?;
    let retained = adapter::custody(&state.peek())?;
    for key in [
        "serial",
        "target",
        "original",
        "fixturePopulation",
        "intent",
        "account",
    ] {
        if retained[key] != old_custody[key] {
            return Err("Retired completion replaced Board choice custody".into());
        }
    }
    proof.stage(state, d, "retired").await?;
    let observed_action = click(d, "slack-list-rename-check").await?;
    phase(state, Stage::Observed).await?;
    completed(state, 4).await?;
    proof.stage(state, d, "observed").await?;
    let observed = draft(state)?;
    let return_focus = done(d).await?;
    proof.stage(state, d, "board").await?;
    if !adapter::registry_empty(&state.peek())? {
        return Err("Done retained Board choice intent".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(
        &ledger,
        13,
        4,
        false,
        false,
        1,
        &proof.docs,
        &proof.population,
        proof.serial,
    )?;
    let history = checked(d, "window.__listBoardChoice.history()").await?;
    proof.stable(state, 1, 0)?;
    if rename_fixture::snapshot(&state.peek())? != ledger
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Board choice changed after final history witness".into());
    }
    Ok(
        json!({"ok":true,"schema":2,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"noSession":true,"noTransport":true,"documents":proof.docs,"populations":proof.population,"find":proof.find,"initialSetup":initial_setup,"setupReady":setup_ready,"queryAction":query_action,"firstFind":first_find,"attempt":proof.serial,"staleAdmission":stale,"cancel":{"opened":cancel_open,"draft":cancelled,"action":cancel_action,"returned":cancel_return,"ledger":cancelled_ledger},"actions":{"open":open_action,"input":input_action,"confirm":confirm_action,"conflict":partial_action,"held":held_action,"leave":leave_action,"observe":observed_action},"retiredCheck":{"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"custodyBefore":old_custody,"setup":setup,"ready":replacement_ready,"reboundFind":rebound,"reboundOpener":rebound_opener,"reboundFocus":rebound_focus,"retained":retained,"readReceipt":retired_reads},"observed":observed,"returnFocus":return_focus,"doneRegistryEmpty":true,"history":history,"stages":proof.stages,"captures":proof.captures,"ledger":ledger}),
    )
}
