//! Disconnected geometry and keyboard checks for the actual Files component.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn fixture(shell: &mut ShellState) {
    shell.core.transport = None;
    shell.core.session = None;
    shell.toast = None;
    let originals = shell.core.files.items.clone();
    for index in 0..8 {
        let mut file = originals[index % originals.len()].clone();
        file.id = Some(format!("FDENSITY{index}"));
        file.title = Some(format!(
            "Project reference {} — release notes and review material",
            index + 1
        ));
        shell.core.files.items.push(file);
    }
}
fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_FILES_DENSITY_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("files-density" | "files-density-narrow")
        )
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("files-density-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Fixture must be disconnected".into());
    }
    wait_dom(
        desktop,
        r#"({ok:document.querySelectorAll('.files-panel .file-result').length===12})"#,
    )
    .await?;
    let mut receipt = checked(desktop, GEOMETRY).await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.files-search input');window.__filesSearch=e;e.focus();e.value='launch notes';e.dispatchEvent(new Event('input',{bubbles:true}));e.setSelectionRange(6,11);return {ok:true};})()"#).await?;
    // A normal result-count update must not replay the field's mount seed into
    // the native editor or replace its node/caret while the user is typing.
    let original_total = state.peek().core.files.total;
    state.write().core.files.total = original_total + 1;
    wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.files-pagination').textContent.includes('{} files')}})"#,original_total+1)).await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.files-search input');return {ok:e===window.__filesSearch&&e.value==='launch notes'&&e.selectionStart===6&&e.selectionEnd===11&&document.activeElement===e};})()"#).await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.files-search input');e.value='';e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    state.write().core.files.total = original_total;
    wait_dom(desktop,&format!(r#"({{ok:document.querySelector('.files-pagination').textContent.includes('{} files')}})"#,original_total)).await?;

    checked(desktop,r#"(()=>{window.__filesRows=[...document.querySelectorAll('.files-panel .file-result')];const first=window.__filesRows[0].querySelector('.file-more');first.focus();first.click();return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:!!document.querySelector('.file-action-panel')&&!document.querySelector('.file-more').disabled})"#).await?;
    checked(desktop,r#"(()=>{const panel=document.querySelector('.file-action-panel'),buttons=[...panel.querySelectorAll('button')],list=document.querySelector('.files-results'),r=panel.getBoundingClientRect(),b=list.getBoundingClientRect();const aligned=Math.abs(document.querySelector('.file-more').getBoundingClientRect().top+16-(document.querySelector('.files-row').getBoundingClientRect().top+document.querySelector('.files-row').getBoundingClientRect().height/2))<=1;const ok=buttons.length>0&&r.left>=b.left-1&&r.right<=b.right+1&&panel.scrollWidth<=panel.clientWidth+1&&aligned;buttons[0].focus();buttons[0].dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok,aligned};})()"#).await?;
    wait_dom(desktop,r#"({ok:!document.querySelector('.file-action-panel')&&document.activeElement===document.querySelector('.file-more')})"#).await?;
    checked(desktop,r#"(()=>{const rows=[...document.querySelectorAll('.file-result')],last=rows.at(-1).querySelector('.files-row');last.focus();last.scrollIntoView({block:'nearest'});return {ok:rows.every((e,i)=>e===window.__filesRows[i])};})()"#).await?;
    wait_dom(desktop,r#"(()=>{const e=document.querySelector('.file-result:last-child .files-row'),r=e.getBoundingClientRect(),b=document.querySelector('.files-results').getBoundingClientRect();return {ok:document.activeElement===e&&r.top>=b.top-1&&r.bottom<=b.bottom+1};})()"#).await?;
    checked(desktop,r#"(()=>{const list=document.querySelector('.files-results'),first=document.querySelector('.files-row');list.scrollTop=0;first.focus({preventScroll:true});return {ok:true};})()"#).await?;
    wait_dom(desktop,r#"({ok:document.querySelector('.files-results').scrollTop===0&&document.activeElement===document.querySelector('.files-row')})"#).await?;
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Fixture connected during check".into());
    }
    for key in [
        "searchDraftPreserved",
        "stableActions",
        "escapeRestoresFocus",
        "lastRowReachable",
        "rowIdentityPreserved",
        "noTransport",
    ] {
        receipt[key] = json!(true);
    }
    Ok(receipt)
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
        "Files density did not reach expected state: {last}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Files density assertion failed: {receipt}"))
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
        .ok_or("Native callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid encoded native callback".into())
        }
        value => Ok(value),
    }
}
const GEOMETRY: &str = r#"(()=>{
 const panel=document.querySelector('.files-panel'),list=panel.querySelector('.files-results'),bounds=list.getBoundingClientRect();
 const rows=[...panel.querySelectorAll('.files-row')],rects=rows.map(e=>e.getBoundingClientRect());
 const full=rects.filter(r=>r.top>=bounds.top-1&&r.bottom<=bounds.bottom+1).length;
 const controls=[...panel.querySelectorAll('.files-header button,.files-search input,.files-search button,.files-owner button,.files-selects select,.files-pagination button')];
 const controlsVisible=controls.every(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>=30&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1;});
 const noOverflow=panel.scrollWidth<=panel.clientWidth+1&&list.scrollWidth<=list.clientWidth+1&&[...panel.querySelectorAll('.files-controls,.files-search,.files-selects')].every(e=>e.scrollWidth<=e.clientWidth+1);
 const compact=rects.every(r=>r.height>=48&&r.height<=58);
 const longTitle=panel.querySelector('[data-file-id="F4"] .files-title');
 const titleAccessible=longTitle.title===longTitle.textContent&&longTitle.textContent.length>90&&getComputedStyle(longTitle).textOverflow==='ellipsis'&&(innerWidth>800||longTitle.scrollWidth>longTitle.clientWidth);
 const filtersIntact=panel.querySelectorAll('.files-owner button').length===3&&panel.querySelector('select[aria-label="File type"]').options.length===11&&panel.querySelector('select[aria-label="Sort files"]').options.length===3;
 const aligned=rows.every(row=>{const r=row.getBoundingClientRect(),a=row.closest('.file-result').querySelector('.file-more').getBoundingClientRect();return Math.abs((r.top+r.height/2)-(a.top+a.height/2))<=1;});
 return {ok:controlsVisible&&noOverflow&&compact&&titleAccessible&&filtersIntact&&aligned&&full>=3,controlsVisible,noOverflow,compactRows:compact,longTitleAccessible:titleAccessible,filtersIntact,actionsAligned:aligned,fullyVisibleRows:full,rowHeight:rects[0].height,resultsHeight:bounds.height,viewport:[innerWidth,innerHeight]};
})()"#;
