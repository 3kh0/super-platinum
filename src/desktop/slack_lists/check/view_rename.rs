//! Mounted name input/confirmation and held original responses, no live writes.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !rename_fixture::view_check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e,
                "failedLedger":rename_fixture::snapshot(&state.peek()).ok(),
                "failedDocument":document(state).ok(),"failedPopulation":population(state).ok()}),
        };
        eprintln!("slack-list-view-rename-check: {receipt}");
    });
}
async fn phase(state: Signal<ShellState>, want: Stage) -> Result<(), String> {
    for _ in 0..100 {
        if rename::presentation(&state.peek()).is_some_and(|v| v.stage == want) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Rename did not reach {want:?}"))
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("No loaded List".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("No loaded List".into())
}
fn layout(state: Signal<ShellState>) -> Result<&'static str, String> {
    let p = crate::slack_lists::presentation(&state.peek()).ok_or("Missing projection")?;
    if p.view_notice.is_some()
        || p.selected_view.as_deref() != Some("V_CUSTOM")
        || p.total_rows != 2
    {
        return Err("Saved view projection fell back or lost rows".into());
    }
    Ok(match p.layout_kind {
        crate::slack_lists::ProjectionLayout::Table => "table",
        crate::slack_lists::ProjectionLayout::Board => "board",
    })
}
async fn stage(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    geometry: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    geometry.push(wait_dom(d, &format!("window.__listRename.geometry({})", json!(name))).await?);
    captures.insert(name.into(), harness::capture(d, name).await?);
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    if !rename_fixture::snapshot(&state.peek())?["requests"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err("Used fixture ledger".into());
    }
    checked(d, include_str!("view_rename.js")).await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('#slack-list-rename-view')})",
    )
    .await?;
    if rename::selected_view(&state.peek()) != Some("V_CUSTOM") {
        return Err("Initial selected view mismatch".into());
    }
    let initial = population(state)?;
    let before = document(state)?;
    let initial_layout = layout(state)?;
    if before["list_metadata"]["views"][0]["type"] != initial_layout {
        return Err("Wrong initial projection kind".into());
    }
    let mut geometry = Vec::new();
    let mut captures = serde_json::Map::new();
    stage(d, "initial", &mut geometry, &mut captures).await?;
    checked(d, "window.__listRename.click('#slack-list-rename-view')").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-name')===document.activeElement})",
    )
    .await?;
    checked(d, "window.__listRename.input('Delivery overview')").await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-rename-name')?.value==='Delivery overview'&&!document.getElementById('slack-list-rename-confirm')?.disabled})").await?;
    stage(d, "editing", &mut geometry, &mut captures).await?;
    checked(d, "window.__listRename.click('#slack-list-rename-confirm')").await?;
    phase(state, Stage::Saving).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename')?.dataset.renameStage==='Saving'})",
    )
    .await?;
    if document(state)? != before || population(state)? != initial {
        return Err("Optimistic schema/population change".into());
    }
    stage(d, "saving", &mut geometry, &mut captures).await?;
    rename_fixture::release(&state.peek(), false)?;
    phase(state, Stage::Uncertain).await?;
    if document(state)? != before {
        return Err("Lost acknowledgement projected".into());
    }
    wait_dom(
        d,
        "({ok:!!document.getElementById('slack-list-rename-check')})",
    )
    .await?;
    stage(d, "uncertain", &mut geometry, &mut captures).await?;
    rename_fixture::hold_read(&state.peek())?;
    checked(d, "window.__listRename.click('#slack-list-rename-check')").await?;
    phase(state, Stage::Checking).await?;
    for _ in 0..100 {
        if rename_fixture::snapshot(&state.peek())?["readPending"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    if rename_fixture::snapshot(&state.peek())?["readPending"] != true {
        return Err("No held read".into());
    }
    stage(d, "checking", &mut geometry, &mut captures).await?;
    let old_visit = crate::slack_lists::generation(&state.peek()).unwrap();
    checked(d, "window.__listRename.closeList()").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    // Retire the original account epoch before returning to a new mounted visit.
    state.write().core.account_epoch += 1;
    let new_visit = rename_fixture::reopen(&mut state.write())?;
    if old_visit == new_visit {
        return Err("Reopen did not replace visit".into());
    }
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'}})",new_visit)).await?;
    rename_fixture::release(&state.peek(), true)?;
    phase(state, Stage::Uncertain).await?;
    // The obsolete read releases its own retained job but cannot populate this visit.
    crate::slack_lists::fixture::refresh(&mut state.write());
    wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'&&!!document.getElementById('slack-list-rename-check')}})",new_visit)).await?;
    if document(state)? != before
        || population(state)? != initial
        || rename::selected_view(&state.peek()) != Some("V_CUSTOM")
    {
        return Err("Retired read changed schema/population/selection".into());
    }
    stage(d, "stale", &mut geometry, &mut captures).await?;
    checked(d, "window.__listRename.click('#slack-list-rename-check')").await?;
    phase(state, Stage::Observed).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename')?.dataset.renameStage==='Observed'})",
    )
    .await?;
    let after = document(state)?;
    let mut expected = before.clone();
    expected["list_metadata"]["views"][0]["name"] = json!("Delivery overview");
    if after != expected
        || population(state)? != initial
        || rename::selected_view(&state.peek()) != Some("V_CUSTOM")
    {
        return Err("Readback did not preserve exact schema/population".into());
    }
    stage(d, "observed", &mut geometry, &mut captures).await?;
    checked(d, "window.__listRename.click('#slack-list-rename-close')").await?;
    wait_dom(d,r#"(()=>{const p=document.getElementById('slack-list-rename'),h=document.getElementById('slack-list-rename-view'),v=document.getElementById('slack-list-view-select'),a=document.activeElement;return {ok:!p&&v?.value==='V_CUSTOM'&&v?.selectedOptions[0]?.textContent==='Delivery overview'&&a===h&&a?.dataset.renameView==='V_CUSTOM',overlayPresent:!!p,selected:v?.value,label:v?.selectedOptions[0]?.textContent,active:{id:a?.id,view:a?.dataset.renameView}}})()"#).await?;
    stage(d, "final", &mut geometry, &mut captures).await?;
    let final_layout = layout(state)?;
    if initial_layout != final_layout {
        return Err("Rename changed projection kind".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if ledger["requests"].as_array().unwrap().len() != 8
        || ledger["finished"].as_array().unwrap().len() != 3
    {
        return Err("Unexpected request/completion count".into());
    }
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],"preparedRequests":8,
        "noTransport":state.peek().core.transport.is_none(),"noSession":state.peek().core.session.is_none(),"syntheticInput":true,
        "actualBuilderDecoderReducers":true,"explicitConfirmation":true,"lostAckRetained":true,"lateReadRejected":true,
        "exactReadback":true,"selectedViewPreserved":true,"selectedView":"V_CUSTOM","layoutKinds":[initial_layout,final_layout],"partialPopulationPreserved":true,"noMutationReplay":true,"writeRequests":1,"readRequests":7,
        "before":before,"after":after,"initialPopulation":initial,"finalPopulation":population(state)?,"ledger":ledger,
        "visitGenerations":[old_visit,new_visit],"geometry":geometry,"captures":captures,"viewport":eval(d,"[innerWidth,innerHeight]").await?}),
    )
}
