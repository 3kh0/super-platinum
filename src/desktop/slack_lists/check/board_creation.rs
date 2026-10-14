//! Native Board lane Add proof, isolated from the larger List interaction sweep.
use super::*;
use crate::slack_lists::fixture;
mod harness;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::board_creation::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("slack-list-board-add-check: {result}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    if !fixture::enabled(&state.peek()) {
        return Err("Disconnected Board fixture required".into());
    }
    checked(desktop, include_str!("board_creation.js")).await?;
    ready(desktop, "OPT_PROGRESS").await?;
    checked(desktop,r#"(()=>{const lanes=[...document.querySelectorAll('.slack-list-lane')];const progress=lanes.find(l=>l.dataset.groupKey==='OPT_PROGRESS'),empty=lanes.find(l=>l.dataset.groupKey==='');return {ok:progress?.querySelectorAll('.slack-list-card').length===1&&empty?.querySelectorAll('.slack-list-card').length===1&&!!empty.querySelector('[data-row-id="R_BOARD_UNASSIGNED"]')};})()"#).await?;
    let viewer = state.peek().self_account.user_id.clone();
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    geometry.push(layout(desktop, "OPT_PROGRESS").await?);
    captures.insert(
        "initial".into(),
        harness::capture(desktop, "initial").await?,
    );

    click_add(desktop, "OPT_PROGRESS").await?;
    pending(state, desktop, 1).await?;
    // Actual disabled controls receive a second synthetic click; no new attempt.
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-lane-add[data-lane-key="OPT_PROGRESS"]').click();document.getElementById('slack-list-add-item').click();return {ok:true};})()"#).await?;
    attempts(state, 1)?;
    geometry.push(wait_dom(desktop, "window.__boardAdd.geometry('pending','')").await?);
    captures.insert(
        "populated-pending".into(),
        harness::capture(desktop, "populated-pending").await?,
    );
    complete(
        state,
        desktop,
        0,
        &fixture::board_creation::expected_fields(&viewer, false, false),
    )
    .await?;
    geometry.push(detail_geometry(desktop).await?);
    captures.insert(
        "populated-result".into(),
        harness::capture(desktop, "populated-result").await?,
    );
    back(desktop).await?;

    ready(desktop, "").await?;
    click_add(desktop, "").await?;
    pending(state, desktop, 2).await?;
    complete(
        state,
        desktop,
        1,
        &fixture::board_creation::expected_fields(&viewer, false, true),
    )
    .await?;
    geometry.push(detail_geometry(desktop).await?);
    captures.insert(
        "empty-result".into(),
        harness::capture(desktop, "empty-result").await?,
    );
    back(desktop).await?;

    choose(desktop, fixture::board_creation::CHECK_VIEW).await?;
    ready(desktop, "").await?;
    geometry.push(layout(desktop, "").await?);
    click_add(desktop, "").await?;
    pending(state, desktop, 3).await?;
    complete(
        state,
        desktop,
        2,
        &fixture::board_creation::expected_fields(&viewer, true, true),
    )
    .await?;
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-record-detail [data-column-id="C_CHECK"] input[type=checkbox]')?.checked===false})"#).await?;
    geometry.push(detail_geometry(desktop).await?);
    captures.insert(
        "unchecked-result".into(),
        harness::capture(desktop, "unchecked-result").await?,
    );
    back(desktop).await?;

    // The canonical backend changes without rewriting the visible source.
    fixture::board_creation::change_backend(&mut state.write(), true);
    click_add(desktop, "").await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-error')?.textContent&&document.querySelector('.slack-list-lane-add[data-lane-key=""]')?.disabled===false&&!document.getElementById('slack-list-create-status')?.textContent.includes('Adding item')})"#).await?;
    attempts(state, 3)?;
    if fixture::board_creation::requests(&state.peek()).len() != 3 {
        return Err("Stale Board source prepared a request".into());
    }
    geometry.push(layout(desktop, "").await?);
    captures.insert(
        "stale-refused".into(),
        harness::capture(desktop, "stale-refused").await?,
    );
    fixture::board_creation::change_backend(&mut state.write(), false);
    crate::slack_lists::refresh(state);
    ready(desktop, "").await?;

    fixture::set_creation_uncertain(&mut state.write());
    click_add(desktop, "").await?;
    pending(state, desktop, 4).await?;
    fixture::complete_creation(&mut state.write()).ok_or("Missing uncertain fixture write")?;
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Could not confirm whether')&&document.getElementById('slack-list-add-item')?.disabled&&[...document.querySelectorAll('.slack-list-lane-add')].every(b=>b.disabled)&&!document.getElementById('slack-list-record-detail')})"#).await?;
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-lane-add[data-lane-key=""]').click();document.getElementById('slack-list-add-item').click();return {ok:true};})()"#).await?;
    crate::slack_lists::refresh(state);
    attempts(state, 4)?;
    geometry.push(wait_dom(desktop, "window.__boardAdd.geometry('uncertain','')").await?);
    captures.insert(
        "uncertain".into(),
        harness::capture(desktop, "uncertain").await?,
    );
    let requests = fixture::board_creation::requests(&state.peek());
    if requests.len() != 4 || !fixture::creation_prepared_valid(&state.peek()) {
        return Err("Board request ledger mismatch".into());
    }
    for (index, (checkbox, empty)) in [(false, false), (false, true), (true, true), (true, true)]
        .into_iter()
        .enumerate()
    {
        if !fixture::board_creation::validate_request(
            &requests[index],
            &fixture::board_creation::expected_fields(&viewer, checkbox, empty),
        ) {
            return Err(format!(
                "Board request {index} changed typed seeds or wire shape"
            ));
        }
    }
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Fixture isolation changed".into());
    }
    let viewport =
        checked(desktop, "({ok:true,viewport:[innerWidth,innerHeight]})").await?["viewport"]
            .clone();
    Ok(
        json!({"ok":true,"fixture":owner["fixture"],"nonce":owner["nonce"],"pid":owner["pid"],"viewport":viewport,
        "preparedRequests":4,"attempts":requests,"viewer":viewer,"captures":captures,"geometry":geometry,
        "noTransport":true,"noSession":true,"syntheticInput":true,"populatedLane":true,"emptySelectLane":true,"unassignedLaneExisted":true,
        "uncheckedLane":true,"firstFilterAndViewer":true,"orderedTypedSeeds":true,"emptyPosition":true,
        "noInventedRouting":true,"pendingNoDuplicate":true,"canonicalResultDetails":true,
        "staleCanonicalRefused":true,"uncertainNoReplay":true,"hitTestedControls":true,"boundedLayout":true}),
    )
}

