//! Real shared deletion worker, explicit independent readback and retired-read proof.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
use rename_fixture::field_delete as adapter;
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
                .write_all(format!("slack-list-field-delete-check: {receipt}\n").as_bytes())
                .expect("Deletion terminal output failed");
        })
        .await
        .expect("Deletion terminal writer failed");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if rename::presentation(&state.peek()).is_some_and(|p| p.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Deletion did not reach {want:?}"))
}
async fn pending(state: Signal<ShellState>, key: &str) -> Result<(), String> {
    for _ in 0..150 {
        if rename_fixture::snapshot(&state.peek())?[key] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Missing held deletion {key}"))
}
async fn completed(state: Signal<ShellState>, count: usize) -> Result<Value, String> {
    for _ in 0..150 {
        let l = rename_fixture::snapshot(&state.peek())?;
        match l["finished"].as_array().map(Vec::len) {
            Some(n) if n == count => return Ok(l),
            Some(n) if n < count => {}
            _ => return Err("Deletion completion count exceeded".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Deletion completion did not settle".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("Missing deletion document".into())
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("Missing deletion population".into())
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
    let p = rename::presentation(&s).ok_or("Missing deletion presentation")?;
    let deletion = p.deletion.ok_or("Wrong editor")?;
    Ok(
        json!({"serial":p.serial,"visit":crate::slack_lists::generation(&s),"selection":rename::selection(&s),"column":deletion.column,"choices":deletion.choices,"stage":format!("{:?}",p.stage),"canConfirm":p.can_confirm}),
    )
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listFieldDelete.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
}
async fn choose(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    column: &str,
) -> Result<Value, String> {
    let before = draft(state)?;
    let action = checked(
        d,
        &format!("window.__listFieldDelete.choose({})", json!(column)),
    )
    .await?;
    for _ in 0..100 {
        let next = draft(state)?;
        if next["column"] == column && next["serial"] != before["serial"] {
            let dom = wait_dom(d, &format!("window.__listFieldDelete.sameDraft({next})")).await?;
            if draft(state)? != next {
                return Err("Deletion choice changed during binding".into());
            }
            return Ok(json!({"before":before,"after":next,"action":action,"dom":dom}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Deletion choice event did not settle".into())
}
async fn open(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    checked(d, "window.__listFieldDelete.rememberDelete()").await?;
    let action = click(d, "slack-list-delete-field").await?;
    phase(state, Stage::Editing).await?;
    let model = draft(state)?;
    let dom = wait_dom(d, &format!("window.__listFieldDelete.sameDraft({model})")).await?;
    Ok(json!({"model":model,"dom":dom,"action":action}))
}
async fn done(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    click(d, "slack-list-rename-close").await?;
    wait_dom(d, "window.__listFieldDelete.returnedDelete()").await
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("field_delete.js")).await?;
    wait_dom(d,"({ok:!!document.getElementById('slack-list-delete-field')&&!document.getElementById('slack-list-rename')})").await?;
    if find(state)? != json!({"input":"","applied":"","revision":0}) {
        return Err("Unexpected initial deletion Find".into());
    }
    checked(d, "window.__listFieldDelete.query()").await?;
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
        return Err("Deletion Find debounce did not apply".into());
    }
    let first_find = checked(d, "window.__listFieldDelete.bindFind()").await?;
    let before = document(state)?;
    let raw = population(state)?;
    if before["user"] != "U_OWNER"
        || before["access"] != "write"
        || raw["records"].as_array().map(Vec::len) != Some(73)
        || raw["records"][11]["parent_record_id"] != "R_LIST_010"
        || raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong deletion raw fixture".into());
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
    let canceled_choice = choose(state, d, "C_HIDDEN").await?;
    let cancel_focus = done(d).await?;
    let cancel_ledger = rename_fixture::snapshot(&state.peek())?;
    if cancel_ledger["requests"] != json!([])
        || cancel_ledger["responses"] != json!([])
        || cancel_ledger["finished"] != json!([])
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Cancel retained deletion or sent a request".into());
    }
    proof.stable(state, 0)?;
    let opened = open(state, d).await?;
    let absent = choose(state, d, "C_ABSENT").await?;
    let chosen = choose(state, d, "C_HIDDEN").await?;
    proof.serial = chosen["after"]["serial"].as_u64().ok_or("Missing serial")?;
    let stale = adapter::refuse_old_confirmation(
        &mut state.write(),
        absent["after"]["serial"].as_u64().unwrap(),
    )?;
    let permission = adapter::policy_refusals(&mut state.write())?;
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
    oracle::exact(&held, 10, 2, false, true, 1, &proof.docs, proof.serial)?;
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
    proof.stable(state, 0)?;
    let new_selection = rename::selection(&state.peek()).unwrap();
    if old_visit == new_visit
        || old_selection == new_selection
        || rename_fixture::snapshot(&state.peek())? != held
    {
        return Err("Replacement deletion owner/ledger drift".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&document.querySelector('.slack-list-find input')?.value==='Launch item 0'}})",new_visit)).await?;
    let rebound = checked(d, "window.__listFieldDelete.bindFind()").await?;
    let rebound_opener = checked(d, "window.__listFieldDelete.rememberDelete()").await?;
    let rebound_focus = wait_dom(d, "window.__listFieldDelete.rememberBusy()").await?;
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
        "population",
        "intent",
        "account",
    ] {
        if retained[key] != old_custody[key] {
            return Err("Retired completion replaced deletion custody".into());
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
        return Err("Done retained deletion intent".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    oracle::exact(&ledger, 14, 4, false, false, 1, &proof.docs, proof.serial)?;
    let history = checked(d, "window.__listFieldDelete.history()").await?;
    proof.stable(state, 1)?;
    if rename_fixture::snapshot(&state.peek())? != ledger
        || !adapter::registry_empty(&state.peek())?
    {
        return Err("Deletion changed after final history witness".into());
    }
    Ok(
        json!({"ok":true,"schema":1,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"noSession":true,"noTransport":true,"documents":proof.docs,"population":proof.population,"find":proof.find,"firstFind":first_find,"attempt":proof.serial,"cancel":{"open":canceled_open,"choice":canceled_choice,"focus":cancel_focus,"ledger":cancel_ledger,"registryEmpty":true},"open":opened,"choices":{"absent":absent,"hidden":chosen},"staleConfirmation":stale,"permission":permission,"actions":{"confirm":confirm_action,"conflict":conflict_action,"held":held_action,"leave":leave_action,"observe":observed_action},"retiredCheck":{"oldVisit":old_visit,"newVisit":new_visit,"oldSelection":old_selection,"newSelection":new_selection,"oldEpoch":old_epoch,"newEpoch":state.peek().core.account_epoch,"held":held,"custodyBefore":old_custody,"setup":setup,"reboundFind":rebound,"reboundOpener":rebound_opener,"reboundFocus":rebound_focus,"retained":retained,"readReceipt":retired_reads},"observed":observed,"returnFocus":return_focus,"doneRegistryEmpty":true,"history":history,"stages":proof.stages,"captures":proof.captures,"ledger":ledger}),
    )
}
