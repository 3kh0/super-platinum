//! Full-schema details exercise canonical edits from table and Board instances.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let opens = fixture::record_open_count(&state.peek());
    let snapshot = fixture::add_record_details(&mut state.write())
        .ok_or("Record checks require a disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_DETAIL_TABLE"]')})"#).await?;
    choose(desktop, "V_DETAIL_TABLE").await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] [data-column-id="C_NAME"]')&&!document.querySelector('.slack-list-grid [data-column-id="C_MULTI"]')&&document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})"#).await?;
    checked(desktop,r#"(()=>{const cell=document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] [data-column-id="C_NAME"]');window.__detailOrigin=cell;cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',altKey:true,bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    detail_ready(desktop).await?;
    wait_record_opens(state, opens + 1).await?;
    let keyboard_help = keyboard_help::run(desktop, "record").await?;
    checked(desktop,r#"(()=>{const panel=document.getElementById('slack-list-record-detail'),fields=[...panel.querySelectorAll('.slack-list-detail-field')],scroll=panel.querySelector('.slack-list-detail-scroll'),back=document.getElementById('slack-list-record-back'),rect=panel.getBoundingClientRect();return {ok:fields.length===30&&fields[0].dataset.columnId==='C_NAME'&&!!panel.querySelector('[data-column-id="C_MULTI"]')&&panel.querySelector('.slack-list-detail-pagination').textContent.includes('35')&&document.querySelector('.slack-list-grid').closest('[hidden],[inert]')!==null&&rect.left>=0&&rect.right<=innerWidth&&rect.bottom<=innerHeight&&scroll.clientHeight>0&&back.getBoundingClientRect().bottom<=innerHeight,fields:fields.length,height:scroll.clientHeight,panel:{left:rect.left,right:rect.right,bottom:rect.bottom}};})()"#).await?;
    checked(desktop,r#"(()=>{const fields=[...document.querySelectorAll('#slack-list-record-detail .slack-list-detail-field')];fields[0].focus();fields[0].dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));return {ok:document.activeElement===fields[1]&&document.querySelectorAll('#slack-list-record-detail .slack-list-detail-field[tabindex="0"]').length===1};})()"#).await?;
    checked(desktop,r#"(()=>{window.__detailFirst=document.querySelector('#slack-list-record-detail .slack-list-detail-field');const next=document.getElementById('slack-list-detail-next');next.focus();next.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const fields=[...document.querySelectorAll('#slack-list-record-detail .slack-list-detail-field')];return {ok:fields.length===5&&!window.__detailFirst.isConnected&&document.activeElement===fields[0]&&document.getElementById('slack-list-detail-next').disabled};})()"#).await?;
    // A canonical refresh retires action callbacks without remounting the open panel.
    checked(desktop,r#"(()=>{const panel=document.getElementById('slack-list-record-detail'),fields=[...panel.querySelectorAll('.slack-list-detail-field')];window.__detailRefresh={panel,fields,focus:document.activeElement,action:panel.dataset.detailGeneration,open:panel.dataset.detailOpenGeneration};return {ok:fields.length===5&&document.activeElement===fields[0]&&!!panel.dataset.detailOpenGeneration};})()"#).await?;
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"(()=>{const before=window.__detailRefresh,panel=document.getElementById('slack-list-record-detail'),fields=[...panel.querySelectorAll('.slack-list-detail-field')],sameFields=fields.length===before.fields.length&&fields.every((field,i)=>field===before.fields[i]);return {ok:panel===before.panel&&panel.dataset.detailGeneration!==before.action&&panel.dataset.detailOpenGeneration===before.open&&sameFields&&fields.length===5&&document.activeElement===before.focus&&before.focus.isConnected&&document.getElementById('slack-list-detail-next').disabled&&!document.getElementById('slack-list-detail-previous').disabled,samePanel:panel===before.panel,sameFields,fields:fields.map(field=>field.dataset.columnId),actionBefore:before.action,actionAfter:panel.dataset.detailGeneration,openBefore:before.open,openAfter:panel.dataset.detailOpenGeneration,focusPreserved:document.activeElement===before.focus,active:{tag:document.activeElement?.tagName,id:document.activeElement?.id,column:document.activeElement?.dataset.columnId}};})()"#).await?;
    assert_record_opens(state, opens + 1)?;
    checked(desktop,r#"(()=>{const previous=document.getElementById('slack-list-detail-previous');previous.focus();previous.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelectorAll('#slack-list-record-detail .slack-list-detail-field').length===30&&document.activeElement===document.querySelector('#slack-list-record-detail .slack-list-detail-field')})"#).await?;

    // Escape first cancels the nested draft, retaining the record panel.
    edit(desktop, "C_NUMBER").await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-number-draft');input.value='999';input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&!!document.getElementById('slack-list-record-detail')&&document.activeElement===document.querySelector('#slack-list-record-detail [data-column-id="C_NUMBER"]')})"#).await?;
    assert_attempts(state, attempts)?;

    edit(desktop, "C_MULTI").await?;
    checked(desktop,r#"(()=>{const design=document.querySelector('#slack-list-cell-editor input[value="OPT_DESIGN"]'),engineering=document.querySelector('#slack-list-cell-editor input[value="OPT_ENGINEERING"]');if(!design?.checked||!engineering?.checked)return {ok:false};design.click();return {ok:true};})()"#).await?;
    save(state, desktop, attempts + 1).await?;
    wait_dom(desktop,r#"(()=>{const options=[...document.querySelectorAll('#slack-list-record-detail [data-column-id="C_MULTI"] .slack-list-option')].map(e=>e.textContent);return {ok:JSON.stringify(options)===JSON.stringify(['Engineering'])};})()"#).await?;
    assert_record_opens(state, opens + 1)?;
    let (table, table_geometry) = capture_detail(desktop, "detail-table").await?;
    close(desktop).await?;
    wait_dom(desktop,r#"({ok:document.activeElement===window.__detailOrigin&&window.__detailOrigin.isConnected&&!window.__detailOrigin.closest('[hidden],[inert]')})"#).await?;

    choose(desktop, "V_BOARD_MULTI").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-board')?.dataset.keyboardMounted==='true'&&!!document.querySelector('.slack-list-lane[data-group-key="OPT_ENGINEERING"] .slack-list-card[data-row-id="R_LIST_002"]')&&!document.querySelector('.slack-list-card [data-column-id="C_MULTI"]')})"#).await?;
    checked(desktop,r#"(()=>{const card=document.querySelector('.slack-list-lane[data-group-key="OPT_ENGINEERING"] .slack-list-card[data-row-id="R_LIST_002"]'),field=card.querySelector('[data-column-id="C_NAME"]');window.__detailBoardOrigin={instance:card.dataset.instanceId,field};field.focus();card.querySelector('.slack-list-record-open').click();return {ok:true};})()"#).await?;
    detail_ready(desktop).await?;
    wait_record_opens(state, opens + 2).await?;
    edit(desktop, "C_MULTI").await?;
    checked(desktop,r#"(()=>{const design=document.querySelector('#slack-list-cell-editor input[value="OPT_DESIGN"]'),engineering=document.querySelector('#slack-list-cell-editor input[value="OPT_ENGINEERING"]');if(design?.checked||!engineering?.checked)return {ok:false};engineering.click();design.click();return {ok:true};})()"#).await?;
    save(state, desktop, attempts + 2).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-record-detail [data-column-id="C_MULTI"] .slack-list-option')?.textContent==='Design'&&document.querySelectorAll('#slack-list-record-detail [data-column-id="C_MULTI"] .slack-list-option').length===1})"#).await?;
    let (board, board_geometry) = capture_detail(desktop, "detail-board").await?;
    close(desktop).await?;
    wait_dom(desktop,r#"(()=>{const card=document.querySelector('.slack-list-lane[data-group-key="OPT_DESIGN"] .slack-list-card[data-row-id="R_LIST_002"]'),field=card?.querySelector('[data-column-id="C_NAME"]');return {ok:!!field&&card.dataset.instanceId!==window.__detailBoardOrigin.instance&&!window.__detailBoardOrigin.field.isConnected&&document.activeElement===field&&field.tabIndex===0&&!field.closest('[hidden],[inert]')};})()"#).await?;

    // Selecting another view retires an open detail draft before it can send.
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-card[data-row-id="R_LIST_002"] .slack-list-record-open').click();return {ok:true};})()"#).await?;
    detail_ready(desktop).await?;
    wait_record_opens(state, opens + 3).await?;
    edit(desktop, "C_NUMBER").await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-number-draft');input.value='777';input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    choose(desktop, "V_DETAIL_TABLE").await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&!document.getElementById('slack-list-record-detail')&&document.activeElement===document.getElementById('slack-list-view-select')})"#).await?;
    assert_attempts(state, attempts + 2)?;

    checked(desktop,r#"(()=>{const field=document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_002"] [data-column-id="C_NAME"]');field.focus();field.closest('tr').querySelector('.slack-list-record-open').click();return {ok:true};})()"#).await?;
    detail_ready(desktop).await?;
    wait_record_opens(state, opens + 4).await?;
    edit(desktop, "C_NUMBER").await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-number-draft');if(input.value!=='2')return {ok:false};input.value='999';input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    if !fixture::change_detail_number(&mut state.write(), "R_LIST_002", 19) {
        return Err("Current record event did not invalidate its owning List".into());
    }
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&!!document.querySelector('#slack-list-cell-editor [role=alert]')&&document.getElementById('slack-list-number-draft')?.value==='999'})"#).await?;
    fixture::restore(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-editor-save')?.disabled&&document.getElementById('slack-list-number-draft')?.value==='999'&&document.querySelector('#slack-list-record-detail [data-column-id="C_NUMBER"]')?.textContent.includes('19')})"#).await?;
    checked(desktop,r#"(()=>{document.getElementById('slack-list-editor-cancel').click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_002'})"#).await?;
    assert_attempts(state, attempts + 2)?;
    fixture::remove_row(&mut state.write(), "R_LIST_002");
    wait_dom(desktop,r#"(()=>{const panel=document.getElementById('slack-list-record-detail');return {ok:panel?.dataset.recordId==='R_LIST_002'&&panel.querySelector('[role=status]')?.textContent.includes('no longer available')&&[...panel.querySelectorAll('input[type=checkbox],.slack-list-edit-cell')].every(e=>e.disabled)};})()"#).await?;
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-detail-scroll').scrollTop=0;return {ok:true};})()"#).await?;
    let unavailable = capture_stage(desktop, "detail-unavailable").await?;
    close(desktop).await?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Detail edit lost canonical record or hidden-field identity".into());
    }
    choose(desktop, "").await?;
    fixture::restore_board_views(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-record-detail')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11&&!document.querySelector('#slack-list-view-select option[value="V_DETAIL_TABLE"]')})"#).await?;
    assert_attempts(state, attempts + 2)?;
    assert_record_opens(state, opens + 4)?;
    Ok(
        json!({"tableOpen":true,"keyboardHelp":keyboard_help,"boardOpen":true,"hiddenFields":true,"fieldPaging":true,"refreshPreservesPanelPageFocus":true,"recordOpenTracking":true,"recordOpenNoRefreshReplay":true,
        "keyboard":true,"nestedEscape":true,"hiddenFieldWrites":true,"canonicalReturnFocus":true,
        "viewChangeCancelsDraft":true,"eventStaleGuard":true,"readbackPreservesDraft":true,
        "removedRecordUnavailable":true,"boundedLayout":true,"cleanup":true,
        "captures":{"table":table,"board":board,"unavailable":unavailable},
        "captureGeometry":{"table":table_geometry,"board":board_geometry}}),
    )
}

async fn detail_ready(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const panel=document.getElementById('slack-list-record-detail'),back=document.getElementById('slack-list-record-back'),stops=panel?.querySelectorAll('.slack-list-detail-field[tabindex="0"]').length;return {ok:panel?.dataset.recordId==='R_LIST_002'&&document.activeElement===back&&stops===1,record:panel?.dataset.recordId,active:{tag:document.activeElement?.tagName,id:document.activeElement?.id},stops};})()"#).await?;
    Ok(())
}
async fn edit(desktop: &dioxus::desktop::DesktopContext, column: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const field=document.querySelector('#slack-list-record-detail [data-column-id="'+{}+'"]');if(!field?.querySelector('.slack-list-edit-cell:not(:disabled)'))return {{ok:false}};window.__detailEditTarget=field;field.focus();return {{ok:document.activeElement===field,column:field.dataset.columnId,connected:field.isConnected,inert:!!field.closest('[inert],[hidden]')}};}})()"#,json!(column))).await?;
    wait_dom(desktop,r#"(()=>{const field=window.__detailEditTarget,active=document.activeElement;return {ok:field?.isConnected&&active===field&&field.tabIndex===0,column:field?.dataset.columnId,connected:field?.isConnected,tabIndex:field?.tabIndex,active:{tag:active?.tagName,id:active?.id,column:active?.dataset.columnId}};})()"#).await?;
    checked(desktop,r#"(()=>{const field=window.__detailEditTarget;if(!field?.isConnected||document.activeElement!==field)return {ok:false};field.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const editor=document.getElementById('slack-list-cell-editor'),save=document.getElementById('slack-list-editor-save'),active=document.activeElement,ancestors=[];for(let node=active;node&&ancestors.length<6;node=node.parentElement)ancestors.push({tag:node.tagName,id:node.id,className:node.className,inert:node.hasAttribute('inert'),hidden:node.hasAttribute('hidden')});return {ok:!!editor&&editor.contains(active)&&!!save&&!save.disabled,editorPresent:!!editor,editorGeneration:editor?.dataset.editorGeneration,focusPhase:editor?.dataset.focusPhase,focusPreviousId:editor?.dataset.focusPreviousId,focusPreviousTag:editor?.dataset.focusPreviousTag,focusCurrentId:editor?.dataset.focusCurrentId,focusCurrentTag:editor?.dataset.focusCurrentTag,saveDisabled:save?.disabled,editorInert:!!editor?.closest('[inert],[hidden]'),active:{tag:active?.tagName,id:active?.id,column:active?.dataset.columnId},ancestors,origin:{connected:window.__detailEditTarget?.isConnected,column:window.__detailEditTarget?.dataset.columnId}};})()"#).await?;
    Ok(())
}
async fn save(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    expected: usize,
) -> Result<(), String> {
    checked(desktop,r#"(()=>{const save=document.getElementById('slack-list-editor-save');save.focus();save.click();return {ok:true};})()"#).await?;
    wait_attempts(state, expected).await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-cell-editor')&&document.getElementById('slack-list-record-detail')?.dataset.recordId==='R_LIST_002'})"#).await?;
    assert_attempts(state, expected)
}
async fn close(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(desktop,r#"(()=>{const back=document.getElementById('slack-list-record-back');back.focus();back.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.getElementById('slack-list-record-detail')&&!!document.querySelector('.slack-list-dialog')})"#).await?;
    Ok(())
}
async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before=checked(desktop,&format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');if(!select||!dialog)return {{ok:false}};const value=select.value,layout=dialog.dataset.layoutGeneration;select.focus();select.value={};select.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,value,layout}};}})()"#,json!(id))).await?;
    wait_dom(desktop,&format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');return {{ok:select?.value==={}&&!!dialog&&({}==={}||dialog.dataset.layoutGeneration!=={})}};}})()"#,json!(id),before["value"],json!(id),before["layout"])).await?;
    Ok(())
}
async fn capture_detail(
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<(Option<String>, Value), String> {
    let prepare = include_str!("record_capture.js").replace("__COMMAND__", "'prepare'");
    let measure = include_str!("record_capture.js").replace("__COMMAND__", "'measure'");
    let header = checked(desktop, &prepare).await?;
    let before = wait_dom(desktop, &measure).await?;
    let path = capture_stage(desktop, stage).await?;
    let after = checked(desktop, &measure).await?;
    if before != after {
        return Err(format!(
            "{stage} record geometry changed during capture: before={before}; after={after}"
        ));
    }
    Ok((
        path,
        json!({"header":header["header"],"before":before,"after":after}),
    ))
}

/// Mount tracking can finish after the first DOM paint; wait for its observable request.
pub(super) async fn wait_record_opens(
    state: Signal<ShellState>,
    expected: usize,
) -> Result<(), String> {
    for _ in 0..40 {
        if fixture::record_open_count(&state.peek()) >= expected {
            return assert_record_opens(state, expected);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_record_opens(state, expected)
}
pub(super) fn assert_record_opens(
    state: Signal<ShellState>,
    expected: usize,
) -> Result<(), String> {
    let shell = state.peek();
    let actual = fixture::record_open_count(&shell);
    if actual != expected {
        return Err(format!(
            "Expected {expected} record-open requests, got {actual}"
        ));
    }
    if !fixture::record_open_valid(&shell) {
        return Err(
            "Record-open request lost its canonical identity or captured wire shape".into(),
        );
    }
    Ok(())
}
