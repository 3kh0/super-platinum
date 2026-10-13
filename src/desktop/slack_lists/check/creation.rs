//! One-click header creation with canonical server IDs and no uncertain replay.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let cell_attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_creation_views(&mut state.write())
        .ok_or("Creation checks require the disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_CREATE_FILTER"]')})"#).await?;
    choose(desktop, "V_CREATE_FILTER").await?;
    fixture::set_creation_limit(&mut state.write(), None);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-add-item')?.disabled&&document.getElementById('slack-list-create-help')?.textContent.includes('not been verified')})"#).await?;
    click_add(desktop).await?;
    attempts(state, 0)?;
    fixture::set_creation_limit(&mut state.write(), Some(true));
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-add-item')?.disabled&&document.getElementById('slack-list-create-help')?.textContent.includes('item limit')})"#).await?;
    click_add(desktop).await?;
    attempts(state, 0)?;
    fixture::set_creation_limit(&mut state.write(), Some(false));
    ready(desktop).await?;
    begin(state, desktop, 1).await?;
    let first = complete(state, desktop).await?;
    detail(desktop, &first).await?;
    let record = fixture::created_record(&state.peek(), &first).ok_or("Created record missing")?;
    if record["fields"] != json!([]) {
        return Err("Header create guessed filter or grouping defaults".into());
    }
    checked(desktop,&format!(r#"(()=>{{const panel=document.getElementById('slack-list-record-detail'),rows=[...document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]')];return {{ok:panel.dataset.recordId==={}&&!rows.some(row=>row.dataset.rowId==={})&&document.getElementById('slack-list-create-status')?.textContent.includes('not shown in the current view')&&panel.querySelectorAll('.slack-list-detail-field').length===12&&!!panel.querySelector('[data-column-id="C_NAME"] .slack-list-edit-cell:not(:disabled)')}};}})()"#,json!(first),json!(first))).await?;
    let created = capture(desktop, "list-created").await?;
    close_detail(desktop).await?;

    choose(desktop, "V_CREATE_COMPLETED").await?;
    ready(desktop).await?;
    begin(state, desktop, 2).await?;
    let second = complete(state, desktop).await?;
    detail(desktop, &second).await?;
    let record =
        fixture::created_record(&state.peek(), &second).ok_or("Completed record missing")?;
    if record["fields"]
        != json!([{"column_id":"C_CREATE_DONE","key":"todo_completed","checkbox":true}])
    {
        return Err("Completed-view create lost its captured initial checkbox".into());
    }
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-record-detail [data-column-id="C_CREATE_DONE"] input[type=checkbox]')?.checked===true})"#).await?;
    close_detail(desktop).await?;

    choose(desktop, "V_CREATE_BOARD").await?;
    ready(desktop).await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelector('.slack-list-board')?.dataset.keyboardMounted==='true'})"#,
    )
    .await?;
    begin(state, desktop, 3).await?;
    let third = complete(state, desktop).await?;
    detail(desktop, &third).await?;
    close_detail(desktop).await?;
    ready(desktop).await?;

    fixture::set_creation_uncertain(&mut state.write());
    begin(state, desktop, 4).await?;
    let unknown =
        fixture::complete_creation(&mut state.write()).ok_or("Uncertain fixture create missing")?;
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Could not confirm whether')&&document.getElementById('slack-list-add-item')?.disabled&&!!document.getElementById('slack-list-create-retire')&&!document.getElementById('slack-list-record-detail')})"#).await?;
    if fixture::created_record(&state.peek(), &unknown).is_none() {
        return Err("Uncertain fixture did not apply server creation".into());
    }
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Could not confirm whether')&&document.getElementById('slack-list-add-item')?.disabled&&!document.getElementById('slack-list-create-open')})"#).await?;
    click_add(desktop).await?;
    attempts(state, 4)?;
    let uncertain = capture(desktop, "list-create-uncertain").await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-create-retire').click();return {ok:true};})()"#).await?;
    ready(desktop).await?;
    attempts(state, 4)?;
    if !fixture::creation_prepared_valid(&state.peek()) {
        return Err("Creation request shape or retry policy did not match capture".into());
    }
    choose(desktop, "").await?;
    fixture::restore_creation_views(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_CREATE_FILTER"]')&&!document.getElementById('slack-list-record-detail')&&!document.getElementById('slack-list-create-status')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11})"#).await?;
    assert_attempts(state, cell_attempts)?;
    Ok(
        json!({"unknownLimitBlocked":true,"overLimitBlocked":true,"singleAttempt":true,"filteredDetail":true,"completedSeed":true,"boardCreate":true,"pendingDuplicateBlocked":true,"canonicalReadback":true,"uncertainNoReplay":true,"explicitRetirement":true,"cleanup":true,"captures":{"created":created,"uncertain":uncertain}}),
    )
}
async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before=checked(desktop,&format!(r#"(()=>{{const picker=document.getElementById('slack-list-view-select'),dialog=picker.closest('.slack-list-dialog'),old=dialog.dataset.layoutGeneration;picker.focus();picker.value={};picker.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,old}};}})()"#,json!(id))).await?;
    wait_dom(desktop,&format!(r#"({{ok:document.getElementById('slack-list-view-select')?.value==={}&&document.querySelector('.slack-list-dialog')?.dataset.layoutGeneration!=={}}})"#,json!(id),before["old"])).await?;
    Ok(())
}
async fn ready(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.getElementById('slack-list-add-item')&&!document.getElementById('slack-list-add-item').disabled})"#).await?;
    Ok(())
}
async fn click_add(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{const add=document.getElementById('slack-list-add-item');add.focus();add.click();return {ok:true};})()"#).await?;
    Ok(())
}
async fn begin(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    expected: usize,
) -> Result<(), String> {
    click_add(desktop).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Adding item')&&document.getElementById('slack-list-add-item')?.disabled&&!document.getElementById('slack-list-record-detail')})"#).await?;
    attempts(state, expected)?;
    click_add(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Adding item')&&document.getElementById('slack-list-add-item')?.disabled})"#).await?;
    attempts(state, expected)
}
async fn complete(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<String, String> {
    let id = fixture::complete_creation(&mut state.write()).ok_or("No pending fixture creation")?;
    crate::slack_lists::refresh(state);
    detail(desktop, &id).await?;
    Ok(id)
}
async fn detail(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const panel=document.getElementById('slack-list-record-detail'),back=document.getElementById('slack-list-record-back');return {{ok:panel?.dataset.recordId==={}&&document.activeElement===back&&panel.querySelectorAll('.slack-list-detail-field[tabindex="0"]').length===1&&document.querySelector('.slack-list-layout')?.inert===true,record:panel?.dataset.recordId,active:document.activeElement?.id}};}})()"#,json!(id))).await?;
    Ok(())
}
async fn close_detail(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(
        desktop,
        r#"(()=>{document.getElementById('slack-list-record-back').click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-record-detail')&&!document.querySelector('.slack-list-layout').hasAttribute('inert')&&!!document.activeElement?.closest('.slack-list-dialog')})"#).await?;
    Ok(())
}
fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = fixture::creation_attempts(&state.peek());
    if actual == expected {
        Ok(())
    } else {
        Err(format!("Expected {expected} create attempts, got {actual}"))
    }
}

async fn capture(
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Option<String>, String> {
    wait_dom(desktop,r#"(()=>{const status=document.getElementById('slack-list-create-status'),surface=document.getElementById('slack-list-record-detail')||document.getElementById('slack-list-create-retire');const visible=e=>{if(!e)return false;const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1;};return {ok:visible(status)&&visible(surface)};})()"#).await?;
    capture_stage(desktop, stage).await
}
