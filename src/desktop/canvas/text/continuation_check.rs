//! Mounted editor continuation on an inserted canonical base-zero paragraph.
//! Synthetic DOM input/clicks and ACKs; no trusted typing or Slack transport proof.
use super::{
    check::{checked, editor_layout, set_draft, wait},
    checklist, continuation_fixture, structure_fixture,
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::slack::api::CanvasCollabSnapshot;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_CANVAS_STRUCTURE_TEXT_CHECK").as_deref() != Ok("1")
        || !continuation_fixture::requested()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-structure-text-check: {receipt}");
    });
}
fn snapshot(state: &Signal<ShellState>) -> Result<Arc<CanvasCollabSnapshot>, String> {
    checklist::presentation(&state.peek())
        .and_then(|v| v.snapshot)
        .ok_or("Missing canonical inventory".into())
}
fn counts(state: &Signal<ShellState>, structure: usize, text: usize) -> Result<(), String> {
    if !continuation_fixture::enabled(&state.peek())
        || structure_fixture::attempts(&state.peek()) != structure
        || continuation_fixture::attempts(&state.peek()) != text
    {
        return Err("Unexpected continuation dispatch count or fixture authority".into());
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="paragraph"] [data-structure-action="after"]:not(:disabled)')})"#).await?;
    counts(&state, 0, 0)?;
    let original = snapshot(&state)?;
    stage(state, "initial", 0).await?;
    click(
        desktop,
        r#"[data-canvas-text-section="paragraph"] [data-structure-action="after"]"#,
    )
    .await?;
    wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');let ready=false;try{ready=!!window.superPlatinumRichEditor.read(e.id)}catch{}return {ok:ready&&e.isContentEditable&&document.activeElement===e};})()"#).await?;
    set_draft(desktop, "<div><strong>Inserted continuation</strong></div>").await?;
    editor_layout(desktop).await?;
    stage(state, "insert-draft", 1).await?;
    save(desktop).await?;
    counts(&state, 1, 0)?;
    stage(state, "insert-saving", 2).await?;
    if !structure_fixture::complete(&mut state.write()) {
        return Err("Insert ACK did not settle".into());
    }
    let inserted = snapshot(&state)?;
    let added: Vec<_> = inserted
        .sections
        .iter()
        .filter(|s| !original.sections.iter().any(|old| old.id() == s.id()))
        .collect();
    let [row] = added.as_slice() else {
        return Err("Expected exactly one inserted identity".into());
    };
    let id = row.id().to_owned();
    let selector = format!("[data-canvas-text-section={}]", json!(id));
    rendered(desktop, &selector, "Inserted continuation").await?;
    canonical(&original, &inserted, &id, 1000)?;
    stage(state, "inserted", 3).await?;
    let open = format!("{selector} .canvas-text-open");
    let mut last = inserted;
    let mut layout = Value::Null;
    for (index, html, plain, rtml) in [
        (
            0,
            "<div>&amp;&lt;<br>\u{1f600}<em>First edit</em></div>",
            "First edit",
            "&amp;&lt;<br>\u{1f600}<i>First edit</i>",
        ),
        (
            1,
            "<div>&amp;&lt;<br>\u{1f600}<strong>Second edit</strong></div>",
            "Second edit",
            "&amp;&lt;<br>\u{1f600}<b>Second edit</b>",
        ),
    ] {
        click(desktop, &open).await?;
        wait(desktop,r#"(()=>{const e=document.getElementById('canvas-text-draft');let ready=false;try{ready=!!window.superPlatinumRichEditor.read(e.id)}catch{}return {ok:ready&&e.isContentEditable&&document.activeElement===e};})()"#).await?;
        set_draft(desktop, html).await?;
        layout = editor_layout(desktop).await?;
        stage(
            state,
            if index == 0 {
                "first-draft"
            } else {
                "second-draft"
            },
            4 + index * 3,
        )
        .await?;
        save(desktop).await?;
        counts(&state, 1, index + 1)?;
        stage(
            state,
            if index == 0 {
                "first-saving"
            } else {
                "second-saving"
            },
            5 + index * 3,
        )
        .await?;
        if !continuation_fixture::complete(&mut state.write()) {
            return Err("Text ACK did not settle".into());
        }
        rendered(desktop, &selector, plain).await?;
        let fresh = snapshot(&state)?;
        let sequence = last.section(&id).unwrap().sequence().unwrap().unwrap() + 1000;
        canonical(&original, &fresh, &id, sequence)?;
        let actual = fresh
            .section(&id)
            .unwrap()
            .raw()
            .message(12)
            .unwrap()
            .unwrap()
            .message(1)
            .unwrap()
            .unwrap();
        if actual.string(1).unwrap() != Some(rtml) {
            return Err("Editor lost encoded text, Unicode, soft break, or marks".into());
        }

        if fresh.section(&id).unwrap().raw().message(12).unwrap()
            == last.section(&id).unwrap().raw().message(12).unwrap()
        {
            return Err("Text canonical content did not change".into());
        }
        last = fresh;
        stage(
            state,
            if index == 0 {
                "first-accepted"
            } else {
                "second-accepted"
            },
            6 + index * 3,
        )
        .await?;
    }
    click(
        desktop,
        &format!("{selector} [data-structure-action=delete]"),
    )
    .await?;
    editor_layout(desktop).await?;
    stage(state, "delete-confirmation", 10).await?;
    save(desktop).await?;
    counts(&state, 2, 2)?;
    stage(state, "delete-saving", 11).await?;
    if !structure_fixture::complete(&mut state.write()) {
        return Err("Delete ACK did not settle".into());
    }
    wait(desktop,&format!(r#"({{ok:!document.getElementById('canvas-text-editor')&&!document.querySelector({})&&!!document.querySelector('[data-canvas-text-section="paragraph"] .canvas-text-open:not(:disabled)')}})"#,json!(selector))).await?;
    let restored = snapshot(&state)?;
    if restored.sections != original.sections {
        return Err("Original raw inventory was not restored".into());
    }
    stage(state, "restored", 12).await?;
    counts(&state, 2, 2)?;
    Ok(
        json!({"ok":true,"inserted":true,"firstEdit":true,"secondEdit":true,"sealedReadbacks":true,
        "baseZeroPreserved":true,"originalRestored":true,"duplicateSafe":true,"controlsHit":true,
        "structureAttempts":2,"textAttempts":2,"noSession":true,"noTransport":true,"layout":layout}),
    )
}
fn canonical(
    original: &CanvasCollabSnapshot,
    current: &CanvasCollabSnapshot,
    id: &str,
    sequence: u64,
) -> Result<(), String> {
    if current.sections.len() != original.sections.len() + 1
        || original
            .sections
            .iter()
            .any(|s| current.section(s.id()) != Some(s))
    {
        return Err("Untouched raw inventory changed".into());
    }
    let row = current.section(id).ok_or("Inserted row disappeared")?;
    let wire = row
        .crdt()
        .map_err(|_| "Malformed CRDT")?
        .ok_or("Missing CRDT")?;
    if row.sequence().ok().flatten() != Some(sequence)
        || wire.uint(2).ok().flatten() != Some(0)
        || wire.uint(3).ok().flatten() != Some(sequence)
    {
        return Err("Canonical zero-base revision changed".into());
    }
    Ok(())
}
async fn rendered(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
    text: &str,
) -> Result<(), String> {
    wait(desktop,&format!(r#"(()=>{{const e=document.querySelector({});return {{ok:!document.getElementById('canvas-text-editor')&&e?.textContent.includes({})&&!!e.querySelector('.canvas-text-open:not(:disabled)')}};}})()"#,json!(selector),json!(text))).await?;
    visible(desktop, selector).await?;
    checked(
        desktop,
        &format!(
            "({HIT})({},false)",
            json!(format!("{selector} .canvas-text-open"))
        ),
    )
    .await?;
    Ok(())
}
async fn save(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(desktop, "#canvas-text-save").await?;
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();return {ok:b.disabled};})()"#).await?;
    Ok(())
}
pub(super) const HIT: &str = r#"(selector,activate)=>{
 const e=document.querySelector(selector),r=e?.getBoundingClientRect();
 if(!e||!e.isConnected||e.disabled||!r||r.width<=0||r.height<=0)return {ok:false};
 let clip={left:0,top:0,right:innerWidth,bottom:innerHeight};
 for(let p=e;p;p=p.parentElement){const s=getComputedStyle(p),b=p.getBoundingClientRect();
  if(s.visibility==='hidden'||s.display==='none'||Number(s.opacity)===0)return {ok:false};
  if(/auto|scroll|hidden|clip/.test(s.overflowX)){clip.left=Math.max(clip.left,b.left);clip.right=Math.min(clip.right,b.right)}
  if(/auto|scroll|hidden|clip/.test(s.overflowY)){clip.top=Math.max(clip.top,b.top);clip.bottom=Math.min(clip.bottom,b.bottom)}}
 const x=(r.left+r.right)/2,y=(r.top+r.bottom)/2,hit=document.elementFromPoint(x,y);
 const ok=r.left>=clip.left-1&&r.right<=clip.right+1&&r.top>=clip.top-1&&r.bottom<=clip.bottom+1&&!!hit&&(e===hit||e.contains(hit));
 if(ok&&activate){e.focus();e.click()}
 return {ok,rect:[r.x,r.y,r.width,r.height],clip,hit:!!hit};
}"#;
pub(super) async fn visible(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<(), String> {
    checked(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e)return {{ok:false}};e.scrollIntoView({{block:'nearest'}});window.__canvasContinuationReady=null;requestAnimationFrame(()=>requestAnimationFrame(()=>{{window.__canvasContinuationReady=e}}));return {{ok:true}};}})()"#,json!(selector))).await?;
    wait(
        desktop,
        &format!(
            r#"({{ok:window.__canvasContinuationReady===document.querySelector({})}})"#,
            json!(selector)
        ),
    )
    .await?;
    checked(desktop, &format!("({HIT})({},false)", json!(selector))).await?;
    Ok(())
}
pub(super) async fn click(
    desktop: &dioxus::desktop::DesktopContext,
    selector: &str,
) -> Result<(), String> {
    visible(desktop, selector).await?;
    checked(desktop, &format!("({HIT})({},true)", json!(selector))).await?;
    Ok(())
}

async fn stage(state: Signal<ShellState>, stage: &str, ordinal: usize) -> Result<(), String> {
    if !continuation_fixture::enabled(&state.peek())
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
        "canvas-structure-text-stage: {}",
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
