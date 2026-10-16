//! Board interactions use the existing disconnected backend and native window.
use super::*;
use crate::slack_lists::fixture;

pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let attempts = fixture::attempt_count(&state.peek());
    let snapshot = fixture::add_board_views(&mut state.write())
        .ok_or("Board checks require a disconnected fixture")?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_BOARD_MULTI"]')})"#).await?;
    choose(desktop, "V_BOARD_MULTI").await?;
    lanes(
        desktop,
        &[
            ("OPT_ENGINEERING", 2),
            ("OPT_DESIGN", 2),
            ("OPT_OPERATIONS", 0),
            ("", 67),
        ],
    )
    .await?;
    wait_dom(desktop,r#"(()=>{
      const cards=[...document.querySelectorAll('.slack-list-card')],copies=cards.filter(c=>c.dataset.rowId==='R_LIST_002');
      const fields=cards.map(c=>({row:c.dataset.rowId,lane:c.closest('.slack-list-lane')?.dataset.groupKey,columns:[...c.querySelectorAll('.slack-list-card-field')].map(f=>f.dataset.columnId)}));
      const stops=[...document.querySelectorAll('[data-board-cell][tabindex="0"]')].map(c=>({row:c.closest('.slack-list-card')?.dataset.rowId,column:c.dataset.columnId})),keyboardMounted=document.querySelector('.slack-list-board')?.dataset.keyboardMounted;
      return {ok:keyboardMounted==='true'&&copies.length===2&&new Set(copies.map(c=>c.dataset.instanceId)).size===2&&fields.every(f=>JSON.stringify(f.columns)===JSON.stringify(['C_NAME','C_NUMBER','C_CHECK']))&&stops.length===1,copies:copies.map(c=>c.dataset.instanceId),fields,stops,keyboardMounted};
    })()"#).await?;
    let keyboard_help = keyboard_help::run(desktop, "board").await?;
    checked(desktop,r#"(()=>{
      const first=document.querySelector('.slack-list-lane[data-group-key="OPT_ENGINEERING"] [data-column-id="C_NAME"]');first.focus();
      first.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));
      const tab=document.activeElement;tab.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true}));
      if(tab.dataset.columnId!=='C_NUMBER'||document.activeElement!==first)return {ok:false};
      first.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));
      const right=document.activeElement;right.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
      const down=document.activeElement;
      down.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));
      return {ok:right.closest('.slack-list-lane')?.dataset.groupKey==='OPT_DESIGN'&&right.closest('.slack-list-card')?.dataset.rowId==='R_LIST_004'&&down.closest('.slack-list-card')?.dataset.rowId==='R_LIST_002'&&down.dataset.columnId==='C_NAME'&&document.activeElement.closest('.slack-list-lane')?.dataset.groupKey===''&&document.querySelectorAll('[data-board-cell][tabindex="0"]').length===1};
    })()"#).await?;
    checked(desktop,r#"(()=>{
      const board=document.querySelector('.slack-list-board'),rect=board.getBoundingClientRect();
      const overflow=board.scrollWidth>board.clientWidth;board.scrollLeft=board.scrollWidth;
      const scrolled=board.scrollLeft>0;board.scrollLeft=0;
      return {ok:overflow&&scrolled&&rect.left>=0&&rect.right<=innerWidth&&getComputedStyle(board).overflowX!=='visible',overflow,scrolled,width:board.clientWidth,content:board.scrollWidth};
    })()"#).await?;
    let multi = capture_stage(desktop, "board-multi").await?;
    // Page the long lane while another lane still has mounted cells. On the
    // last page its Next button becomes disabled; focus must stay in this lane.
    for _ in 0..6 {
        checked(desktop,r#"(()=>{const lane=document.querySelector('.slack-list-lane[data-group-key=""]'),next=lane.querySelector('.slack-list-lane-next');window.__boardLaneOld=lane.querySelector('.slack-list-card');window.__boardLanePage={before:lane.querySelector('.slack-list-lane-pagination')?.textContent,first:window.__boardLaneOld?.dataset.rowId};if(!next||next.disabled)return {ok:false};next.focus();next.click();return {ok:true};})()"#).await?;
        wait_dom(desktop,r#"(()=>{const lane=document.querySelector('.slack-list-lane[data-group-key=""]'),target=lane?.querySelector('[data-board-cell]'),active=document.activeElement,old=window.__boardLaneOld,stops=document.querySelectorAll('[data-board-cell][tabindex="0"]').length;const info=e=>({tag:e?.tagName,id:e?.id,lane:e?.closest('.slack-list-lane')?.dataset.groupKey,row:e?.closest('.slack-list-card')?.dataset.rowId,column:e?.dataset.columnId,tabindex:e?.tabIndex,connected:e?.isConnected});return {ok:!old.isConnected&&active===target&&stops===1,active:info(active),target:info(target),old:info(old),oldConnected:old.isConnected,stops,before:window.__boardLanePage,page:lane?.querySelector('.slack-list-lane-pagination')?.textContent,pagerDisabled:lane?.querySelector('.slack-list-lane-next')?.disabled};})()"#).await?;
    }
    checked(desktop,r#"({ok:document.querySelector('.slack-list-lane[data-group-key=""] .slack-list-lane-next').disabled&&document.querySelectorAll('.slack-list-lane[data-group-key=""] .slack-list-card').length===7})"#).await?;

    fixture::set_read_only(&mut state.write());
    lanes(
        desktop,
        &[
            ("OPT_ENGINEERING", 2),
            ("OPT_DESIGN", 2),
            ("OPT_OPERATIONS", 0),
            ("", 67),
        ],
    )
    .await?;
    wait_dom(desktop,r#"({ok:[...document.querySelectorAll('.slack-list-card input[type=checkbox]')].every(c=>c.disabled)&&!document.querySelector('.slack-list-card .slack-list-edit-cell:not(:disabled)')})"#).await?;
    fixture::restore(&mut state.write());
    crate::slack_lists::refresh(state);
    choose(desktop, "V_BOARD_FILTERED").await?;
    lanes(
        desktop,
        &[
            ("OPT_ENGINEERING", 0),
            ("OPT_DESIGN", 0),
            ("OPT_OPERATIONS", 0),
            ("", 5),
        ],
    )
    .await?;
    choose(desktop, "V_BOARD_TEAM_FILTER").await?;
    lanes(desktop, &[("OPT_ENGINEERING", 2)]).await?;
    choose(desktop, "V_BOARD_CHECK").await?;
    lanes(desktop, &[("true", 2), ("", 68)]).await?;
    checked(desktop, include_str!("board_headings.js")).await?;
    checked(desktop,r#"({ok:!document.querySelector('.slack-list-card [data-column-id="C_CHECK"]')&&[...document.querySelector('.slack-list-card').querySelectorAll('.slack-list-card-field')].map(c=>c.dataset.columnId).join(',')==='C_NAME,C_NUMBER,C_MULTI'})"#).await?;

    choose(desktop, "V_BOARD_UNGROUPED").await?;
    lanes(desktop, &[("", 70)]).await?;
    checked(desktop,r#"(()=>{
      const lane=document.querySelector('.slack-list-lane'),cards=lane.querySelectorAll('.slack-list-card');
      window.__boardOldCard=cards[0];const next=lane.querySelector('.slack-list-lane-next');next.focus();next.click();
      return {ok:cards.length===10&&!!next&&!next.disabled};
    })()"#).await?;
    wait_dom(desktop,r#"(()=>{const cards=[...document.querySelectorAll('.slack-list-card')],active=document.activeElement;return {ok:cards.length===10&&!window.__boardOldCard.isConnected&&active===cards[0].querySelector('[data-board-cell]')&&document.querySelectorAll('[data-board-cell][tabindex="0"]').length===1};})()"#).await?;
    let paging = capture_stage(desktop, "board-paging").await?;
    choose(desktop, "V_BOARD_MANY").await?;
    lanes(
        desktop,
        &[("", 70), ("LANE_1", 0), ("LANE_2", 0), ("LANE_3", 0)],
    )
    .await?;
    checked(
        desktop,
        r#"(()=>{const next=document.getElementById('slack-list-lanes-next');next.focus();next.click();return {ok:true};})()"#,
    )
    .await?;
    lanes(desktop, &[("LANE_4", 0), ("LANE_5", 0)]).await?;
    wait_dom(desktop,r#"(()=>{const previous=document.getElementById('slack-list-lanes-previous'),next=document.getElementById('slack-list-lanes-next'),active=document.activeElement;return {ok:next.disabled&&!previous.disabled&&document.querySelectorAll('.slack-list-card').length===0&&active===previous,active:{tag:active?.tagName,id:active?.id},previousDisabled:previous.disabled,nextDisabled:next.disabled,cards:document.querySelectorAll('.slack-list-card').length};})()"#).await?;
    choose(desktop, "V_BOARD_EMPTY_CARD").await?;
    lanes(desktop, &[("", 1)]).await?;
    checked(desktop,r#"(()=>{const card=document.querySelector('.slack-list-card'),target=card?.querySelector('[data-board-cell][data-column-id="__card"]');target?.focus();target?.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));return {ok:card?.dataset.rowId==='R_LIST_072'&&!!target&&document.activeElement===target&&target.tabIndex===0&&!card.querySelector('[data-column-id="C_NAME"]')&&!card.querySelector('.slack-list-edit-cell')&&document.querySelectorAll('[data-board-cell][tabindex="0"]').length===1};})()"#).await?;
    choose(desktop, "V_BOARD_MULTI").await?;
    lanes(
        desktop,
        &[
            ("OPT_ENGINEERING", 2),
            ("OPT_DESIGN", 2),
            ("OPT_OPERATIONS", 0),
            ("", 67),
        ],
    )
    .await?;

    // Both rendered copies must resolve to one canonical mutation authorization.
    checked(desktop,r#"(()=>{const copies=[...document.querySelectorAll('.slack-list-card[data-row-id="R_LIST_002"] input[type=checkbox]')];if(copies.length!==2||copies.some(c=>!c.checked||c.disabled))return {ok:false};copies[0].click();copies[1].dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};})()"#).await?;
    wait_attempts(state, attempts + 1).await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"(()=>{const copies=[...document.querySelectorAll('.slack-list-card[data-row-id="R_LIST_002"] input[type=checkbox]')];return {ok:copies.length===2&&copies.every(c=>!c.checked&&!c.disabled)};})()"#).await?;
    assert_attempts(state, attempts + 1)?;

    checked(desktop,r#"(()=>{const cell=document.querySelector('.slack-list-card[data-row-id="R_LIST_002"] [data-column-id="C_NUMBER"]');window.__boardEditedCell=cell;cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'F2',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('slack-list-number-draft')?.value==='2'&&document.getElementById('slack-list-cell-editor').contains(document.activeElement)})"#).await?;
    checked(desktop,r#"(()=>{const input=document.getElementById('slack-list-number-draft');input.value='17';input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    checked(desktop,r#"(()=>{const save=document.getElementById('slack-list-editor-save');save.focus();save.click();return {ok:true};})()"#).await?;
    wait_attempts(state, attempts + 2).await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    wait_dom(desktop,r#"(()=>{const fields=[...document.querySelectorAll('.slack-list-card[data-row-id="R_LIST_002"] [data-column-id="C_NUMBER"]')];return {ok:!document.getElementById('slack-list-cell-editor')&&fields.length===2&&fields.every(c=>c.textContent.includes('17'))&&document.activeElement===window.__boardEditedCell};})()"#).await?;

    fixture::enable_board_tasks(&mut state.write());
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-view-select option[value="V_BOARD_TASKS"]')})"#).await?;
    choose(desktop, "V_BOARD_TASKS").await?;
    lanes(desktop, &[("", 70), ("true", 0)]).await?;
    checked(desktop,r#"(()=>{
      const card=document.querySelector('.slack-list-card'),field=card.querySelector('[data-column-id="C_TODO"]'),checkbox=field?.querySelector('input[type=checkbox]');
      window.__boardTask={id:card.dataset.rowId,column:'C_TODO'};
      field?.focus();field?.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true}));
      return {ok:card.querySelectorAll('[data-column-id="C_TODO"]').length===1&&!!checkbox&&!checkbox.checked&&!checkbox.disabled&&document.activeElement===card.querySelector('[data-column-id="C_NAME"]')&&document.querySelector('.slack-list-lane[data-group-key=""] .slack-list-lane-title')?.textContent.includes('Open')&&document.querySelector('.slack-list-lane[data-group-key="true"] .slack-list-lane-title')?.textContent.includes('Completed')};
    })()"#).await?;
    checked(desktop,r#"(()=>{const cell=document.querySelector('.slack-list-card [data-column-id="C_TODO"]');cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:' ',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_attempts(state, attempts + 3).await?;
    fixture::complete_pending(&mut state.write());
    crate::slack_lists::refresh(state);
    lanes(desktop, &[("", 69), ("true", 1)]).await?;
    wait_dom(desktop,r#"(()=>{const card=document.querySelector('.slack-list-lane[data-group-key="true"] .slack-list-card'),cell=card?.querySelector('[data-column-id="C_TODO"]');return {ok:card?.dataset.rowId===window.__boardTask.id&&cell?.querySelector('input')?.checked&&!cell.querySelector('input').disabled&&document.activeElement===cell};})()"#).await?;
    let completion = capture_stage(desktop, "board-completion").await?;
    assert_attempts(state, attempts + 3)?;
    if !fixture::prepared_valid(&state.peek()) {
        return Err("Board mutation lost canonical cell request identity".into());
    }
    choose(desktop, "").await?;
    fixture::restore_board_views(&mut state.write(), snapshot);
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-grid')&&!document.querySelector('.slack-list-board')&&document.querySelector('.slack-list-grid tbody tr[data-row-id]')?.cells.length===11})"#).await?;
    Ok(
        json!({"laneOrder":true,"keyboardHelp":keyboard_help,"duplicateIdentity":true,"primaryFields":true,"emptyLanes":true,
        "filterMembership":true,"checkboxGroups":true,"keyboard":true,"horizontalScroll":true,
        "boundedPaging":true,"pagingFocus":true,"sameLanePagingFocus":true,"emptyLanePageFocus":true,"emptyCardFocus":true,"canonicalDuplicateWrite":true,"typedEditor":true,
        "headingCompletion":true,"completionFocus":true,"cleanup":true,
        "captures":{"multi":multi,"paging":paging,"completion":completion}}),
    )
}

async fn lanes(
    desktop: &dioxus::desktop::DesktopContext,
    expected: &[(&str, usize)],
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const lanes=[...document.querySelectorAll('.slack-list-board > .slack-list-lane')],actual=lanes.map(l=>[l.dataset.groupKey,Number(l.dataset.count)]);return {{ok:JSON.stringify(actual)===JSON.stringify({})&&lanes.length<=4&&lanes.every(l=>l.querySelectorAll('.slack-list-card').length<=10)&&document.querySelectorAll('.slack-list-card').length<=40,actual,notices:document.querySelector('.slack-list-notices')?.textContent}};}})()"#,json!(expected))).await?;
    Ok(())
}

async fn choose(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let before=checked(desktop,&format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');if(!select||!dialog)return {{ok:false}};const value=select.value,layout=dialog.dataset.layoutGeneration;select.value={};select.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true,value,layout}};}})()"#,json!(id))).await?;
    wait_dom(desktop,&format!(r#"(()=>{{const select=document.getElementById('slack-list-view-select'),dialog=select?.closest('.slack-list-dialog');return {{ok:select?.value==={}&&!!dialog&&({}==={}||dialog.dataset.layoutGeneration!=={})}};}})()"#,json!(id),before["value"],json!(id),before["layout"])).await?;
    if !id.is_empty() {
        wait_dom(desktop,r#"(()=>{const board=document.querySelector('.slack-list-board'),fields=board?.querySelectorAll('[data-board-cell]').length??0,stops=board?.querySelectorAll('[data-board-cell][tabindex="0"]').length??0;return {ok:board?.dataset.keyboardMounted==='true'&&stops===(fields?1:0),keyboardMounted:board?.dataset.keyboardMounted,fields,stops,selected:document.getElementById('slack-list-view-select')?.value};})()"#).await?;
    }
    Ok(())
}
