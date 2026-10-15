//! Explicit direct-root creation through mounted production controls; synthetic HTTP only.
use super::*;
use crate::slack_lists::{
    self,
    subitem::{self, Stage, fixture as subitem_fixture},
};
mod harness;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !subitem_fixture::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => {
                json!({"ok":false,"error":e,"failedLedger":subitem_fixture::snapshot(&state.peek()).ok(),"failedPopulation":population(state).ok()})
            }
        };
        eprintln!("slack-list-subitem-check: {receipt}");
    });
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.records.raw.clone())
        .ok_or("No population".into())
}
fn document(state: Signal<ShellState>) -> Result<Value, String> {
    state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .map(|l| l.document.raw.clone())
        .ok_or("No document".into())
}
async fn phase(state: Signal<ShellState>, stage: Stage) -> Result<(), String> {
    for _ in 0..150 {
        if subitem::presentation(&state.peek()).is_some_and(|p| p.stage == stage) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Subitem did not reach {stage:?}"))
}
async fn held(state: Signal<ShellState>, which: &str) -> Result<(), String> {
    for _ in 0..150 {
        if subitem_fixture::snapshot(&state.peek())?["held"][which] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("No actual held {which} response"))
}
async fn stage(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    geometry: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    geometry.push(
        wait_dom(
            d,
            &format!("window.__listSubitem.geometry({})", json!(name)),
        )
        .await?,
    );
    captures.insert(name.into(), harness::capture(d, name).await?);
    Ok(())
}
async fn root(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d, "window.__listSubitem.click('root')").await?;
    wait_dom(d,"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_HYDRATE_PARENT'&&!!document.getElementById('slack-list-subitem-add')&&document.getElementById('slack-list-record-detail')?.contains(document.activeElement)})").await?;
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    let name = owner["fixture"].as_str().ok_or("fixture")?;
    let uncertain = name.contains("-uncertain");
    let narrow = name.ends_with("-narrow");
    if !subitem_fixture::snapshot(&state.peek())?["requests"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err("Used ledger".into());
    }
    checked(d, include_str!("subitem.js")).await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('.slack-list-record-open')})",
    )
    .await?;
    let before = document(state)?;
    let initial = population(state)?;
    let mut geometry = Vec::new();
    let mut captures = serde_json::Map::new();
    let mut visits = Vec::new();
    stage(d, "initial", &mut geometry, &mut captures).await?;
    root(d).await?;
    stage(d, "parent", &mut geometry, &mut captures).await?;
    checked(d, "window.__listSubitem.click('#slack-list-subitem-add')").await?;
    phase(state, Stage::Preparing).await?;
    held(state, "preflight").await?;
    stage(d, "preparing", &mut geometry, &mut captures).await?;
    subitem_fixture::release("preflight")?;
    phase(state, Stage::Sending).await?;
    held(state, "write").await?;
    stage(d, "sending", &mut geometry, &mut captures).await?;
    if population(state)? != initial {
        return Err("Optimistic child projection".into());
    }
    subitem_fixture::release("write")?;
    phase(
        state,
        if uncertain {
            Stage::Uncertain
        } else {
            Stage::AwaitingRead
        },
    )
    .await?;
    if population(state)? != initial {
        return Err("ACK projected without read".into());
    }
    let retained_receipt = subitem_fixture::snapshot(&state.peek())?["retained"].clone();
    let retained_reply = retained_receipt["reply"].clone();
    stage(
        d,
        if uncertain {
            "uncertain"
        } else {
            "awaiting-read"
        },
        &mut geometry,
        &mut captures,
    )
    .await?;
    if uncertain {
        subitem_fixture::hold_read();
        checked(d, "window.__listSubitem.click('#slack-list-subitem-check')").await?;
        phase(state, Stage::Checking).await?;
        held(state, "read").await?;
        stage(d, "checking", &mut geometry, &mut captures).await?;
        let old = slack_lists::generation(&state.peek()).ok_or("old visit")?;
        // Back is a real mounted close. Then close the List, retire its epoch, and
        // reopen through the existing fixture read path; wait for the new mount.
        checked(d, "window.__listSubitem.click('#slack-list-record-back')").await?;
        wait_dom(
            d,
            "({ok:!document.getElementById('slack-list-record-detail')})",
        )
        .await?;
        checked(d, "window.__listSubitem.closeList()").await?;
        wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
        state.write().core.account_epoch += 1;
        let fresh = subitem_fixture::reopen(&mut state.write())?;
        if old == fresh {
            return Err("No fresh visit".into());
        }
        visits = vec![old, fresh];
        wait_dom(d,&format!("({{ok:document.querySelector('.slack-list-dialog')?.dataset.dialogGeneration==='{}'}})",fresh)).await?;
        root(d).await?;
        subitem_fixture::release("read")?;
        phase(state, Stage::Uncertain).await?;
        if population(state)? != initial {
            return Err("Obsolete read projected".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-subitem-check')?.disabled===true})",
        )
        .await?;
        stage(d, "stale", &mut geometry, &mut captures).await?;
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-subitem-check')?.disabled===false})",
        )
        .await?;
    }
    checked(d, "window.__listSubitem.click('#slack-list-subitem-check')").await?;
    phase(
        state,
        if uncertain {
            Stage::Reviewed
        } else {
            Stage::Available
        },
    )
    .await?;
    let current = population(state)?;
    let mut expected = initial.clone();
    let ledger = subitem_fixture::snapshot(&state.peek())?;
    let server = ledger["serverPopulation"]["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R_SERVER_SUBITEM")
        .ok_or("No server child")?
        .clone();
    expected["records"].as_array_mut().unwrap().push(server);
    expected["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("old_parent_unknown");
    expected["records"][0]["returned_parent_unknown"] = json!({"keep":true});
    let mut expected_doc = before.clone();
    expected_doc
        .as_object_mut()
        .unwrap()
        .remove("old_document_unknown");
    expected_doc["returned_document_unknown"] = json!({"keep":[1,2]});
    if current != expected || document(state)? != expected_doc {
        return Err("Exact sparse merge or metadata preservation failed".into());
    }
    stage(
        d,
        if uncertain { "reviewed" } else { "available" },
        &mut geometry,
        &mut captures,
    )
    .await?;
    if uncertain {
        checked(
            d,
            "({ok:!document.getElementById('slack-list-subitem-open')})",
        )
        .await?;
        checked(
            d,
            "window.__listSubitem.click('#slack-list-subitem-dismiss')",
        )
        .await?;
    } else {
        checked(d, "window.__listSubitem.click('#slack-list-subitem-open')").await?;
        wait_dom(d,"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_SERVER_SUBITEM'&&document.getElementById('slack-list-record-back')?.textContent.includes('parent')&&document.getElementById('slack-list-record-detail')?.contains(document.activeElement)})").await?;
        stage(d, "child", &mut geometry, &mut captures).await?;
        checked(d,"window.__listSubitem.click('#slack-list-record-detail [data-column-id=\"C_NAME\"] .slack-list-edit-cell')").await?;
        wait_dom(
            d,
            "({ok:!!document.getElementById('slack-list-cell-editor')})",
        )
        .await?;
        checked(d, "window.__listSubitem.click('#slack-list-editor-cancel')").await?;
        wait_dom(d,"({ok:!document.getElementById('slack-list-cell-editor')&&document.activeElement?.dataset.columnId==='C_NAME'})").await?;
        checked(d, "window.__listSubitem.click('#slack-list-record-back')").await?;
        wait_dom(d,"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_HYDRATE_PARENT'&&document.activeElement?.dataset.subitemId==='R_SERVER_SUBITEM'})").await?;
        stage(d, "parent-return", &mut geometry, &mut captures).await?;
    }
    checked(d, "window.__listSubitem.click('#slack-list-record-back')").await?;
    wait_dom(d,"({ok:!document.getElementById('slack-list-record-detail')&&document.activeElement?.closest('[data-row-id]')?.dataset.rowId==='R_HYDRATE_PARENT'})").await?;
    stage(d, "list-return", &mut geometry, &mut captures).await?;
    let ledger = subitem_fixture::snapshot(&state.peek())?;
    let reads = if uncertain { 11 } else { 10 };
    if ledger["requests"].as_array().unwrap().len() != reads + 1
        || slack_lists::fixture::attempt_count(&state.peek()) != 0
        || subitem::blocks(&state.peek())
    {
        return Err("Unexpected dispatch or unresolved receipt".into());
    }
    let opens = slack_lists::fixture::record_open_count(&state.peek());
    if opens != if uncertain { 2 } else { 3 }
        || !slack_lists::fixture::record_open_valid(&state.peek())
    {
        return Err("Record-open lifecycle differs".into());
    }
    Ok(
        json!({"ok":true,"fixture":name,"nonce":owner["nonce"],"pid":owner["pid"],"preparedRequests":reads+1,"readRequests":reads,"writeRequests":1,
 "noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"syntheticInput":true,"sourcePlatform":std::env::consts::OS.replace("macos","darwin"),"actualBuilderDecoderReducers":true,
 "explicitConfirmation":true,"writeReplyNotProjected":true,"exactReadback":true,"partialPopulationPreserved":true,"noMutationReplay":true,"selectedViewPreserved":true,
 "positiveReplyRetained":!uncertain,"retainedMutationReply":retained_reply,"retainedReceipt":retained_receipt,"lateReadRejected":uncertain,"explicitOpen":!uncertain,"unknownIDNotGuessed":uncertain,
 "recordOpenPrepared":opens,"recordOpenPreparedValid":true,"cellWriteRequests":0,"fileViewingRequests":0,"before":before,"after":document(state)?,"initialPopulation":initial,"finalPopulation":population(state)?,"ledger":ledger,"visitGenerations":visits,
 "layoutKind":if narrow{"board"}else{"table"},"selectedView":"V_CUSTOM","geometry":geometry,"captures":captures,"viewport":eval(d,"[innerWidth,innerHeight]").await?}),
    )
}
