//! Exercises real native DOM/controller selection, not a duplicate projector.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let initial_attempts = fixture::attempt_count(&state.peek());
    fixture::add_saved_views(&mut state.write());
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_LIST_GROUPED"]')})"#).await?;
    choose(desktop, "V_LIST_REORDERED_WIDE").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells[1]?.dataset.columnId==='C_NAME'})"#).await?;
    let horizontal = super::horizontal::verify(desktop, 1).await?;
    choose(desktop, "V_LIST_REORDERED").await?;
    wait_dom(desktop,r#"(()=>{const row=document.querySelector('.slack-list-grid tbody tr[data-row-id]');return {ok:row&&JSON.stringify([...row.cells].map(c=>c.dataset.columnId))===JSON.stringify(['C_SELECT','C_NAME','C_CHECK'])&&!document.querySelector('.slack-list-grid [data-column-id="C_NUMBER"]')};})()"#).await?;
    let compact = capture_stage(desktop, "saved-view-columns").await?;
    choose(desktop, "V_LIST_NONAME").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===2})"#).await?;
    super::horizontal::hidden(desktop).await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('slack-list-next').click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop,r#"(()=>{const row=document.querySelector('tr[data-row-id="R_LIST_020"]');return {ok:row&&JSON.stringify([...row.cells].map(c=>c.dataset.columnId))===JSON.stringify(['C_NUMBER','C_SELECT'])&&row.querySelector('[data-column-id="C_NUMBER"] .slack-list-cell-text')?.textContent===''&&!row.querySelector('[data-tree-primary="true"]')};})()"#).await?;
    choose(desktop, "V_LIST_FILTERED").await?;
    wait_dom(desktop,r#"(()=>{const rows=[...document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]')];return {ok:JSON.stringify(rows.map(r=>r.dataset.rowId))===JSON.stringify(['R_LIST_072','R_LIST_070','R_LIST_068'])&&rows.every(r=>r.cells.length===3)};})()"#).await?;
    choose(desktop, "V_LIST_GROUPED").await?;
    wait_dom(desktop,r#"(()=>{const rows=[...document.querySelectorAll('tr[data-row-id="R_LIST_002"]')],groups=[...document.querySelectorAll('tr[data-group-key]')];return {ok:rows.length===2&&new Set(rows.map(r=>r.dataset.instanceId)).size===2&&groups.length===4&&groups.some(g=>g.textContent.includes('Operations'))&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1,rows:rows.length,groups:groups.length};})()"#).await?;
    let grouped = capture_stage(desktop, "saved-view-groups").await?;
    for expanded in [false, true] {
        checked(desktop,r#"(()=>{const button=document.querySelector('tr[data-group-key="OPT_DESIGN"] .slack-list-group-toggle');if(!button)return {ok:false};button.focus({preventScroll:true});button.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
        wait_dom(desktop,&format!(r#"(()=>{{const button=document.querySelector('tr[data-group-key="OPT_DESIGN"] .slack-list-group-toggle');return {{ok:button?.getAttribute('aria-expanded')==={}&&document.activeElement===button&&document.querySelectorAll('tr[data-row-id="R_LIST_002"]').length==={}}};}})()"#,json!(expanded.to_string()),if expanded {2}else{1})).await?;
    }
    checked(desktop,r#"(()=>{const button=document.querySelector('tr[data-group-key="OPT_DESIGN"] .slack-list-group-toggle');button.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const group=document.querySelector('tr[data-group-key="OPT_DESIGN"]'),row=group?.nextElementSibling,cell=row?.cells[0];return {ok:row?.dataset.rowId==='R_LIST_004'&&document.activeElement===cell&&cell.tabIndex===0&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1};})()"#).await?;
    checked(desktop,r#"(()=>{const header=document.querySelector('tr[data-group-key="OPT_DESIGN"]'),button=header?.querySelector('.slack-list-group-toggle');window.__listGroupButton=button;const row=[...document.querySelectorAll('tr[data-row-id="R_LIST_002"]')][0];row.cells[0].focus();button.click();return {ok:!!button};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('tr[data-row-id="R_LIST_002"]').length===1&&window.__listGroupButton.getAttribute('aria-expanded')==='false'&&document.activeElement===window.__listGroupButton})"#).await?;
    checked(
        desktop,
        r#"(()=>{window.__listGroupButton.click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('tr[data-row-id="R_LIST_002"]').length===2})"#,
    )
    .await?;
    for collapsed in [true, false] {
        for key in ["OPT_DESIGN", "OPT_ENGINEERING", "OPT_OPERATIONS", ""] {
            let selector = format!("tr[data-group-key=\"{key}\"] .slack-list-group-toggle");
            checked(desktop,&format!(r#"(()=>{{const button=document.querySelector({});button.click();return {{ok:true}};}})()"#,json!(selector))).await?;
            wait_dom(
                desktop,
                &format!(
                    r#"({{ok:document.querySelector({})?.getAttribute('aria-expanded')==={}}})"#,
                    json!(selector),
                    json!((!collapsed).to_string())
                ),
            )
            .await?;
        }
        if collapsed {
            checked(desktop,r#"({ok:document.querySelectorAll('tr[data-group-key]').length===4&&document.querySelectorAll('tr[data-row-id]').length===0})"#).await?;
        }
    }
    assert_attempts(state, initial_attempts)?;
    checked(desktop,r#"(()=>{const rows=[...document.querySelectorAll('tr[data-row-id="R_LIST_002"]')];rows[0].querySelector('input[type=checkbox]').click();rows[1].querySelector('input[type=checkbox]').dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_attempts(state, initial_attempts + 1).await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"(()=>{const checks=[...document.querySelectorAll('tr[data-row-id="R_LIST_002"] input[type=checkbox]')];return {ok:checks.length===2&&checks.every(c=>c.checked&&!c.disabled)};})()"#).await?;
    assert_attempts(state, initial_attempts + 1)?;
    // Save from the Design copy of a multi-group row, removing that copy while
    // preserving the canonical record in Engineering. Focus must follow the cell.
    checked(desktop,r#"(()=>{const row=document.querySelector('tr[data-row-id="R_LIST_002"]'),cell=row?.querySelector('[data-column-id="C_MULTI"]');let group=row;while(group&&!group.hasAttribute('data-group-key'))group=group.previousElementSibling;if(!cell||group?.dataset.groupKey!=='OPT_DESIGN'||!cell.querySelector('.slack-list-edit-cell:not(:disabled)'))return {ok:false};window.__listMovingOrigin={instance:row.dataset.instanceId,cell};cell.focus({preventScroll:true});cell.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const editor=document.getElementById('slack-list-cell-editor');return {ok:!!editor&&editor.contains(document.activeElement)&&editor.querySelector('input[value="OPT_DESIGN"]')?.checked&&editor.querySelector('input[value="OPT_ENGINEERING"]')?.checked&&!document.getElementById('slack-list-editor-save').disabled};})()"#).await?;
    checked(desktop,r#"(()=>{document.querySelector('#slack-list-cell-editor input[value="OPT_DESIGN"]').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('#slack-list-cell-editor input:checked').length===1&&document.querySelector('#slack-list-cell-editor input[value="OPT_ENGINEERING"]')?.checked&&!document.getElementById('slack-list-editor-save').disabled})"#).await?;
    checked(
        desktop,
        r#"(()=>{const save=document.getElementById('slack-list-editor-save');save.focus({preventScroll:true});save.click();return {ok:true};})()"#,
    )
    .await?;
    wait_attempts(state, initial_attempts + 2).await?;
    wait_dom(
        desktop,
        r#"({ok:document.getElementById('slack-list-editor-save')?.disabled})"#,
    )
    .await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"(()=>{const rows=[...document.querySelectorAll('tr[data-row-id="R_LIST_002"]')],row=rows[0],cell=row?.querySelector('[data-column-id="C_MULTI"]');let group=row;while(group&&!group.hasAttribute('data-group-key'))group=group.previousElementSibling;const active=document.activeElement;return {ok:!document.getElementById('slack-list-cell-editor')&&rows.length===1&&row.dataset.instanceId!==window.__listMovingOrigin.instance&&!window.__listMovingOrigin.cell.isConnected&&group?.dataset.groupKey==='OPT_ENGINEERING'&&cell.querySelector('.slack-list-option')?.textContent==='Engineering'&&active===cell&&cell.tabIndex===0&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1,rows:rows.length,group:group?.dataset.groupKey,active:{tag:active?.tagName,id:active?.id,row:active?.closest('tr')?.dataset.rowId,column:active?.dataset.columnId}};})()"#).await?;
    assert_attempts(state, initial_attempts + 2)?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Grouped cell edit lost canonical request identity".into());
    }
    choose(desktop, "V_LIST_FILTERED").await?;
    fixture::set_incomplete(&mut state.write());
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-notices').textContent.includes('Only part')&&document.querySelectorAll('.slack-list-grid tbody tr[data-row-id]').length===3})"#).await?;
    fixture::restore(&mut state.write());
    crate::slack_lists::refresh(state);
    choose(desktop, "V_LIST_BOARD").await?;
    wait_dom(desktop,r#"({ok:!document.querySelector('.slack-list-grid tbody tr[data-row-id]')&&/not supported|unsupported/i.test(document.querySelector('.slack-list-notices').textContent)})"#).await?;
    choose(desktop, "").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11&&document.querySelector('.slack-list-notices').textContent.includes('without saved-view')})"#).await?;
    assert_attempts(state, initial_attempts + 2)?;
    Ok(
        json!({"columns":true,"reorderedHorizontal":horizontal,"hiddenPrimaryUnpinned":true,"filterSort":true,"duplicateGroupIdentity":true,"collapseFocus":true,"groupKeyboard":true,"canonicalDuplicateWrite":true,"groupedEditFocus":true,"incompletePreserved":true,"unsupportedExplicit":true,"fallback":true,"captures":{"columns":compact,"groups":grouped}}),
    )
}

async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let previous = checked(desktop,&format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');if(!select||!dialog)return {{ok:false}};const previousValue=select.value,previousLayout=dialog.dataset.layoutGeneration;select.value={};select.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,previousValue,previousLayout}};}})()"#,json!(id))).await?;
    wait_dom(
        desktop,
        &format!(
            r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');return {{ok:select?.value==={}&&!!dialog&&({}==={}||dialog.dataset.layoutGeneration!=={}),value:select?.value,layout:dialog?.dataset.layoutGeneration}};}})()"#,
            json!(id), previous["previousValue"], json!(id), previous["previousLayout"]
        ),
    )
    .await?;
    Ok(())
}