async fn ready(d: &dioxus::desktop::DesktopContext, lane: &str) -> Result<(), String> {
    wait_dom(d, &format!("window.__boardAdd.ready({})", json!(lane))).await?;
    Ok(())
}
async fn layout(d: &dioxus::desktop::DesktopContext, lane: &str) -> Result<Value, String> {
    checked(d, &format!("window.__boardAdd.reveal({})", json!(lane))).await?;
    wait_dom(
        d,
        &format!("window.__boardAdd.geometry('lane',{})", json!(lane)),
    )
    .await
}
async fn click_add(d: &dioxus::desktop::DesktopContext, lane: &str) -> Result<(), String> {
    layout(d, lane).await?;
    checked(d, &format!("window.__boardAdd.click({})", json!(lane))).await?;
    Ok(())
}
async fn pending(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    count: usize,
) -> Result<(), String> {
    wait_dom(d,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Adding item')&&document.getElementById('slack-list-add-item')?.disabled&&[...document.querySelectorAll('.slack-list-lane-add')].every(b=>b.disabled)})"#).await?;
    for _ in 0..40 {
        if fixture::creation_attempts(&state.peek()) == count {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    attempts(state, count)
}
fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let actual = fixture::creation_attempts(&state.peek());
    if actual == count {
        Ok(())
    } else {
        Err(format!("Expected {count} Board requests, got {actual}"))
    }
}
async fn complete(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    index: usize,
    expected: &Value,
) -> Result<(), String> {
    let requests = fixture::board_creation::requests(&state.peek());
    if !requests
        .get(index)
        .is_some_and(|r| fixture::board_creation::validate_request(r, expected))
    {
        return Err("Prepared Board request did not match exact oracle".into());
    }
    let id =
        fixture::complete_creation(&mut state.write()).ok_or("Missing Board pending creation")?;
    crate::slack_lists::refresh(state);
    wait_dom(d,&format!(r#"({{ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==={}&&document.activeElement===document.getElementById('slack-list-record-back')}})"#,json!(id))).await?;
    let record =
        fixture::created_record(&state.peek(), &id).ok_or("Missing canonical created record")?;
    let mut actual = record["fields"].clone();
    for field in actual.as_array_mut().ok_or("Malformed created fields")? {
        field
            .as_object_mut()
            .ok_or("Malformed created cell")?
            .remove("key");
    }
    if actual != *expected || record["position"] != "" || record.get("parent_record_id").is_some() {
        return Err("Canonical record lost typed seeds/position".into());
    }
    Ok(())
}
async fn detail_geometry(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait_dom(d, "window.__boardAdd.geometry('detail','')").await
}
async fn back(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    detail_geometry(d).await?;
    checked(
        d,
        "(()=>{document.getElementById('slack-list-record-back').click();return {ok:true};})()",
    )
    .await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-record-detail')})",
    )
    .await?;
    Ok(())
}
async fn choose(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before=checked(d,&format!("(()=>{{const e=document.getElementById('slack-list-view-select'),old=e.closest('.slack-list-dialog').dataset.layoutGeneration;e.value={};e.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,old}};}})()",json!(id))).await?;
    wait_dom(d,&format!("({{ok:document.getElementById('slack-list-view-select')?.value==={}&&document.querySelector('.slack-list-dialog')?.dataset.layoutGeneration!=={}}})",json!(id),before["old"])).await?;
    Ok(())
}
