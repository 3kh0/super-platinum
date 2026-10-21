//! Exact-gated native Lists fixture checks. Never runs against a live workspace.
mod board_choice;
mod board_choice_options;
mod board_move;
mod card_extended;
mod card_order;
mod comments;
mod compose_comments;
mod group_delete;
mod lane_order;
mod refinement_save;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
mod board_creation;
mod board_views;
mod channel_edit;
mod column_precision;
mod column_rename;
mod creation;
mod date_edit;
mod date_views;
pub(crate) mod delete;
mod duplicate;
mod editor;
mod editor_hover;
mod entity_edit;
mod field_delete;
mod field_visibility;
mod find;
mod horizontal;
mod hydration;
mod keyboard_help;
mod link_edit;
mod number_rename;
mod record_detail;
mod saved_views;
mod subitem;
mod task_assignment;
pub(crate) mod task_date;
mod text_field_create;
pub(crate) mod todo;
mod user_edit;
mod view_create;
mod view_filter;
mod view_grouping;
mod view_order;
mod view_rename;
mod view_sort;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    comments::start(state, desktop.clone());
    compose_comments::start(state, desktop.clone());
    board_move::start(state, desktop.clone());
    lane_order::start(state, desktop.clone());
    refinement_save::start(state, desktop.clone());
    group_delete::start(state, desktop.clone());
    board_choice::start(state, desktop.clone());
    board_choice_options::start(state, desktop.clone());
    card_order::start(state, desktop.clone());
    card_extended::start(state, desktop.clone());
    find::start(state, desktop.clone());
    field_delete::start(state, desktop.clone());
    view_order::start(state, desktop.clone());
    todo::start(state, desktop.clone());
    task_assignment::start(state, desktop.clone());
    task_date::start(state, desktop.clone());
    field_visibility::start(state, desktop.clone());
    view_sort::start(state, desktop.clone());
    view_filter::start(state, desktop.clone());
    view_grouping::start(state, desktop.clone());
    text_field_create::start(state, desktop.clone());
    hydration::start(state, desktop.clone());
    column_rename::start(state, desktop.clone());
    column_precision::start(state, desktop.clone());
    number_rename::start(state, desktop.clone());
    view_rename::start(state, desktop.clone());
    view_create::start(state, desktop.clone());
    subitem::start(state, desktop.clone());
    board_creation::start(state, desktop.clone());
    if std::env::var("SUPER_PLATINUM_SLACK_LIST_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list" | "slack-list-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let receipt = match run(state, &desktop).await {
            Ok(receipt) => receipt,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("slack-list-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.transport.is_some() || !super::fixture::enabled(&state.peek()) {
        return Err("Native List check requires a disconnected fixture".into());
    }
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('.slack-list-grid tbody tr').length===50&&document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})"#,
    )
    .await?;
    let initial=checked(desktop, r#"(()=>{
      const dialog=document.querySelector('.slack-list-dialog'),grid=dialog?.querySelector('table[role=treegrid]');
      const rows=[...grid.querySelectorAll('tbody tr')],cells=[...rows[0].cells],check=grid.querySelector('input[type=checkbox]:not(:disabled)');
      window.__listInitial={row:rows[0],cell:cells[0],checkbox:check,id:rows[0].dataset.rowId};
      const named=!!document.getElementById(dialog.getAttribute('aria-labelledby'))?.textContent;
      const indexed=rows.every((r,i)=>Number(r.getAttribute('aria-rowindex'))===i+2&&[...r.cells].every((c,j)=>Number(c.getAttribute('aria-colindex'))===j+1));
      return {ok:named&&indexed&&rows.length===50&&cells.length===11&&Number(grid.getAttribute('aria-rowcount'))===72&&check?.getAttribute('aria-label')?.length>0&&dialog.textContent.includes('All loaded items')&&dialog.querySelector('.slack-list-pagination').textContent.includes('73 loaded'),rows:rows.length,columns:cells.length};
    })()"#).await?;
    let keyboard_help = keyboard_help::run(desktop, "table").await?;
    let horizontal = horizontal::run(desktop).await?;
    checked(desktop, r#"(()=>{
      const first=document.querySelector('.slack-list-grid tbody td');first.focus();
      first.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));
      const right=document.activeElement;right.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
      const down=document.activeElement;down.dispatchEvent(new KeyboardEvent('keydown',{key:'Home',ctrlKey:true,bubbles:true,cancelable:true}));
      return {ok:right.cellIndex===1&&down.cellIndex===1&&down.parentElement.rowIndex===2&&document.activeElement===first&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1,right:{tag:right.tagName,column:right.cellIndex},down:{tag:down.tagName,column:down.cellIndex,row:down.parentElement.rowIndex},active:{tag:document.activeElement?.tagName,id:document.activeElement?.id},layout:{hidden:first.closest(".slack-list-layout")?.getAttribute("hidden"),inert:first.closest(".slack-list-layout")?.getAttribute("inert")}};
    })()"#).await?;
    let rich=checked(desktop,r#"(()=>{
      const row=document.querySelector('[data-row-id="R_LIST_000"]');
      const rich=row.querySelector('[data-column-id="C_RICH"]'),bold=rich?.querySelector('.bold'),italic=rich?.querySelector('.italic'),link=rich?.querySelector('a');
      const choice=row.querySelector('[data-column-id="C_SELECT"] .slack-list-option');
      const user=row.querySelector('[data-column-id="C_USER"] .mention-chip'),channel=row.querySelector('[data-column-id="C_CHANNEL"] .channel-chip');
      const reference=row.querySelector('[data-column-id="C_LINK"] a');
      const styled=bold?.textContent==='Ready to ship'&&Number(getComputedStyle(bold).fontWeight)>=600&&italic?.textContent.trim()==='review'&&getComputedStyle(italic).fontStyle==='italic';
      const labeled=choice?.textContent==='In progress'&&user?.textContent==='@Maya Chen'&&channel?.textContent==='#general';
      return {ok:styled&&labeled&&link?.textContent==='Spec'&&link?.getAttribute('href')==='https://example.com/spec'&&link.tabIndex===-1&&reference?.textContent==='Project reference'&&reference?.getAttribute('href')==='https://example.com/reference',styled,labeled,choice:choice?.textContent,user:user?.textContent,channel:channel?.textContent,reference:reference?.textContent};
    })()"#).await?;
    checked(desktop,r#"(()=>{
      const row=document.querySelector('[data-row-id="R_LIST_020"]');
      return {ok:row?.querySelector('[data-tree-primary=true] .slack-list-cell-text')?.textContent==='Untitled item'&&row?.querySelector('[data-column-id="C_CHECK"] input')?.getAttribute('aria-label')==='Done, Untitled item'};
    })()"#).await?;
    let rich_capture = capture_columns(desktop, "rich", &["C_RICH", "C_SELECT"]).await?;
    let references_capture =
        capture_columns(desktop, "references", &["C_USER", "C_CHANNEL"]).await?;
    checked(desktop,r#"(()=>{
      const cell=document.querySelector('[data-row-id="R_LIST_000"] [data-column-id="C_RICH"]');
      window.__listRichCell=cell;cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));
      return {ok:document.activeElement===cell.querySelector('a')&&cell.hasAttribute('data-escape-local')};
    })()"#).await?;
    checked(desktop,r#"(()=>{document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('.slack-list-dialog')&&document.activeElement===window.__listRichCell&&!window.__listRichCell.hasAttribute('data-escape-local')&&window.__listRichCell.querySelector('a').tabIndex===-1})"#).await?;
    checked(desktop,r#"(()=>{
      const parent=document.querySelector('[data-row-id="R_LIST_010"]'),cell=parent.querySelector('[data-tree-primary=true]');
      window.__listParentCell=cell;
      const collapsed=parent.getAttribute('aria-expanded')==='false'&&!document.querySelector('[data-row-id="R_LIST_011"]');
      cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));
      return {ok:collapsed};
    })()"#).await?;
    wait_dom(desktop,r#"(()=>{
      const parent=window.__listParentCell.closest('tr'),child=document.querySelector('[data-row-id="R_LIST_011"]'),second=document.querySelector('[data-row-id="R_LIST_012"]');
      return {ok:parent.getAttribute('aria-expanded')==='true'&&child?.getAttribute('aria-level')==='2'&&second?.getAttribute('aria-level')==='2'&&parseFloat(getComputedStyle(child.querySelector('.slack-list-tree-cell')).paddingInlineStart)>0&&document.querySelectorAll('.slack-list-grid tbody tr').length===50};
    })()"#).await?;
    let hierarchy_capture = capture_hierarchy(desktop).await?;
    checked(desktop,r#"(()=>{
      window.__listParentCell.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));
      const child=document.querySelector('[data-row-id="R_LIST_011"] [data-tree-primary=true]'),entered=document.activeElement===child;
      child.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true,cancelable:true}));
      return {ok:entered&&document.activeElement===window.__listParentCell};
    })()"#).await?;
    super::refresh(state);
    wait_dom(desktop,r#"({ok:window.__listParentCell.isConnected&&window.__listParentCell.closest('tr').getAttribute('aria-expanded')==='true'&&!!document.querySelector('[data-row-id="R_LIST_012"]')})"#).await?;
    checked(
        desktop,
        r#"(()=>{
      document.querySelector('[data-row-id="R_LIST_012"] [data-tree-primary=true]').focus();
      window.__listParentCell.querySelector('.slack-list-tree-toggle').click();return {ok:true};
    })()"#,
    )
    .await?;
    wait_dom(desktop,r#"({ok:window.__listParentCell.closest('tr').getAttribute('aria-expanded')==='false'&&!document.querySelector('[data-row-id="R_LIST_011"]')&&!document.querySelector('[data-row-id="R_LIST_012"]')&&document.activeElement===window.__listParentCell&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1})"#).await?;
    checked(desktop,r#"(()=>{window.__listParentCell.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:window.__listParentCell.closest('tr').getAttribute('aria-expanded')==='true'})"#,
    )
    .await?;
    checked(desktop,r#"(()=>{window.__listParentCell.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:window.__listParentCell.closest('tr').getAttribute('aria-expanded')==='false'&&document.querySelector('.slack-list-grid').getAttribute('aria-rowcount')==='72'&&document.querySelector('.slack-list-pagination').textContent.includes('71 shown')})"#).await?;
    assert_attempts(state, 0)?;
    checked(
        desktop,
        r#"(()=>{const b=document.getElementById('slack-list-next');b.focus();b.click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop, r#"(()=>{const rows=[...document.querySelectorAll('.slack-list-grid tbody tr')];return {ok:rows.length>0&&rows.length<=50&&rows[0].getAttribute('aria-rowindex')==='52'&&!window.__listInitial.row.isConnected&&document.activeElement===rows[0].cells[0]};})()"#).await?;
    checked(
        desktop,
        r#"(()=>{const b=document.getElementById('slack-list-previous');b.focus();b.click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop, r#"({ok:document.querySelector('.slack-list-grid tbody tr')?.dataset.rowId===window.__listInitial.id&&document.querySelectorAll('.slack-list-grid tbody tr').length===50&&document.activeElement===document.querySelector('.slack-list-grid tbody td')})"#).await?;
    checked(desktop, r#"(()=>{window.__listRefreshRow=document.querySelector('.slack-list-grid tbody tr');document.getElementById('slack-list-refresh').click();return {ok:true};})()"#).await?;
    wait_dom(desktop, r#"({ok:!document.querySelector('.slack-list-scroll')?.getAttribute('aria-busy')?.includes('true')&&document.querySelector('.slack-list-grid tbody tr')===window.__listRefreshRow})"#).await?;
    checked(desktop,r#"(()=>{
      const row=document.querySelector('[data-row-id="R_LIST_000"]'),cell=row.querySelector('[data-column-id="C_CHECK"]'),checkbox=cell.querySelector('input');
      window.__listCheckbox=checkbox;
      const zero=[...row.querySelectorAll('.slack-list-cell-text')].some(c=>c.textContent==='0');
      const fallback=row.textContent.includes('Future field value');
      return {ok:zero&&fallback&&!checkbox.checked&&!checkbox.disabled,zero,fallback};
    })()"#).await?;
    assert_attempts(state, 0)?;
    // Space crosses the same native input change path as a pointer click; a
    // second queued change must be rejected by the controller's pending lock.
    checked(
        desktop,
        r#"(()=>{
      const checkbox=window.__listCheckbox,cell=checkbox.closest('td');cell.focus();
      cell.dispatchEvent(new KeyboardEvent('keydown',{key:' ',bubbles:true,cancelable:true}));
      checkbox.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};
    })()"#,
    )
    .await?;
    wait_attempts(state, 1).await?;
    wait_dom(desktop,r#"({ok:window.__listCheckbox.isConnected&&!window.__listCheckbox.checked&&[...document.querySelectorAll('.slack-list-grid input')].every(c=>c.disabled)&&window.__listCheckbox.closest('td').getAttribute('aria-busy')==='true'})"#).await?;
    assert_attempts(state, 1)?;
    super::fixture::complete_pending(&mut state.write());
    super::refresh(state);
    wait_dom(desktop,r#"({ok:window.__listCheckbox.isConnected&&window.__listCheckbox.checked&&!window.__listCheckbox.disabled})"#).await?;
    assert_attempts(state, 1)?;

    super::fixture::set_outcome_uncertain(&mut state.write());
    checked(
        desktop,
        r#"(()=>{window.__listCheckbox.click();return {ok:true};})()"#,
    )
    .await?;
    wait_attempts(state, 2).await?;
    wait_dom(desktop,r#"({ok:window.__listCheckbox.checked&&window.__listCheckbox.disabled&&window.__listCheckbox.closest('td').getAttribute('aria-busy')==='true'})"#).await?;
    super::fixture::complete_pending(&mut state.write());
    wait_dom(desktop,r#"({ok:window.__listCheckbox.disabled&&window.__listCheckbox.closest('td').textContent.includes('Save unconfirmed')})"#).await?;
    checked(desktop,r#"(()=>{window.__listCheckbox.dispatchEvent(new Event('change',{bubbles:true}));return {ok:true};})()"#).await?;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_attempts(state, 2)?;
    super::refresh(state);
    wait_dom(desktop,r#"({ok:!window.__listCheckbox.checked&&!window.__listCheckbox.disabled&&!window.__listCheckbox.closest('td').textContent.includes('Save unconfirmed')})"#).await?;
    assert_attempts(state, 2)?;

    super::fixture::set_read_only(&mut state.write());
    super::refresh(state);
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-read-only')?.textContent.length>0&&[...document.querySelectorAll('.slack-list-grid input')].every(c=>c.disabled)})"#).await?;
    guidance(desktop, "locked").await?;
    let guidance_locked = capture_stage(desktop, "guidance-locked").await?;
    super::fixture::set_incomplete(&mut state.write());
    super::refresh(state);
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-notices')?.textContent.includes('Only part of this list')&&document.querySelectorAll('.slack-list-grid tbody tr').length===50})"#).await?;
    super::fixture::set_error(&mut state.write());
    super::refresh(state);
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-error')?.textContent.includes('Could not refresh')&&document.querySelector('[data-row-id="R_LIST_000"]')===window.__listRefreshRow&&window.__listCheckbox.disabled})"#).await?;
    guidance(desktop, "stale").await?;
    let guidance_stale = capture_stage(desktop, "guidance-stale").await?;
    super::fixture::restore(&mut state.write());
    super::refresh(state);
    wait_dom(desktop,r#"({ok:!document.querySelector('.slack-list-error')&&!document.querySelector('.slack-list-read-only')&&!document.querySelector('.slack-list-notices')?.textContent.includes('Only part of this list')&&!window.__listCheckbox.disabled&&!window.__listCheckbox.checked})"#).await?;
    guidance(desktop, "restored").await?;
    let guidance_restored = capture_stage(desktop, "guidance-restored").await?;
    assert_attempts(state, 2)?;
    if !super::fixture::prepared_valid(&state.peek()) {
        return Err(
            "Checkbox request did not preserve the canonical cell or single-attempt policy".into(),
        );
    }
    let editor_checks = editor::run(state, desktop).await?;
    checked(desktop,r#"(()=>{
      const cell=document.querySelector('[data-row-id="R_LIST_040"] [data-column-id="C_NUMBER"]');
      cell.focus();window.__listReorderCell=cell;
      return {ok:document.activeElement===cell&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1};
    })()"#).await?;
    super::fixture::reorder_rows(&mut state.write());
    wait_dom(desktop,r#"(()=>{
      const cell=window.__listReorderCell,active=document.activeElement;
      const first=document.querySelector('.slack-list-grid tbody tr')?.dataset.rowId;
      const stops=[...document.querySelectorAll('.slack-list-grid td[tabindex="0"]')].map(c=>[c.parentElement.dataset.rowId,c.dataset.columnId]);
      return {ok:first==='R_LIST_072'&&active===cell&&cell.isConnected&&cell.tabIndex===0&&stops.length===1,
        first,active:{tag:active?.tagName,id:active?.id,row:active?.closest('tr')?.dataset.rowId,column:active?.dataset.columnId},
        retained:{connected:cell.isConnected,tabIndex:cell.tabIndex,row:cell.parentElement?.dataset.rowId,column:cell.dataset.columnId},stops};
    })()"#).await?;
    super::fixture::remove_row(&mut state.write(), "R_LIST_040");
    wait_dom(desktop,r#"(()=>{
      const first=document.querySelector('.slack-list-grid tbody td');
      return {ok:!window.__listReorderCell.isConnected&&document.activeElement===first&&first.tabIndex===0&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1};
    })()"#).await?;
    let layout = checked(desktop, LAYOUT).await?;
    let saved_view_checks = saved_views::run(state, desktop).await?;
    let date_view_checks = date_views::run(state, desktop).await?;
    let board_view_checks = board_views::run(state, desktop).await?;
    let record_detail_checks = record_detail::run(state, desktop).await?;
    let date_edit_checks = date_edit::run(state, desktop).await?;
    let user_edit_checks = user_edit::run(state, desktop).await?;
    let channel_edit_checks = channel_edit::run(state, desktop).await?;
    let link_edit_checks = link_edit::run(state, desktop).await?;
    let creation_checks = creation::run(state, desktop).await?;
    let duplicate_checks = duplicate::run(state, desktop).await?;
    Ok(
        json!({"ok":true,"boundedRows":true,"semanticGrid":true,"keyboardHelp":keyboard_help,"horizontal":horizontal,"keyboardMovement":true,"richCellPresentation":rich,"emptyPrimaryFallback":true,"cellControlsReturnFocus":true,"hierarchyExpansion":true,"hierarchyKeyboard":true,"collapseRecoversParentFocus":true,"expansionSurvivesRefresh":true,"localPaging":true,"pagingFocusRecovered":true,"reorderPreservedFocus":true,"deletedRowRecoveredFocus":true,"refreshPreservedRows":true,"falseZeroAndFallback":true,"keyboardCheckboxEdit":true,"duplicateLocked":true,"pendingRestoresCanonicalCheckbox":true,"uncertainReadbackWithoutReplay":true,"readOnlyIncompleteAndErrorRecovery":true,"guidanceCaptures":{"locked":guidance_locked,"stale":guidance_stale,"restored":guidance_restored},"cellEditors":editor_checks,"savedViews":saved_view_checks,"dateViews":date_view_checks,"boardViews":board_view_checks,"recordDetails":record_detail_checks,"dateEdits":date_edit_checks,"userEdits":user_edit_checks,"channelEdits":channel_edit_checks,"linkEdits":link_edit_checks,"creation":creation_checks,"duplication":duplicate_checks,"preparedWrites":super::fixture::attempt_count(&state.peek()),"captures":{"rich":rich_capture,"references":references_capture,"hierarchy":hierarchy_capture},"initial":initial,"layout":layout}),
    )
}

async fn capture_columns(
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
    columns: &[&str],
) -> Result<Option<String>, String> {
    if std::env::var_os("SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR").is_none() {
        return Ok(None);
    }
    let columns = json!(columns);
    checked(desktop, &format!(r#"(()=>{{
      const columns={columns},s=document.querySelector('.slack-list-scroll'),row=document.querySelector('[data-row-id="R_LIST_000"]');
      const cell=[...row.cells].find(c=>c.dataset.columnId===columns[0]);s.scrollTop=0;
      const pin=row.querySelector('[data-pinned="true"]');
      const clearance=pin&&cell.cellIndex>pin.cellIndex?pin.getBoundingClientRect().width:0;
      s.scrollLeft+=cell.getBoundingClientRect().left-s.getBoundingClientRect().left-clearance;return {{ok:true}};
    }})()"#)).await?;
    wait_dom(desktop, &format!(r#"(()=>{{
      const columns={columns},s=document.querySelector('.slack-list-scroll'),bounds=s.getBoundingClientRect();
      const row=document.querySelector('[data-row-id="R_LIST_000"]');
      const cells=columns.map(id=>[...row.cells].find(c=>c.dataset.columnId===id));
      return {{ok:cells.every(cell=>{{const r=cell.getBoundingClientRect();return r.left>=Math.max(bounds.left,row.querySelector('[data-pinned="true"]')?.getBoundingClientRect().right||bounds.left)-1&&r.right<=bounds.right+1&&r.top>=bounds.top&&r.bottom<=bounds.bottom;}})}};
    }})()"#)).await?;
    capture_stage(desktop, stage).await
}

async fn capture_hierarchy(
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Option<String>, String> {
    if std::env::var_os("SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR").is_none() {
        return Ok(None);
    }
    checked(desktop,r#"(()=>{
      const s=document.querySelector('.slack-list-scroll'),parent=document.querySelector('[data-row-id="R_LIST_010"]');
      const header=document.querySelector('.slack-list-grid thead');s.scrollLeft=0;
      s.scrollTop+=parent.getBoundingClientRect().top-s.getBoundingClientRect().top-header.getBoundingClientRect().height-8;
      return {ok:true};
    })()"#).await?;
    wait_dom(desktop,r#"(()=>{
      const s=document.querySelector('.slack-list-scroll').getBoundingClientRect(),header=document.querySelector('.slack-list-grid thead').getBoundingClientRect();
      const rows=['R_LIST_010','R_LIST_011','R_LIST_012'].map(id=>document.querySelector(`[data-row-id="${id}"]`));
      return {ok:rows.every(row=>{const cell=row?.querySelector('[data-tree-primary=true]'),r=cell?.getBoundingClientRect();return r&&r.left>=s.left-1&&r.right<=s.right+1&&r.top>=header.bottom&&r.bottom<=s.bottom;})};
    })()"#).await?;
    capture_stage(desktop, "hierarchy").await
}

async fn capture_stage(
    desktop: &dioxus::desktop::DesktopContext,
    stage: &str,
) -> Result<Option<String>, String> {
    let Some(directory) = std::env::var_os("SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR") else {
        return Ok(None);
    };
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if !matches!(fixture.as_str(), "slack-list" | "slack-list-narrow") {
        return Err("Capture requires the exact Lists fixture".into());
    }
    checked(desktop,r#"(()=>{window.__listCapturePaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>{window.__listCapturePaint=true;}));return {ok:true};})()"#).await?;
    wait_dom(desktop, "({ok:window.__listCapturePaint===true})").await?;
    let path = std::path::PathBuf::from(directory).join(format!("{fixture}-{stage}.png"));
    // Reuse the bounded, DPI-aware native capture rather than a platform-only
    // subprocess. Stage captures must be real PNGs on every supported host.
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn assert_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = super::fixture::attempt_count(&state.peek());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Expected {expected} checkbox attempts, got {actual}"
        ))
    }
}
async fn wait_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..40 {
        if super::fixture::attempt_count(&state.peek()) >= expected {
            return assert_attempts(state, expected);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_attempts(state, expected)
}

async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "List DOM did not settle: {last}; assertion: {script}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let result = eval(desktop, script).await?;
    if result["ok"] == true {
        Ok(result)
    } else {
        Err(format!(
            "List native assertion failed: {result}; assertion: {script}"
        ))
    }
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation request failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Native evaluation timed out")?
        .ok_or("Native evaluation callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(raw) => {
            serde_json::from_str(&raw).map_err(|_| "Invalid encoded native callback".into())
        }
        value => Ok(value),
    }
}
async fn guidance(desktop: &dioxus::desktop::DesktopContext, phase: &str) -> Result<(), String> {
    wait_dom(
        desktop,
        &READ_ONLY_GUIDANCE.replace("__PHASE__", &json!(phase).to_string()),
    )
    .await
    .map(|_| ())
}
const READ_ONLY_GUIDANCE: &str = r#"(()=>{
 const phase=__PHASE__,d=document.querySelector('.slack-list-dialog');
 const add=d?.querySelector('#slack-list-add-item'),notice=d?.querySelector('.slack-list-read-only');
 const visible=e=>{
  if(!e)return false;
  const r=e.getBoundingClientRect(),s=getComputedStyle(e);
  return r.width>1&&r.height>1&&!e.hidden&&s.display!=='none'&&s.visibility!=='hidden'&&s.clipPath!=='inset(50%)';
 };
 const ids=(add?.getAttribute('aria-describedby')??'').trim().split(/\s+/).filter(Boolean);
 const descriptions=ids.map(id=>{
  const matches=[...document.querySelectorAll('[id]')].filter(e=>e.id===id);
  const e=matches.length===1&&d?.contains(matches[0])?matches[0]:null;
  return {id,matches:matches.length,text:e?.textContent??null,visible:visible(e)};
 });
 const reason='This List is locked.',stale='Refresh this list before adding an item.';
 const visibleReasons=[...(d?.querySelectorAll('*')||[])].filter(e=>!e.children.length&&e.textContent===reason&&visible(e));
 const accessible=ids.length>0&&new Set(ids).size===ids.length&&descriptions.every(e=>e.matches===1&&typeof e.text==='string'&&e.text.trim().length>0);
 // Restoring this fixture clears its lock, but does not certify creation limits.
 const title=phase==='stale'?stale:phase==='locked'?reason:"This List's item limit has not been verified.";
 const policy=phase==='restored'
  ?!notice&&visibleReasons.length===0&&add?.disabled===true
   &&descriptions.some(e=>e.text===title&&e.visible)
  :notice?.textContent===reason&&visibleReasons.length===1&&add?.disabled===true
   &&descriptions.some(e=>e.text===title&&(phase!=='stale'||e.visible));
 return {ok:!!add&&accessible&&add.title===title&&policy,phase,title:add?.title,
  disabled:add?.disabled,visibleReasons:visibleReasons.map(e=>e.textContent),descriptions};
})()"#;
const LAYOUT: &str = r#"(()=>{
 const d=document.querySelector('.slack-list-dialog'),s=d.querySelector('.slack-list-scroll'),r=d.getBoundingClientRect();
 const header=d.querySelector(':scope > .slack-list-header'),refreshButton=header?.querySelector('#slack-list-refresh'),closeButton=header?.querySelector('#slack-list-close');
 if(!header||!refreshButton||!closeButton)return {ok:false,error:'Original List header controls missing'};
 const original={left:header.scrollLeft,top:header.scrollTop,focus:document.activeElement};
 const separate=(a,b)=>Math.min(a.right,b.right)-Math.max(a.left,b.left)<=1||Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top)<=1;
 const initialRefresh=refreshButton.getBoundingClientRect(),initialClose=closeButton.getBoundingClientRect();
 const headerNoOverlap=separate(initialRefresh,initialClose);
 const probe=button=>{
  button.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
  button.focus({preventScroll:true});
  const rect=button.getBoundingClientRect(),h=header.getBoundingClientRect(),hit=document.elementFromPoint(rect.left+rect.width/2,rect.top+rect.height/2);
  const contained=rect.left>=h.left+header.clientLeft-1&&rect.right<=h.left+header.clientLeft+header.clientWidth+1&&rect.top>=h.top+header.clientTop-1&&rect.bottom<=h.top+header.clientTop+header.clientHeight+1;
  const bounded=[rect.x,rect.y,rect.width,rect.height].every(Number.isFinite)&&rect.width>0&&rect.height>=24&&rect.left>=r.left-1&&rect.right<=r.right+1&&rect.top>=0&&rect.bottom<=innerHeight+1;
  const noOverlap=separate(refreshButton.getBoundingClientRect(),closeButton.getBoundingClientRect());
  return {ok:contained&&bounded&&noOverlap&&document.activeElement===button&&!!hit&&(hit===button||button.contains(hit)),id:button.id,text:button.textContent.trim(),ariaLabel:button.getAttribute('aria-label'),otherRect:(button===refreshButton?closeButton:refreshButton).getBoundingClientRect().toJSON(),rect:rect.toJSON(),header:h.toJSON(),contained,bounded,noOverlap,hit:!!hit&&(hit===button||button.contains(hit)),focused:document.activeElement===button,scroll:[header.scrollLeft,header.scrollTop]};
 };
 let refreshReachable,closeReachable;
 try{refreshReachable=probe(refreshButton);closeReachable=probe(closeButton);}
 finally{header.scrollLeft=original.left;header.scrollTop=original.top;if(original.focus?.isConnected&&original.focus!==document.body)original.focus.focus({preventScroll:true});}
 const headerRestored=Math.abs(header.scrollLeft-original.left)<=1&&Math.abs(header.scrollTop-original.top)<=1&&(original.focus===document.body||document.activeElement===original.focus);
 const refresh=refreshButton.getBoundingClientRect(),close=closeButton.getBoundingClientRect();
 const vertical=s.scrollHeight>s.clientHeight;s.scrollTop=s.scrollHeight;const reachedBottom=s.scrollTop>0;
 const horizontal=s.scrollWidth>s.clientWidth;s.scrollLeft=s.scrollWidth;const reachedRight=!horizontal||s.scrollLeft>0;
 s.scrollTop=0;s.scrollLeft=0;
 const check=s.querySelector('input:not(:disabled)'),b=check.getBoundingClientRect(),hit=document.elementFromPoint(b.left+b.width/2,b.top+b.height/2);
 const bounded=r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1;
 return {ok:bounded&&headerNoOverlap&&refresh.width>=50&&refreshReachable.ok&&closeReachable.ok&&headerRestored&&vertical&&reachedBottom&&reachedRight&&hit===check&&d.scrollWidth<=d.clientWidth+1,bounded,headerNoOverlap,header:{refresh:[refresh.left,refresh.right,refresh.width],close:[close.left,close.right],refreshReachable,closeReachable,restored:headerRestored},vertical,horizontal,reachedBottom,reachedRight,viewport:[innerWidth,innerHeight],scroll:[s.clientWidth,s.scrollWidth,s.clientHeight,s.scrollHeight]};
})()"#;
