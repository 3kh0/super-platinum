//! Calendar edits reuse canonical cell writes, including hidden detail fields.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_date_edits(&mut state.write())
        .ok_or("Date checks require a disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_EDIT_DATES"]')})"#).await?;
    choose(desktop, "V_EDIT_DATES").await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] [data-column-id="C_EDIT_DATE"]')&&!document.querySelector('.slack-list-grid [data-column-id="C_EDIT_TODO"]')&&!!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LATER"]')&&!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_000"] [data-column-id="C_EDIT_LATER"] .slack-list-edit-cell')})"#).await?;

    edit(desktop, "R_LIST_002", "C_EDIT_DATE", false, "").await?;
    // A native date input sanitizes malformed values. Simulate its invalid
    // browser validity state to exercise the guarded Save branch directly.
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-date-draft');Object.defineProperty(input,'validity',{configurable:true,get:()=>({valid:false,badInput:true})});return {ok:true};})()"#).await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('slack-list-editor-save').click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-cell-editor [role=alert]')&&!!document.getElementById('slack-list-date-draft')&&!document.getElementById('slack-list-date-draft').disabled&&!document.getElementById('slack-list-editor-save').disabled&&!document.getElementById('slack-list-date-clear').disabled})"#).await?;
    assert_attempts(state, attempts)?;
    checked(desktop,r#"(()=>{delete document.getElementById('slack-list-date-draft').validity;return {ok:true};})()"#).await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    edit(desktop, "R_LIST_002", "C_EDIT_DATE", false, "").await?;
    date(desktop, "2028-02-29").await?;
    let editor = capture_editor(desktop, "date-editor").await?;
    save(state, desktop, attempts + 1).await?;
    canonical(state, "R_LIST_002", "edit_date", json!(["2028-02-29"]))?;
    edit(desktop, "R_LIST_002", "C_EDIT_DATE", false, "2028-02-29").await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('slack-list-date-clear').click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('slack-list-date-draft')?.value===''} )"#,
    )
    .await?;
    assert_attempts(state, attempts + 1)?;
    save(state, desktop, attempts + 2).await?;
    canonical(state, "R_LIST_002", "edit_date", json!([]))?;
    edit(desktop, "R_LIST_002", "C_EDIT_DATE", false, "").await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-date-draft').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.activeElement===document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] [data-column-id="C_EDIT_DATE"]')})"#).await?;

    checked(desktop,r#"(()=>{const cell=document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_000"] [data-column-id="C_NAME"]');cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',altKey:true,bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_000'&&document.activeElement===document.getElementById('slack-list-record-back')})"#).await?;
    edit(desktop, "R_LIST_000", "C_EDIT_TODO", true, "2026-10-13").await?;
    date(desktop, "2026-10-14").await?;
    let hidden = capture_editor(desktop, "date-hidden-editor").await?;
    save(state, desktop, attempts + 3).await?;
    canonical(state, "R_LIST_000", "todo_due_date", json!(["2026-10-14"]))?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_000'&&document.activeElement===document.querySelector('#slack-list-record-detail [data-column-id="C_EDIT_TODO"]')})"#).await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('slack-list-record-back').click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-record-detail')&&!document.querySelector('.slack-list-layout').hasAttribute('inert')})"#).await?;

    edit(desktop, "R_LIST_000", "C_EDIT_DUE", false, "2026-10-13").await?;
    date(desktop, "2026-10-15").await?;
    fixture::change_date_edit_due(&mut state.write());
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-date-draft')?.value==='2026-10-15'&&document.getElementById('slack-list-editor-save')?.disabled&&!!document.querySelector('#slack-list-cell-editor [role=alert]')})"#).await?;
    canonical(state, "R_LIST_000", "edit_due", json!(["2026-10-16"]))?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    assert_attempts(state, attempts + 3)?;

    choose(desktop, "V_EDIT_DATE_FILTER").await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]').length===2&&!!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_000"]')})"#).await?;
    edit(desktop, "R_LIST_000", "C_EDIT_DATE", false, "2026-10-13").await?;
    date(desktop, "2026-10-14").await?;
    save(state, desktop, attempts + 4).await?;
    canonical(state, "R_LIST_000", "edit_date", json!(["2026-10-14"]))?;
    wait_dom(desktop,r#"(()=>{const rows=[...document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]')],active=document.activeElement;return {ok:rows.length===1&&rows[0].dataset.rowId==='R_LIST_001'&&active?.closest('tr')===rows[0]&&active.matches('td[data-column-id]')&&active.tabIndex===0,rows:rows.map(row=>row.dataset.rowId),active:{tag:active?.tagName,id:active?.id,column:active?.dataset.columnId}};})()"#).await?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Date writes did not retain canonical request identity".into());
    }
    choose(desktop, "").await?;
    fixture::restore_date_edits(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-view-select option[value="V_EDIT_DATES"]')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11})"#).await?;
    assert_attempts(state, attempts + 4)?;
    Ok(
        json!({"emptyAndLeapDate":true,"clear":true,"invalidNoWrite":true,"snapshotLock":true,"pendingLock":true,"invalidRecovery":true,"timestampReadOnly":true,"hiddenDetailEdit":true,"staleDraft":true,"filteredRowFocus":true,"escape":true,"canonicalWrites":true,"cleanup":true,"captures":{"editor":editor,"hidden":hidden}}),
    )
}

