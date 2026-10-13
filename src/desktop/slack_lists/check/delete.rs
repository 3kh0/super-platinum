//! Isolated native destructive-action checks; the backend is disconnected.
use super::*;
use crate::slack_lists::{delete::fixture as backend, fixture};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_SLACK_LIST_DELETE_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list" | "slack-list-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(result) => result,
            Err(error) => {
                let dom = eval(&desktop, DIAGNOSTICS).await.unwrap_or(Value::Null);
                json!({"ok":false,"error":error,"shell":snapshot(&state.peek()),"dom":dom})
            }
        };
        eprintln!("slack-list-delete-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Delete check requires disconnected Lists fixture".into());
    }
    wait_dom(desktop,r#"(()=>{const dialog=document.querySelector('.slack-list-dialog'),rows=[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')];return {ok:dialog?.dataset.listKeyboardMounted==='true'&&rows.some(row=>row.dataset.rowId==='R_LIST_010'),dialogPresent:!!dialog,keyboardMounted:dialog?.dataset.listKeyboardMounted,rowCount:rows.length,rowIds:rows.map(row=>row.dataset.rowId)};})()"#).await?;
    checked(desktop,r#"(()=>{window.__listDeleteEvents=[];const record=e=>{window.__listDeleteEvents.push({type:e.type,key:e.type==='keydown'?e.key:undefined,id:e.target?.id,tag:e.target?.tagName,trusted:e.isTrusted});if(window.__listDeleteEvents.length>32)window.__listDeleteEvents.shift();};document.addEventListener('click',record,true);document.addEventListener('keydown',record,true);return {ok:true};})()"#).await?;
    let baseline = fixture::deletion_loaded(&state.peek())
        .ok_or("Missing baseline List")?
        .records
        .records
        .into_iter()
        .map(|record| (record.id, record.raw))
        .collect::<std::collections::BTreeMap<_, _>>();
    open(desktop, "R_LIST_010").await?;
    confirm(desktop).await?;
    let confirmation = capture_stage(desktop, "delete-confirm").await?;
    click(desktop, "#slack-list-delete-cancel").await?;
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-delete-confirm')&&document.activeElement?.id==='slack-list-record-delete'})"#).await?;
    count(state, 0)?;
    // macOS mouse activation need not focus the opener. Cancellation still
    // returns to Delete, rather than whichever field preceded the click.
    checked(desktop,r#"(()=>{document.querySelector('.slack-list-detail-field').focus();document.querySelector('#slack-list-record-delete').click();return {ok:true};})()"#).await?;
    wait_dom(
        desktop,
        r#"({ok:document.activeElement?.id==='slack-list-delete-cancel'})"#,
    )
    .await?;
    click(desktop, "#slack-list-delete-cancel").await?;
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-delete-confirm')&&document.activeElement?.id==='slack-list-record-delete'})"#).await?;
    count(state, 0)?;
    confirm(desktop).await?;
    checked(desktop,r#"(()=>{const b=document.querySelector('#slack-list-delete-confirm-button');b.click();b.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-status')?.dataset.stage==='Sending'&&!document.querySelector('#slack-list-delete-confirm')&&document.querySelector('#slack-list-record-delete')?.disabled&&document.querySelector('#slack-list-refresh')?.disabled&&[...document.querySelectorAll('#slack-list-record-detail input,#slack-list-record-detail .slack-list-edit-cell')].every(x=>x.disabled)})"#).await?;
    count(state, 1)?;
    let pending = capture_stage(desktop, "delete-pending").await?;
    let before_ack = snapshot(&state.peek());
    if before_ack["detail"] != "R_LIST_010" {
        return Err(format!(
            "Detail changed before held ACK completion: {before_ack}"
        ));
    }
    if !backend::complete(&mut state.write()) {
        return Err("Missing held delete".into());
    }
    let after_ack = snapshot(&state.peek());
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-status')?.dataset.stage==='AwaitingRead'&&document.querySelector('#slack-list-record-detail')?.dataset.recordId==='R_LIST_010',stage:document.querySelector('#slack-list-delete-status')?.dataset.stage,detail:document.querySelector('#slack-list-record-detail')?.dataset.recordId,focus:{id:document.activeElement?.id,tag:document.activeElement?.tagName}})"#).await.map_err(|error|format!("{error}; beforeACK={before_ack}; afterACK={after_ack}"))?;
    // A tombstone for a different canonical item cannot settle this receipt.
    if !backend::complete_check(&mut state.write(), true, true) {
        return Err("Missing initial result read".into());
    }
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-delete-check')?.disabled&&document.querySelector('#slack-list-record-detail')?.dataset.recordId==='R_LIST_010'})"#).await?;
    click(desktop, "#slack-list-delete-check").await?;
    wait_check(state, 2).await?;
    if !backend::complete_check(&mut state.write(), true, false) {
        return Err("Missing exact result read".into());
    }
    wait_dom(desktop,r#"({ok:!document.querySelector('#slack-list-record-detail')&&document.querySelector('#slack-list-delete-status')?.textContent.includes('Item deleted')&&!document.querySelector('.slack-list-grid [data-row-id="R_LIST_010"]')&&!!document.querySelector('.slack-list-grid [data-row-id="R_LIST_009"]')})"#).await?;
    let loaded = fixture::deletion_loaded(&state.peek()).unwrap();
    if loaded
        .records
        .records
        .iter()
        .any(|row| matches!(row.id.as_str(), "R_LIST_010" | "R_LIST_011" | "R_LIST_012"))
    {
        return Err("Synthetic cascade did not load canonical result".into());
    }
    count(state, 1)?;
    // Explicit same-ID Undo restores the parent and the fixture's known children.
    double_transition(desktop).await?;
    transition_count(state, 1)?;
    if !backend::complete(&mut state.write()) {
        return Err("Missing held Undo".into());
    }
    wait_check(state, 3).await?;
    if !backend::complete_check(&mut state.write(), false, false) {
        return Err("Missing Undo result read".into());
    }
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-undo')?.textContent==='Redo'&&!!document.querySelector('.slack-list-grid [data-row-id="R_LIST_010"]')})"#).await?;
    let restored = fixture::deletion_loaded(&state.peek()).unwrap();
    if ["R_LIST_010", "R_LIST_011", "R_LIST_012"]
        .iter()
        .any(|id| restored.records.record(id).is_none())
    {
        return Err("Undo lost a known subtask identity".into());
    }
    let restored_records = restored
        .records
        .records
        .into_iter()
        .map(|record| (record.id, record.raw))
        .collect::<std::collections::BTreeMap<_, _>>();
    if restored_records != baseline {
        return Err("Undo changed baseline record fields or left missing/extra records".into());
    }
    checked(desktop,r#"(()=>{const row=document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_010"]'),cell=row?.querySelector('[data-column-id="C_NAME"]');if(!cell)return {ok:false};cell.focus();if(row.getAttribute('aria-expanded')!=='true')cell.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const parent=document.querySelector('.slack-list-grid tr[data-row-id="R_LIST_010"]'),children=['R_LIST_011','R_LIST_012'].map(id=>document.querySelector(`.slack-list-grid tr[data-row-id="${id}"]`));return {ok:parent?.getAttribute('aria-expanded')==='true'&&children.every(row=>row&&row.getClientRects().length>0&&row.getBoundingClientRect().height>0&&!row.closest('[hidden],[inert]')),childIds:children.map(row=>row?.dataset.rowId)};})()"#).await?;
    let undo_capture = capture_stage(desktop, "delete-restored").await?;
    double_transition(desktop).await?;
    transition_count(state, 2)?;
    if !backend::complete(&mut state.write()) {
        return Err("Missing held Redo".into());
    }
    wait_check(state, 4).await?;
    if !backend::complete_check(&mut state.write(), true, false) {
        return Err("Missing Redo result read".into());
    }
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-undo')?.textContent==='Undo'&&!document.querySelector('.slack-list-grid [data-row-id="R_LIST_010"]')})"#).await?;
    if backend::restore_attempts(&state.peek()) != 2 {
        return Err("Undo/Redo sent unexpected request count".into());
    }
    open(desktop, "R_LIST_002").await?;
    backend::uncertain_next(&mut state.write());
    click(desktop, "#slack-list-record-delete").await?;
    wait_dom(
        desktop,
        r#"({ok:document.querySelector('#slack-list-delete-status')?.dataset.stage==='Sending'&&!document.querySelector('#slack-list-delete-confirm')})"#,
    )
    .await?;
    if !backend::complete(&mut state.write()) {
        return Err("Missing uncertain delete".into());
    }
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-status')?.dataset.stage==='Uncertain'&&!document.querySelector('#slack-list-delete-check')?.disabled})"#).await?;
    count(state, 2)?;
    let uncertain = capture_stage(desktop, "delete-uncertain").await?;
    click(desktop, "#slack-list-delete-check").await?;
    wait_check(state, 5).await?;
    backend::complete_check(&mut state.write(), false, false);
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-delete-status')?.textContent.includes('still visible')&&!document.querySelector('#slack-list-delete-check')?.disabled})"#).await?;
    let file = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .raw
        .clone();
    click(desktop, "#slack-list-close").await?;
    wait_dom(
        desktop,
        "({ok:!document.querySelector('.slack-list-dialog')})",
    )
    .await?;
    {
        let mut shell = state.write();
        assert!(crate::slack_lists::open_shell(
            &mut shell,
            serde_json::from_value(file).unwrap()
        ));
        fixture::refresh(&mut shell);
    }
    wait_dom(desktop,r#"({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'&&document.querySelector('#slack-list-delete-status')?.dataset.stage==='Uncertain'})"#).await?;
    open(desktop, "R_LIST_003").await?;
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-record-delete')?.disabled&&[...document.querySelectorAll('#slack-list-record-detail input,#slack-list-record-detail .slack-list-edit-cell')].every(x=>x.disabled)})"#).await?;
    click(desktop, "#slack-list-delete-check").await?;
    wait_check(state, 6).await?;
    backend::complete_check(&mut state.write(), true, false);
    wait_dom(desktop,r#"({ok:document.querySelector('#slack-list-record-detail')?.dataset.recordId==='R_LIST_003'&&!document.querySelector('#slack-list-record-delete')?.disabled&&document.querySelector('#slack-list-delete-status')?.textContent.includes('Item deleted')&&document.activeElement?.id==='slack-list-record-back'})"#).await?;
    count(state, 2)?;
    if !backend::prepared_valid(&state.peek()) {
        return Err("Delete request shape changed".into());
    }
    let layout=checked(desktop,r#"(()=>{const d=document.querySelector('.slack-list-dialog'),r=d.getBoundingClientRect();return {ok:r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&d.scrollWidth<=d.clientWidth+1,viewport:[innerWidth,innerHeight]};})()"#).await?;
    Ok(
        json!({"ok":true,"noTransport":fixture::enabled(&state.peek()),"preparedRequests":2,"deletionStateRequests":2,"undoRedoSameIds":true,"transitionDuplicatesBlocked":true,"restoredBaselineExact":true,"restoredChildrenRendered":true,"childlessImmediate":true,"cancelNoSend":true,"duplicateBlocked":true,"exactReadbackOnly":true,"uncertainNoReplay":true,"reopenLocked":true,"differentDetailPreserved":true,"noOverflow":true,"viewport":layout["viewport"],"captures":[confirmation,pending,uncertain,undo_capture]}),
    )
}
async fn double_transition(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"({ok:!!document.querySelector('#slack-list-delete-undo')&&!document.querySelector('#slack-list-delete-undo').disabled})"#).await?;
    checked(desktop,r#"(()=>{const b=document.querySelector('#slack-list-delete-undo');b.focus();b.click();b.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const b=document.querySelector('#slack-list-delete-undo');return {ok:document.querySelector('#slack-list-delete-status')?.dataset.stage==='Sending'&&(!b||b.disabled)&&document.querySelector('#slack-list-refresh')?.disabled&&document.querySelector('#slack-list-add-item')?.disabled&&[...document.querySelectorAll('.slack-list-layout input,.slack-list-layout .slack-list-edit-cell')].every(x=>x.disabled)};})()"#).await?;
    Ok(())
}
fn transition_count(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = backend::restore_attempts(&state.peek());
    if actual != expected {
        return Err(format!(
            "Undo/Redo duplicate dispatch: {actual} requests, expected {expected}"
        ));
    }
    count(state, 1)
}
async fn open(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const cell=document.querySelector('.slack-list-grid tr[data-row-id="{id}"] [data-column-id="C_NAME"]');if(!cell)return {{ok:false}};cell.focus();cell.dispatchEvent(new KeyboardEvent('keydown',{{key:'Enter',altKey:true,bubbles:true,cancelable:true}}));return {{ok:true}};}})()"#)).await?;
    wait_dom(desktop,&format!(r#"({{ok:document.querySelector('#slack-list-record-detail')?.dataset.recordId==='{id}'&&document.activeElement?.id==='slack-list-record-back'}})"#)).await?;
    Ok(())
}
async fn confirm(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#slack-list-record-delete").await?;
    wait_dom(desktop,r#"(()=>{const d=document.querySelector('#slack-list-delete-confirm'),r=d?.getBoundingClientRect();return {ok:!!d&&document.activeElement?.id==='slack-list-delete-cancel'&&d.textContent.includes('any associated subtasks')&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1&&d.scrollWidth<=d.clientWidth+1};})()"#).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const b=document.querySelector({selector:?});if(!b||b.disabled)return {{ok:false}};b.focus();b.click();return {{ok:true}};}})()"#)).await?;
    Ok(())
}
fn count(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = backend::attempts(&state.peek());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Delete request count {actual}, expected {expected}"
        ))
    }
}
async fn wait_check(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..30 {
        if backend::checks(&state.peek()) == expected {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Result read did not start".into())
}

fn snapshot(shell: &ShellState) -> Value {
    let view = crate::slack_lists::presentation(shell);
    let deletion = crate::slack_lists::delete::presentation(shell);
    json!({"visit":view.as_ref().map(|view|view.generation),"detail":view.as_ref().and_then(|view|view.detail.as_ref()).map(|detail|detail.record_id.as_str()),
        "detailOpen":view.as_ref().and_then(|view|view.detail.as_ref()).map(|detail|detail.open_generation),"stale":view.as_ref().map(|view|view.stale),
        "stage":deletion.operation.as_ref().map(|operation|format!("{:?}",operation.stage)),"checking":deletion.operation.as_ref().map(|operation|operation.checking),
        "attempts":backend::attempts(shell),"reads":backend::checks(shell)})
}
const DIAGNOSTICS: &str = r#"(()=>{const notices=document.querySelector('.slack-list-notices'),text=document.querySelector('#slack-list-delete-status p'),range=document.createRange();if(text)range.selectNodeContents(text);const bounds=notices?.getBoundingClientRect(),textRects=text?[...range.getClientRects()].map(r=>({left:r.left,right:r.right,top:r.top,bottom:r.bottom})):[];const d=document.querySelector('.slack-list-dialog'),detail=document.querySelector('#slack-list-record-detail'),active=document.activeElement;return {visit:d?.dataset.dialogGeneration,dialogPresent:!!d,keyboardMounted:d?.dataset.listKeyboardMounted,rowCount:document.querySelectorAll('.slack-list-grid tr[data-row-id]').length,rowIds:[...document.querySelectorAll('.slack-list-grid tr[data-row-id]')].map(row=>row.dataset.rowId),stage:document.querySelector('#slack-list-delete-status')?.dataset.stage,detail:detail?.dataset.recordId,detailOpen:detail?.dataset.detailOpenGeneration,focus:{id:active?.id,tag:active?.tagName},notices:{left:bounds?.left,right:bounds?.right,scrollLeft:notices?.scrollLeft,clientWidth:notices?.clientWidth,scrollWidth:notices?.scrollWidth},textRects,events:window.__listDeleteEvents||[]};})()"#;
