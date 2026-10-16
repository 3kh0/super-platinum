//! Mounted name input/confirmation and held original responses, no live writes.
use super::*;
use crate::slack_lists::rename::{self, Stage, fixture as rename_fixture};
mod harness;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !rename_fixture::create_check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e,
                "failedLedger":rename_fixture::snapshot(&state.peek()).ok(),
                "failedDocument":document(state).ok(),"failedPopulation":population(state).ok()}),
        };
        eprintln!("slack-list-view-create-check: {receipt}");
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
fn layout(state: Signal<ShellState>, expected: &str) -> Result<&'static str, String> {
    let p = crate::slack_lists::presentation(&state.peek()).ok_or("Missing projection")?;
    if p.view_notice.is_some() || p.selected_view.as_deref() != Some(expected) || p.total_rows != 2
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
    checked(d, include_str!("view_create.js")).await?;
    wait_dom(d, "({ok:!!document.querySelector('#slack-list-new-view')})").await?;
    if rename::selected_view(&state.peek()) != Some("V_CUSTOM") {
        return Err("Initial selected view mismatch".into());
    }
    let initial = population(state)?;
    let before = document(state)?;
    let initial_layout = layout(state, "V_CUSTOM")?;
    let narrow = owner["fixture"].as_str().unwrap().ends_with("-narrow");
    if before["list_metadata"]["views"][0]["type"] != initial_layout {
        return Err("Wrong initial projection kind".into());
    }
    let mut geometry = Vec::new();
    let mut captures = serde_json::Map::new();
    stage(d, "initial", &mut geometry, &mut captures).await?;
    draft(state, d, narrow).await?;
    checked(d, "window.__listRename.click('#slack-list-rename-close')").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-rename')&&document.activeElement?.id==='slack-list-new-view'})").await?;
    if !rename_fixture::snapshot(&state.peek())?["requests"]
        .as_array()
        .unwrap()
        .is_empty()
    {
        return Err("Canceled grouping draft issued a request".into());
    }
    draft(state, d, narrow).await?;
    let grouping = if narrow { "status" } else { "done" };
    rename_fixture::grouping_schema(&state.peek(), grouping, true)?;
    checked(d, "window.__listRename.click('#slack-list-rename-confirm')").await?;
    phase(state, Stage::NotRequested).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename')?.dataset.renameStage==='NotRequested'})",
    )
    .await?;
    let refused = rename_fixture::snapshot(&state.peek())?;
    if refused["requests"].as_array().unwrap().len() != 3
        || refused["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "/api/lists.views.create")
        || document(state)? != before
        || population(state)? != initial
    {
        return Err("Changed grouping schema escaped preflight".into());
    }
    stage(d, "refused", &mut geometry, &mut captures).await?;
    rename_fixture::grouping_schema(&state.peek(), grouping, false)?;
    checked(d, "window.__listRename.click('#slack-list-rename-close')").await?;
    wait_dom(d, "({ok:!document.getElementById('slack-list-rename')})").await?;
    draft(state, d, narrow).await?;
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
    let retained_reply = rename_fixture::retained_reply(&state.peek());
    let source_reply = rename_fixture::snapshot(&state.peek())?["responses"][6].clone();
    if if narrow {
        retained_reply.is_some()
    } else {
        retained_reply.as_ref() != Some(&source_reply)
    } {
        return Err("Original mutation reply provenance was not retained".into());
    }
    if document(state)? != before {
        return Err("Lost acknowledgement projected".into());
    }
    wait_dom(
        d,
        "({ok:!!document.getElementById('slack-list-rename-check')})",
    )
    .await?;
    stage(d, "uncertain", &mut geometry, &mut captures).await?;
    let mut visits = Vec::new();
    if narrow {
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
        visits = vec![old_visit, new_visit];
    }
    checked(d, "window.__listRename.click('#slack-list-rename-check')").await?;
    phase(state, Stage::Available).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename')?.dataset.renameStage==='Available'})",
    )
    .await?;
    let after = document(state)?;
    let mut expected = before.clone();
    crate::slack_lists::fixture::view_create::add(
        &mut expected,
        "Delivery overview",
        if narrow {
            super_platinum_core::slack_lists::ViewLayout::Board
        } else {
            super_platinum_core::slack_lists::ViewLayout::Table
        },
        Some(grouping),
    );
    if after != expected
        || population(state)? != initial
        || rename::selected_view(&state.peek()) != Some("V_CUSTOM")
    {
        return Err("Readback did not preserve exact schema/population".into());
    }
    stage(d, "available", &mut geometry, &mut captures).await?;
    checked(d, "window.__listRename.click('#slack-list-new-view-open')").await?;
    wait_dom(d,r#"(()=>{const p=document.getElementById('slack-list-rename'),v=document.getElementById('slack-list-view-select'),a=document.activeElement;return {ok:!p&&v?.value==='V_SERVER_NEW'&&v?.selectedOptions[0]?.textContent==='Delivery overview'&&a===v,overlayPresent:!!p,selected:v?.value,label:v?.selectedOptions[0]?.textContent,active:a?.id}})()"#).await?;
    stage(d, "final", &mut geometry, &mut captures).await?;
    let group_projection = wait_dom(d, "window.__listRename.groups()").await?;
    let final_layout = layout(state, "V_SERVER_NEW")?;
    if initial_layout != final_layout {
        return Err("Rename changed projection kind".into());
    }
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if ledger["requests"].as_array().unwrap().len() != if narrow { 11 } else { 10 }
        || ledger["finished"].as_array().unwrap().len() != if narrow { 3 } else { 2 }
    {
        return Err("Unexpected request/completion count".into());
    }
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],"preparedRequests":if narrow{11}else{10},
        "noTransport":state.peek().core.transport.is_none(),"noSession":state.peek().core.session.is_none(),"syntheticInput":true,
        "actualBuilderDecoderReducers":true,"explicitConfirmation":true,"writeReplyNotProjected":true,"lateReadRejected":narrow,"positiveReplyRetained":!narrow,"retainedMutationReply":retained_reply,
        "exactReadback":true,"groupingKey":grouping,"groupProjection":group_projection,"groupingCancelNoRequest":true,"groupingSchemaRefused":true,"selectedViewPreservedUntilOpen":true,"selectedView":"V_SERVER_NEW","explicitOpen":true,"layoutKinds":[initial_layout,final_layout],"partialPopulationPreserved":true,"noMutationReplay":true,"writeRequests":1,"readRequests":if narrow{10}else{9},
        "before":before,"after":after,"initialPopulation":initial,"finalPopulation":population(state)?,"ledger":ledger,
        "visitGenerations":visits,"geometry":geometry,"captures":captures,"viewport":eval(d,"[innerWidth,innerHeight]").await?}),
    )
}

async fn draft(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    narrow: bool,
) -> Result<(), String> {
    checked(d, "window.__listRename.click('#slack-list-new-view')").await?;
    phase(state, Stage::Editing).await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-new-view-grouping')?.value===''})",
    )
    .await?;
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-rename-name')===document.activeElement})",
    )
    .await?;
    checked(d, "window.__listRename.input('Delivery overview')").await?;
    checked(
        d,
        &format!(
            "window.__listRename.chooseLayout({})",
            json!(if narrow { "board" } else { "table" })
        ),
    )
    .await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-rename-name')?.value==='Delivery overview'&&!document.getElementById('slack-list-rename-confirm')?.disabled})").await?;
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-rename')?.dataset.viewCreate==={}}})",
            json!(if narrow { "board" } else { "table" })
        ),
    )
    .await?;
    checked(
        d,
        &format!(
            "window.__listRename.chooseGrouping({})",
            json!(if narrow { "status" } else { "done" })
        ),
    )
    .await?;
    wait_dom(
        d,
        &format!(
            "({{ok:document.getElementById('slack-list-new-view-grouping')?.value==={}}})",
            json!(if narrow { "status" } else { "done" })
        ),
    )
    .await?;
    Ok(())
}