async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before=checked(desktop,&format!(r#"(()=>{{const picker=document.getElementById('slack-list-view-select'),dialog=picker.closest('.slack-list-dialog'),old=dialog.dataset.layoutGeneration;picker.focus();picker.value={};picker.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,old}};}})()"#,json!(id))).await?;
    wait_dom(desktop,&format!(r#"({{ok:document.getElementById('slack-list-view-select')?.value==={}&&document.querySelector('.slack-list-dialog')?.dataset.layoutGeneration!=={}}})"#,json!(id),before["old"])).await?;
    Ok(())
}
async fn edit(
    desktop: &dioxus::desktop::DesktopContext,
    row: &str,
    column: &str,
    detail: bool,
    seed: &str,
) -> Result<(), String> {
    let selector = if detail {
        format!("#slack-list-record-detail .slack-list-detail-field[data-column-id=\"{column}\"]")
    } else {
        format!(".slack-list-grid tr[data-row-id=\"{row}\"] [data-column-id=\"{column}\"]")
    };
    checked(desktop,&format!(r#"(()=>{{const field=document.querySelector({});if(!field?.querySelector('.slack-list-edit-cell:not(:disabled)'))return {{ok:false}};field.scrollIntoView({{block:'nearest',inline:'nearest'}});field.focus();field.dispatchEvent(new KeyboardEvent('keydown',{{key:'F2',bubbles:true,cancelable:true}}));return {{ok:true}};}})()"#,json!(selector))).await?;
    wait_dom(desktop,&format!(r#"(()=>{{const input=document.getElementById('slack-list-date-draft'),editor=document.getElementById('slack-list-cell-editor');return {{ok:input?.value==={}&&document.activeElement===input&&!document.getElementById('slack-list-editor-save')?.disabled,active:document.activeElement?.id,value:input?.value,focusPhase:editor?.dataset.focusPhase}};}})()"#,json!(seed))).await?;
    Ok(())
}
async fn date(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const input=document.getElementById('slack-list-date-draft');input.value={};input.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:input.value==={}}};}})()"#,json!(value),json!(value))).await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    attempts: usize,
) -> Result<(), String> {
    // Observe the first microtask after the browser reads native validity. This
    // tests that the captured draft is locked before asynchronous delivery, not
    // merely after the controller has received it and entered Saving.
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-date-draft'),native=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'validity').get;window.__dateSnapshotLock=null;let observed=false;Object.defineProperty(input,'validity',{configurable:true,get(){if(!observed){observed=true;queueMicrotask(()=>{window.__dateSnapshotLock={locked:input.disabled||input.readOnly,connected:input.isConnected,value:input.value};});}return native.call(input);}});const save=document.getElementById('slack-list-editor-save');save.focus();save.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:window.__dateSnapshotLock?.locked===true&&window.__dateSnapshotLock?.connected===true,snapshot:window.__dateSnapshotLock})"#).await?;
    wait_attempts(state, attempts).await?;
    wait_dom(desktop,r#"(()=>{const input=document.getElementById('slack-list-date-draft'),save=document.getElementById('slack-list-editor-save'),clear=document.getElementById('slack-list-date-clear');return {ok:!!input&&(input.disabled||input.readOnly)&&save?.disabled&&clear?.disabled&&input.value===window.__dateSnapshotLock.value,inputDisabled:input?.disabled,inputReadOnly:input?.readOnly,saveDisabled:save?.disabled,clearDisabled:clear?.disabled};})()"#).await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-date-draft');delete input.validity;const save=document.getElementById('slack-list-editor-save');save.click();save.click();document.getElementById('slack-list-date-clear').click();return {ok:input.value===window.__dateSnapshotLock.value};})()"#).await?;
    assert_attempts(state, attempts)?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('slack-list-cell-editor')})"#,
    )
    .await?;
    assert_attempts(state, attempts)
}
fn canonical(
    state: Signal<ShellState>,
    row: &str,
    key: &str,
    expected: Value,
) -> Result<(), String> {
    if fixture::date_edit_value(&state.peek(), row, key) != Some(expected) {
        return Err(format!("Date readback mismatch for {row}/{key}"));
    }
    Ok(())
}

async fn capture_editor(
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Option<String>, String> {
    wait_dom(desktop,r#"(()=>{const editor=document.getElementById('slack-list-cell-editor'),input=document.getElementById('slack-list-date-draft'),save=document.getElementById('slack-list-editor-save');const visible=e=>{if(!e)return false;const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;};return {ok:visible(input)&&visible(save)&&editor?.contains(document.activeElement)};})()"#).await?;
    capture_stage(desktop, stage).await
}
