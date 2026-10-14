//! Duplicate exercises the real controller against disconnected canonical data.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let snapshot = fixture::add_duplicate_views(&mut state.write())
        .ok_or("Duplicate checks require the disconnected fixture")?;
    open(desktop).await?;
    let before =
        fixture::created_record(&state.peek(), "R_LIST_002").ok_or("Duplicate source missing")?;
    click(desktop, "slack-list-record-duplicate").await?;
    wait_dom(desktop, r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Duplicating item')&&document.getElementById('slack-list-record-duplicate')?.disabled&&document.getElementById('slack-list-record-delete')?.disabled&&document.getElementById('slack-list-add-item')?.disabled})"#).await?;
    click(desktop, "slack-list-record-duplicate").await?;
    attempts(state, 1)?;
    let id = fixture::complete_creation(&mut state.write()).ok_or("Duplicate request missing")?;
    let created =
        fixture::created_record(&state.peek(), &id).ok_or("Duplicate response missing")?;
    if created["fields"] != before["fields"]
        || created["id"] == before["id"]
        || created["position"]
            .as_str()
            .is_none_or(|position| position <= "0000000300" || position >= "0000000400")
        || created.get("parent_record_id").is_some()
    {
        return Err("Duplicate lost server fields, identity or canonical adjacent position".into());
    }
    crate::slack_lists::refresh(state);
    wait_dom(desktop, &format!(r#"({{ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==={}&&document.getElementById('slack-list-create-status')?.textContent.includes('Item duplicated.')}})"#, json!(id))).await?;
    let geometry = geometry(desktop).await?;
    let duplicated = capture_stage(desktop, "list-duplicated").await?;
    click(desktop, "slack-list-record-back").await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-record-detail')})"#,
    )
    .await?;
    open(desktop).await?;
    fixture::set_creation_uncertain(&mut state.write());
    click(desktop, "slack-list-record-duplicate").await?;
    wait_dom(desktop, r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Duplicating item')})"#).await?;
    attempts(state, 2)?;
    fixture::complete_creation(&mut state.write()).ok_or("Uncertain duplicate missing")?;
    crate::slack_lists::refresh(state);
    wait_dom(desktop, r#"({ok:document.getElementById('slack-list-create-status')?.textContent.includes('Could not confirm whether the item was duplicated')&&document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_002'&&document.getElementById('slack-list-record-duplicate')?.disabled&&document.getElementById('slack-list-record-delete')?.disabled&&!!document.getElementById('slack-list-create-retire')})"#).await?;
    click(desktop, "slack-list-record-duplicate").await?;
    crate::slack_lists::refresh(state);
    attempts(state, 2)?;
    let uncertain = capture_stage(desktop, "list-duplicate-uncertain").await?;
    click(desktop, "slack-list-record-back").await?;
    wait_dom(desktop, r#"({ok:!document.getElementById('slack-list-record-detail')&&document.getElementById('slack-list-add-item')?.disabled})"#).await?;
    click(desktop, "slack-list-create-retire").await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-add-item')?.disabled})"#,
    )
    .await?;
    if !fixture::creation_prepared_valid(&state.peek()) {
        return Err("Duplicate request shape or retry policy did not match capture".into());
    }
    attempts(state, 2)?;
    fixture::restore_creation_views(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_CREATE_FILTER"]')&&!document.getElementById('slack-list-record-detail')&&!document.getElementById('slack-list-create-status')})"#).await?;
    Ok(
        json!({"canonicalAdjacentPosition":true,"serverOwnedFields":true,"newServerIdentity":true,"singleAttempt":true,"pendingMutationLocks":true,"canonicalReadback":true,"uncertainNoReplay":true,"explicitRetirement":true,"noTransport":true,"cleanup":true,"preparedRequests":2,"geometry":geometry,"captures":{"duplicated":duplicated,"uncertain":uncertain}}),
    )
}

async fn open(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] .slack-list-record-open')})"#).await?;
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] .slack-list-record-open').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_002'&&!document.getElementById('slack-list-record-duplicate')?.disabled})"#).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(desktop, &format!(r#"(()=>{{const button=document.getElementById({});if(!button)return {{ok:false}};button.focus();button.click();return {{ok:true}};}})()"#, json!(id))).await?;
    Ok(())
}
fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = fixture::creation_attempts(&state.peek());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Expected {expected} duplicate requests, got {actual}"
        ))
    }
}
async fn geometry(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,r#"(()=>{const ids=['slack-list-record-back','slack-list-record-duplicate','slack-list-record-delete'];const controls=ids.map(id=>document.getElementById(id));const visible=controls.every(e=>{if(!e)return false;const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1&&(hit===e||e.contains(hit));});const scroll=document.querySelector('.slack-list-detail-scroll');return {ok:visible&&scroll?.clientHeight>0,hitTestedControls:visible,bodyReachable:scroll?.clientHeight>0,viewport:{width:innerWidth,height:innerHeight}};})()"#).await
}
