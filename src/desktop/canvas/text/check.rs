//! Disconnected native proof of draft preservation and one-attempt text saves.
mod formatted_title;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if super::fixture::formatted_title::requested() {
        formatted_title::start(state, desktop);
        return;
    }
    if std::env::var("SUPER_PLATINUM_CANVAS_TEXT_CHECK").as_deref() != Ok("1")
        || !super::fixture::requested()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-text-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    initial_load_recovery(state, desktop).await?;
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] .canvas-text-open:not(:disabled)')})"#).await?;
    if !super::fixture::enabled(&state.peek()) {
        return Err("Expected disconnected text fixture".into());
    }
    checked(desktop,r#"(()=>{const d=document.querySelector('.canvas-dialog');return {ok:d.textContent.includes('This unsupported formatting stays readable.')&&!d.querySelector('[data-canvas-text-section="unsupported"] .canvas-text-open')};})()"#).await?;
    open(desktop).await?;
    checked(desktop,r#"(()=>{const b=window.superPlatinumRichEditor.read('canvas-text-draft');window.__canvasTextSeed=JSON.stringify(b);return {ok:!document.querySelector('#canvas-text-editor .format-toolbar [aria-label="Bulleted list"],#canvas-text-editor .format-toolbar [aria-label="Numbered list"]')&&b[0].elements.length===1&&b[0].elements[0].elements.some(e=>e.style?.bold&&e.text==='bold')&&JSON.stringify(b).includes('🦊')};})()"#).await?;
    set_draft(
        desktop,
        "<div><strong>Keep this</strong></div><div>Second paragraph</div>",
    )
    .await?;
    click(desktop, "#canvas-text-save").await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');return {ok:!!document.querySelector('#canvas-text-editor > [role=alert]')&&e?.isContentEditable&&e.textContent==='Keep thisSecond paragraph'&&window.superPlatinumRichEditor.read(e.id)[0].elements.length===2};})()"#).await?;
    editor_layout(desktop).await?;
    no_writes(&state)?;
    capture_stage(state, "unsupported-draft", 0).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    no_writes(&state)?;
    open(desktop).await?;
    checked(desktop,r#"({ok:JSON.stringify(window.superPlatinumRichEditor.read('canvas-text-draft'))===window.__canvasTextSeed})"#).await?;
    set_draft(
        desktop,
        "<div>A <strong>bold</strong> line 🦊 日本語 &amp; details. <em>Added</em></div>",
    )
    .await?;
    let team = state.peek().core.active_team.clone().unwrap();
    if !super::checklist::invalidate(&mut state.write(), &team, "Ffixture") {
        return Err("Stale fixture did not invalidate".into());
    }
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');return {ok:document.getElementById('canvas-text-save')?.disabled&&e?.textContent.includes('Added')&&!document.getElementById('canvas-text-cancel')?.disabled};})()"#).await?;
    checked(
        desktop,
        r#"(()=>{document.getElementById('canvas-text-save').click();return {ok:true};})()"#,
    )
    .await?;
    editor_layout(desktop).await?;
    no_writes(&state)?;
    capture_stage(state, "stale-draft", 1).await?;
    click(desktop, "#canvas-text-cancel").await?;
    super::fixture::refresh(&mut state.write());
    open(desktop).await?;
    set_draft(
        desktop,
        "<div>A <strong>bold</strong> line 🦊 日本語 &amp; details. <em>Added</em></div>",
    )
    .await?;
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
    // While canonical state is stale, the mapping worker clears old action
    // authority. The neighboring item can therefore be an inert checkbox span
    // rather than a disabled button; both must remain present and non-actionable.
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft'),row=document.querySelector('[data-canvas-section="item"]'),controls=row?.querySelectorAll('[role="checkbox"]'),c=controls?.length===1?controls[0]:null;const frozen=e?.contentEditable==='false',saveDisabled=document.getElementById('canvas-text-save')?.disabled===true,cancelDisabled=document.getElementById('canvas-text-cancel')?.disabled===true,checkboxDisabled=!!c&&c.isConnected&&c.getAttribute('aria-checked')==='false'&&((c.tagName==='BUTTON'&&c.disabled===true)||(c.tagName==='SPAN'&&c.getAttribute('aria-disabled')==='true'))&&!row.querySelector('button:not(:disabled)');return {ok:frozen&&saveDisabled&&cancelDisabled&&checkboxDisabled,frozen,saveDisabled,cancelDisabled,checkboxDisabled,checkboxTag:c?.tagName??null};})()"#).await?;
    if super::fixture::attempts(&state.peek()) != 1 {
        return Err("Duplicate Save dispatched more than once".into());
    }
    capture_stage(state, "saving", 2).await?;
    if !super::fixture::complete(&mut state.write()) {
        return Err("Text receipt failed to complete".into());
    }
    wait(desktop,r#"(()=>{const p=document.querySelector('[data-canvas-text-section="paragraph"]');return {ok:!document.getElementById('canvas-text-editor')&&p?.textContent.includes('Added')&&!p.querySelector('button')?.disabled};})()"#).await?;
    let canonical = super::checklist::presentation(&state.peek())
        .unwrap()
        .snapshot
        .unwrap();
    let draft = super_platinum_core::canvas::edit::text::TextIntent::capture(
        &canonical,
        "paragraph",
        super::checklist::save::now_usec().unwrap(),
    )
    .map_err(|_| "Fresh canonical text unsupported")?;
    if !draft
        .seed()
        .runs
        .iter()
        .any(|run| run.style.bold && run.text == "bold")
        || !draft
            .seed()
            .runs
            .iter()
            .any(|run| run.style.italic && run.text == "Added")
        || canonical.section("item").unwrap().checked().unwrap() != Some(false)
    {
        return Err("Canonical formatting or untouched checkbox changed".into());
    }
    capture_stage(state, "confirmed", 3).await?;
    open(desktop).await?;
    set_draft(
        desktop,
        "<div>A <strong>bold</strong> line 🦊 日本語 &amp; details. <em>Added</em> again</div>",
    )
    .await?;
    click(desktop, "#canvas-text-save").await?;
    wait(
        desktop,
        r#"({ok:document.getElementById('canvas-text-cancel')?.disabled===true})"#,
    )
    .await?;
    if super::fixture::attempts(&state.peek()) != 2
        || !super::fixture::uncertain(&mut state.write())
    {
        return Err("Second attempt did not enter uncertain state".into());
    }
    wait(desktop,r#"({ok:document.querySelector('.canvas-dialog')?.textContent.includes('Could not confirm')||document.querySelector('.canvas-dialog')?.textContent.includes('not confirmed')})"#).await?;
    checked(desktop,r#"(()=>{document.getElementById('canvas-text-save').click();document.getElementById('canvas-text-cancel').click();return {ok:document.getElementById('canvas-text-draft').textContent.includes('again')};})()"#).await?;
    if super::fixture::attempts(&state.peek()) != 2 {
        return Err("Uncertain request was replayed".into());
    }
    let layout = editor_layout(desktop).await?;
    capture_stage(state, "uncertain", 4).await?;
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        return Err("Fixture acquired real transport".into());
    }
    Ok(
        json!({"ok":true,"attempts":2,"initialLoadRecovery":true,"boundedPairedReads":3,"latestPairWins":true,"cancelNoWrite":true,"unsupportedPreserved":true,"staleDraftPreserved":true,"duplicateBlocked":true,"canonicalFormatting":true,"uncertainNoReplay":true,"disconnected":true,"noSession":true,"noTransport":true,"layout":layout}),
    )
}
async fn initial_load_recovery(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<(), String> {
    use crate::canvas::loader::fixture as load_fixture;
    async fn phase(state: Signal<ShellState>, expected: u8) -> Result<(), String> {
        for _ in 0..80 {
            if load_fixture::status(&state.peek()).is_some_and(|(phase, _, _)| phase == expected) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Err(format!("Initial load phase {expected} did not complete"))
    }
    phase(state, 0).await?;
    wait(desktop,r#"({ok:!!document.querySelector('.canvas-skeleton')&&!document.querySelector('.canvas-document')})"#).await?;
    for step in 1..=2 {
        if !load_fixture::queue(&mut state.write(), step) {
            return Err("Could not release initial read pair".into());
        }
        phase(state, step).await?;
        wait(desktop,r#"(()=>{const d=document.querySelector('.canvas-dialog');return {ok:d?.querySelector('header h2')?.textContent==='First-load preview'&&d.querySelector('.canvas-document')?.textContent.includes('First-load preview')&&!d.querySelector('.canvas-text-open:not(:disabled),button.canvas-checkbox:not(:disabled)')&&![...d.querySelectorAll('.canvas-toolbar button')].some(b=>b.textContent==='Rename'&&!b.disabled)};})()"#).await?;
        let view =
            super::checklist::presentation(&state.peek()).ok_or("Initial view disappeared")?;
        if view.snapshot.is_some() || !view.stale {
            return Err("Retired initial pair granted canonical authority".into());
        }
        no_writes(&state)?;
    }
    if !load_fixture::queue(&mut state.write(), 3) {
        return Err("Could not release latest read pair".into());
    }
    phase(state, 3).await?;
    wait(desktop,r#"(()=>{const d=document.querySelector('.canvas-dialog'),p=d.querySelector('[data-canvas-text-section="paragraph"]'),b=p?.querySelector('.canvas-text-open'),r=p?.getBoundingClientRect(),clip=d.querySelector('.canvas-scroll').getBoundingClientRect();return {ok:d.querySelector('header h2').textContent==='Paragraph editing verification'&&p?.textContent.includes('bold')&&p.textContent.includes('日本語')&&!b.disabled&&r.top>=clip.top-1&&r.bottom<=clip.bottom+1&&!d.textContent.includes('Late obsolete export'),rect:r?[r.x,r.y,r.width,r.height]:null,clip:[clip.top,clip.bottom]};})()"#).await?;
    let view = super::checklist::presentation(&state.peek()).ok_or("Recovered view disappeared")?;
    if load_fixture::status(&state.peek()) != Some((3, 3, true))
        || view.stale
        || view.loading
        || view.snapshot.is_none()
    {
        return Err("Initial recovery did not settle exactly three bounded pairs".into());
    }
    no_writes(&state)
}
fn no_writes(state: &Signal<ShellState>) -> Result<(), String> {
    if super::fixture::attempts(&state.peek()) == 0 {
        Ok(())
    } else {
        Err("Uncommitted draft sent a request".into())
    }
}
async fn open(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(
        desktop,
        "[data-canvas-text-section=paragraph] .canvas-text-open",
    )
    .await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');let ready=false;try{ready=!!window.superPlatinumRichEditor.read(e.id)}catch{}return {ok:ready&&e.isContentEditable&&document.activeElement===e};})()"#).await?;
    editor_layout(desktop).await?;
    Ok(())
}
pub(super) async fn editor_layout(
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    // Viewport visibility alone misses clipping by the reader and modal. Do not
    // scroll here: the label, toolbar, editable field and actions must coexist.
    checked(desktop, r#"(()=>{
      const d=document.querySelector('.canvas-dialog'),sc=d.querySelector('.canvas-scroll'),e=document.getElementById('canvas-text-editor');
      const dr=d.getBoundingClientRect(),sr=sc.getBoundingClientRect();
      const clip={left:Math.max(0,dr.left,sr.left),right:Math.min(innerWidth,dr.right,sr.right),top:Math.max(0,dr.top,sr.top),bottom:Math.min(innerHeight,dr.bottom,sr.bottom)};
      const selectors=['#canvas-text-draft-label','.format-toolbar','#canvas-text-draft','#canvas-text-cancel','#canvas-text-save'];
      const bounds=selectors.map(selector=>{const n=e.querySelector(selector),r=n?.getBoundingClientRect();return {selector,visible:!!r&&r.width>0&&r.height>0&&r.left>=clip.left-1&&r.right<=clip.right+1&&r.top>=clip.top-1&&r.bottom<=clip.bottom+1,rect:r?[r.x,r.y,r.width,r.height]:null};});
      const field=e.querySelector('#canvas-text-draft'),er=e.getBoundingClientRect();
      return {ok:d.scrollWidth<=d.clientWidth+1&&sc.scrollWidth<=sc.clientWidth+1&&bounds.every(b=>b.visible)&&field.clientHeight>=46,viewport:[innerWidth,innerHeight],clip,bounds,editor:[er.x,er.y,er.width,er.height],readerScroll:sc.scrollTop};
    })()"#).await
}
pub(in crate::canvas) async fn click(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<(), String> {
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled||!e.isConnected)return {{ok:false,selector:{}}};e.scrollIntoView({{block:'nearest'}});e.focus();e.click();return {{ok:true}};}})()"#,json!(selector),json!(selector))).await?;
    Ok(())
}
pub(super) async fn set_draft(
    desktop: &dioxus::desktop::DesktopContext,
    html: &str,
) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.getElementById('canvas-text-draft');e.innerHTML={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));e.focus();return {{ok:e.isContentEditable}};}})()"#,json!(html))).await?;
    Ok(())
}
pub(in crate::canvas) async fn wait(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..80 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("Canvas did not reach expected state: {last}"))
}
pub(in crate::canvas) async fn checked(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!("Canvas assertion failed: {receipt}"))
    }
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation failed")?;
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

async fn capture_stage(
    state: Signal<ShellState>,
    stage: &str,
    ordinal: usize,
) -> Result<(), String> {
    if !super::fixture::enabled(&state.peek())
        || state.peek().core.session.is_some()
        || state.peek().core.transport.is_some()
    {
        return Err("Canvas capture requires a disconnected fixture".into());
    }
    // Older direct check invocations remain valid; the portable runner always
    // supplies a fresh private directory and requires every captured stage.
    let Some(directory) = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Canvas capture acknowledgement directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    eprintln!(
        "canvas-text-stage: {}",
        json!({"ok":true,"stage":stage,"ordinal":ordinal,"noSession":true,"noTransport":true})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Canvas capture acknowledgement task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Canvas capture acknowledgement was malformed".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Canvas capture acknowledgement could not be read".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Canvas capture acknowledgement timed out at {stage}"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
