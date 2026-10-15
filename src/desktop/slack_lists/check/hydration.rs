//! Real mounted detail, held raw info responses and production merge guards.
use super::*;
use crate::slack_lists::{fixture, record_detail::hydration_fixture};
mod harness;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !hydration_fixture::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("slack-list-hydration-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    hydration_fixture::begin(&state.peek())?;
    checked(desktop, include_str!("hydration.js")).await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.slack-list-grid tbody tr').length===2&&!!document.querySelector('.slack-list-warning')?.textContent.includes('Only part')})"#).await?;
    let initial = population(state)?;
    let mut geometry = Vec::new();
    let mut captures = serde_json::Map::new();
    stage(desktop, "initial", &mut geometry, &mut captures).await?;
    open(desktop).await?;
    wait_requests(state, 1).await?;
    record_detail::wait_record_opens(state, 1).await?;
    let first_open = detail_generation(state)?;
    stage(desktop, "pending", &mut geometry, &mut captures).await?;
    checked(
        desktop,
        "window.__listHydration.click('#slack-list-record-delete')",
    )
    .await?;
    wait_dom(desktop,r#"({ok:!!document.getElementById('slack-list-delete-confirm')&&document.getElementById('slack-list-delete-confirm').textContent.includes('any associated subtasks')})"#).await?;
    stage(desktop, "confirmation", &mut geometry, &mut captures).await?;
    checked(
        desktop,
        "window.__listHydration.click('#slack-list-delete-cancel')",
    )
    .await?;
    wait_dom(
        desktop,
        "({ok:!document.getElementById('slack-list-delete-confirm')})",
    )
    .await?;
    hydration_fixture::release(&state.peek(), 0)?;
    wait_decoded(state, 0).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-record-title')?.textContent==='Hydrated project parent'})"#).await?;
    let first = population(state)?;
    verify_population(&initial, &first, &["R_HYDRATE_CHILD"])?;
    stage(desktop, "detail", &mut geometry, &mut captures).await?;
    back(desktop).await?;
    expand(desktop).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('tr[data-row-id="R_HYDRATE_CHILD"]')?.getAttribute('aria-level')==='2'})"#).await?;
    stage(desktop, "children", &mut geometry, &mut captures).await?;
    // Second response is held across actual Back and a new mounted opening.
    open(desktop).await?;
    wait_requests(state, 2).await?;
    record_detail::wait_record_opens(state, 2).await?;
    let old_open = detail_generation(state)?;
    back(desktop).await?;
    open(desktop).await?;
    wait_requests(state, 3).await?;
    record_detail::wait_record_opens(state, 3).await?;
    let current_open = detail_generation(state)?;
    if first_open == old_open || old_open == current_open {
        return Err("Detail mount did not retire ownership".into());
    }
    let before_late = population(state)?;
    hydration_fixture::release(&state.peek(), 1)?;
    wait_decoded(state, 1).await?;
    // Observe the actual finish result before checking the unchanged population.
    // The extra paint barrier synchronizes visual capture only.
    paint(desktop).await?;
    if population(state)? != before_late || detail_generation(state)? != current_open {
        return Err("Late old detail response changed current population or visit".into());
    }
    checked(desktop,r#"({ok:!document.querySelector('.slack-list-dialog').textContent.includes('STALE')&&document.getElementById('slack-list-record-title')?.textContent==='Hydrated project parent'})"#).await?;
    stage(desktop, "stale", &mut geometry, &mut captures).await?;
    hydration_fixture::release(&state.peek(), 2)?;
    wait_decoded(state, 2).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-record-title')?.textContent==='Hydrated project parent again'})"#).await?;
    let final_page = population(state)?;
    verify_population(
        &initial,
        &final_page,
        &["R_HYDRATE_CHILD", "R_HYDRATE_SECOND"],
    )?;
    if final_page["records"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "R_HYDRATE_STALE")
    {
        return Err("Stale child adopted".into());
    }
    back(desktop).await?;
    expand(desktop).await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('tr[data-row-id="R_HYDRATE_CHILD"]')&&!!document.querySelector('tr[data-row-id="R_HYDRATE_SECOND"]')&&document.querySelectorAll('.slack-list-grid tbody tr').length===4})"#).await?;
    stage(desktop, "final", &mut geometry, &mut captures).await?;
    record_detail::assert_record_opens(state, 3)?;
    let ledger = hydration_fixture::snapshot(&state.peek())?;
    if ledger["attempts"].as_array().map(Vec::len) != Some(3)
        || ledger["decoded"] != json!([true, true, true])
        || ledger["applied"] != json!([true, false, true])
        || ledger["released"] != json!([true, true, true])
        || fixture::attempt_count(&state.peek()) != 0
        || fixture::creation_attempts(&state.peek()) != 0
        || crate::slack_lists::delete::fixture::attempts(&state.peek()) != 0
        || crate::slack_lists::delete::fixture::restore_attempts(&state.peek()) != 0
    {
        return Err("Unexpected fixture request ledger".into());
    }
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],
        "preparedRequests":3,"noTransport":state.peek().core.transport.is_none(),"noSession":state.peek().core.session.is_none(),
        "syntheticInput":true,"mountedDetail":true,"actualBuilderDecoderMerge":true,"backRevealsChildren":true,
        "partialPopulationPreserved":true,"deleteConfirmationRequired":true,"duplicateBlocked":true,
        "lateCloseReopenRejected":true,"heldCurrentMerged":true,"absentChildRetained":true,"noMutationReplay":true,
        "recordOpenRequests":3,"mutationRequests":0,"applied":ledger["applied"],"attempts":ledger["attempts"],"responses":ledger["responses"],
        "initialPopulation":initial,"afterFirst":first,"beforeLate":before_late,"finalPopulation":final_page,
        "openGenerations":[first_open,old_open,current_open],"geometry":geometry,"captures":captures,
        "viewport":eval(desktop,"[innerWidth,innerHeight]").await?}),
    )
}
fn population(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .ok_or("Missing List")?;
    if loaded.records.complete() {
        return Err("Hydration falsely completed List".into());
    }
    Ok(loaded.records.raw.clone())
}
fn detail_generation(state: Signal<ShellState>) -> Result<u64, String> {
    crate::slack_lists::presentation(&state.peek())
        .and_then(|v| v.detail.map(|d| d.open_generation))
        .ok_or("Missing mounted detail".into())
}
fn verify_population(initial: &Value, after: &Value, children: &[&str]) -> Result<(), String> {
    if after["response_metadata"] != initial["response_metadata"]
        || after["records"].as_array().map(Vec::len) != Some(2 + children.len())
    {
        return Err("Population cursor/count changed unexpectedly".into());
    }
    let rows = after["records"].as_array().unwrap();
    if rows.iter().find(|r| r["id"] == "R_HYDRATE_OTHER")
        != initial["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "R_HYDRATE_OTHER")
    {
        return Err("Unrelated record changed".into());
    }
    for child in children {
        if !rows.iter().any(|r| {
            r["id"] == *child
                && r["parent_record_id"] == "R_HYDRATE_PARENT"
                && r["opaque"]["keep"] == true
        }) {
            return Err("Missing positive child metadata".into());
        }
    }
    Ok(())
}
async fn open(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,r#"window.__listHydration.click('tr[data-row-id="R_HYDRATE_PARENT"] .slack-list-record-open')"#).await?;
    wait_dom(d,r#"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_HYDRATE_PARENT'&&document.getElementById('slack-list-record-duplicate')?.disabled===true&&document.activeElement?.id==='slack-list-record-back'})"#).await?;
    Ok(())
}
async fn back(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d, "window.__listHydration.click('#slack-list-record-back')").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-record-detail')})",
    )
    .await?;
    Ok(())
}
async fn expand(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,r#"(()=>{const row=document.querySelector('tr[data-row-id="R_HYDRATE_PARENT"]');return row?.getAttribute('aria-expanded')==='true'?{ok:true}:window.__listHydration.click('tr[data-row-id="R_HYDRATE_PARENT"] .slack-list-tree-toggle');})()"#).await?;
    Ok(())
}
async fn wait_requests(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..60 {
        if hydration_fixture::snapshot(&state.peek())?["attempts"]
            .as_array()
            .map(Vec::len)
            == Some(count)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Hydration request count did not settle".into())
}
async fn wait_decoded(state: Signal<ShellState>, index: usize) -> Result<(), String> {
    for _ in 0..60 {
        let ledger = hydration_fixture::snapshot(&state.peek())?;
        if ledger["decoded"][index] == true && ledger["applied"][index].is_boolean() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Held response did not decode".into())
}
async fn paint(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{window.__hydratePaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__hydratePaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__hydratePaint===true})").await?;
    Ok(())
}
async fn stage(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    geometry: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    geometry.push(
        checked(
            d,
            &format!("window.__listHydration.geometry({})", json!(name)),
        )
        .await?,
    );
    captures.insert(name.into(), harness::capture(d, name).await?);
    Ok(())
}
