//! Owned offline interaction checks; each semantic state waits for original pixels.
use super::*;
use crate::canvas::text::check::{checked, click, editor_layout, set_draft, wait};
use std::sync::atomic::{AtomicBool, Ordering};
static STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if last_live_replacement::requested() {
        last_live_replacement::start(state, desktop);
        return;
    }
    if owner_first::requested() {
        owner_first::start(state, desktop);
        return;
    }
    if retained_pair::requested() {
        retained_pair::start(state, desktop);
        return;
    }
    if child_delete::requested() {
        child_delete::start(state, desktop);
        return;
    }
    if owner_rich::requested() {
        owner_rich::start(state, desktop);
        return;
    }
    if gap::requested() {
        gap::start(state, desktop);
        return;
    }
    if live_rich::requested() {
        live_rich::start(state, desktop);
        return;
    }
    if !requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_OWNER_DELETE_CHECK").as_deref() != Ok("1")
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-owner-delete-check: {result}");
    });
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    name: &str,
    ordinal: usize,
    geometry: Value,
) -> Result<(), String> {
    if !enabled(&state.peek()) {
        return Err("Offline owner changed".into());
    }
    let source = snapshot(&state.peek());
    let payload = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    let journal = recovery::observe().await?;
    let mut evidence = {
        let shell = state.peek();
        let f = shell
            .canvas_checklist
            .structure_fixture
            .as_ref()
            .unwrap()
            .owner_delete
            .as_ref()
            .unwrap();
        json!({"ok":true,"stage":name,"ordinal":ordinal,"coalesced":coalesced(),"noSession":true,"noTransport":true,"geometry":geometry,"canonical":payload,"journal":journal,"writes":f.writes,"reservations":f.reservations,"acknowledgements":f.acknowledgements})
    };
    if owner_first::requested() {
        owner_first::add_metadata(&mut evidence);
    } else if retained_pair::requested() {
        retained_pair::add_metadata(&mut evidence);
    } else if child_delete::requested() {
        child_delete::add_metadata(&mut evidence);
    } else if owner_rich::requested() {
        owner_rich::add_metadata(&mut evidence);
    } else if gap::requested() {
        gap::add_metadata(&mut evidence);
    } else if live_rich::requested() {
        live_rich::add_metadata(&mut evidence);
    }
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Missing private capture directory")?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let prefix = if owner_first::requested() {
        "canvas-owner-first-pair-delete"
    } else if retained_pair::requested() {
        "canvas-retained-pair-delete"
    } else if child_delete::requested() {
        "canvas-live-split-child-delete"
    } else if owner_rich::requested() {
        "canvas-coalesced-owner-rich"
    } else if gap::requested() {
        "canvas-split-gap"
    } else if live_rich::requested() {
        "canvas-live-split-rich"
    } else {
        "canvas-owner-delete"
    };
    eprintln!("{prefix}-stage: {evidence}");
    let ack = directory.join(format!("{ordinal}.ack"));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture worker failed")?
        {
            Ok(v) if v == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture ACK failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Capture ACK timed out at {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
pub(super) async fn layout(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    wait(desktop,r#"(()=>{const d=document.querySelector('.canvas-dialog'),r=d?.getBoundingClientRect();return {ok:!!r&&r.left>=-1&&r.right<=innerWidth+1&&r.top>=-1&&r.bottom<=innerHeight+1,viewport:[innerWidth,innerHeight]};})()"#).await
}
pub(super) async fn row(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focus: bool,
) -> Result<Value, String> {
    // This helper is confined to the saved single-pair fixture: both target
    // roles have the separate live neighbor, including after owner deletion.
    row_actions(desktop, id, focus, true).await
}
pub(super) async fn row_actions(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    focus: bool,
    can_delete: bool,
) -> Result<Value, String> {
    let expected = json!(id);
    let control_count = if can_delete { 4 } else { 3 };
    wait(desktop,&format!(r#"(()=>{{const id={expected},e=[...document.querySelectorAll('[data-canvas-text-section]')].find(e=>e.getAttribute('data-canvas-text-section')===id),s=e?.closest('.canvas-scroll'),r=e?.getBoundingClientRect(),c=s?.getBoundingClientRect(),buttons=[...e?.querySelectorAll('button')||[]],inside=x=>!!x&&!!c&&x.left>=Math.max(0,c.left)-1&&x.right<=Math.min(innerWidth,c.right)+1&&x.top>=Math.max(0,c.top)-1&&x.bottom<=Math.min(innerHeight,c.bottom)+1,controls=buttons.map(b=>({{text:b.textContent,enabled:!b.disabled,visible:inside(b.getBoundingClientRect())}}));return {{ok:!!r&&inside(r)&&buttons.length==={control_count}&&controls.every(c=>c.enabled&&c.visible)&&(!{focus}||e.contains(document.activeElement)),id,controls,focus:!!e?.contains(document.activeElement),rect:r?[r.x,r.y,r.width,r.height]:null,clip:c?[c.x,c.y,c.width,c.height]:null,viewport:[innerWidth,innerHeight]}};}})()"#)).await
}
// A click acknowledges event dispatch, not the following Dioxus mount. Keep
// geometry strict, but only inspect it after the owned editor is initialized.
pub(super) async fn editor_ready(
    desktop: &dioxus::desktop::DesktopContext,
    deleting: bool,
) -> Result<(), String> {
    wait(desktop, &format!(r#"(()=>{{const d=document.querySelector('.canvas-dialog'),e=document.getElementById('canvas-text-editor'),f=document.getElementById('canvas-text-draft'),c=document.getElementById('canvas-text-cancel'),s=document.getElementById('canvas-text-save');if(!d||!e||!f||!c||!s||!d.contains(e)||![f,c,s].every(n=>e.contains(n))||!e.querySelector('#canvas-text-draft-label')||!e.querySelector('.format-toolbar'))return {{ok:false}};let ready=false;try{{ready=!!window.superPlatinumRichEditor.read(f.id)}}catch{{}}return {{ok:ready&&!c.disabled&&!s.disabled&&({deleting}?(f.contentEditable==='false'&&document.activeElement===c):(f.isContentEditable&&document.activeElement===f))}};}})()"#)).await?;
    Ok(())
}

async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let phase = phase();
    if matches!(
        phase.as_str(),
        "restart" | "review" | "storage-failure" | "reviewed"
    ) {
        return recovery::run(state, desktop).await;
    }
    let owner = parent(&mode());
    wait(desktop,&format!(r#"({{ok:!!document.querySelector('[data-canvas-text-section="{owner}"] [data-structure-action="delete"]:not(:disabled)')}})"#)).await?;
    let initial = snapshot(&state.peek());
    let child = seed::child(&initial, owner);
    let initial_owner = initial.section(owner).unwrap().clone();
    let initial_child = initial.section(&child).unwrap().clone();
    let geom = row(desktop, owner, false).await?;
    stage(state, "initial", 0, geom).await?;
    let delete = format!(
        "[data-canvas-text-section={}] [data-structure-action=delete]",
        json!(owner)
    );
    click(desktop, &delete).await?;
    editor_ready(desktop, true).await?;
    editor_layout(desktop).await?;
    checked(desktop,r#"({ok:document.getElementById('canvas-text-draft')?.contentEditable==='false'&&document.activeElement?.id==='canvas-text-cancel'})"#).await?;
    click(desktop, "#canvas-text-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.getElementById('canvas-text-editor')})"#,
    )
    .await?;
    if !fixture(&mut state.write()).writes.is_empty() || recovery::stored().await?.is_some() {
        return Err("Cancel changed custody".into());
    }
    click(desktop, &delete).await?;
    editor_ready(desktop, true).await?;
    let geometry = editor_layout(desktop).await?;
    stage(state, "delete-confirmation", 1, geometry).await?;
    checked(desktop,r#"(()=>{const b=document.getElementById('canvas-text-save');b.click();b.click();return {ok:true};})()"#).await?;
    if phase == "stale" {
        wait(desktop,r#"(()=>{const root=document.getElementById('canvas-text-editor'),error=root?.querySelector(':scope > p[role="alert"]'),cancel=root?.querySelector('#canvas-text-cancel'),r=error?.getBoundingClientRect(),style=error?getComputedStyle(error):null;const visible=!!r&&r.width>0&&r.height>0&&!error.hidden&&style.display!=='none'&&style.visibility==='visible';return {ok:visible&&error.textContent==='This paragraph or your access changed. Cancel and refresh.'&&!!cancel&&!cancel.disabled,error:error?.textContent??null,cancelEnabled:!!cancel&&!cancel.disabled};})()"#).await?;
        if !fixture(&mut state.write()).writes.is_empty() || recovery::stored().await?.is_some() {
            return Err("Stale preflight dispatched".into());
        }
        stage(state, "stale", 2, editor_layout(desktop).await?).await?;
    } else {
        wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled&&document.getElementById('canvas-text-save')?.textContent.includes('Saving')})"#).await?;
        // The disabled UI may precede worker dispatch: wait on the real pending slot, not a longer UI timeout.
        for _ in 0..600 {
            if fixture(&mut state.write()).pending.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        if fixture(&mut state.write()).writes.len() != 1
            || fixture(&mut state.write()).reservations != 1
            || recovery::stored().await?.is_none()
        {
            return Err("Duplicate guard or durable dispatch failed".into());
        }
        stage(state, "delete-saving", 2, editor_layout(desktop).await?).await?;
        let unknown = matches!(phase.as_str(), "unknown" | "seed");
        let omitted = phase == "omitted";
        completion::complete(state, unknown).await?;
        if omitted {
            wait(desktop,r#"({ok:!!document.getElementById('canvas-text-editor')&&document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
            if recovery::stored().await?.is_none()
                || checklist::presentation(&state.peek())
                    .and_then(|v| v.operation)
                    .is_none()
            {
                return Err("Omitted owner incorrectly unlocked".into());
            }
            stage(state, "owner-omitted", 3, editor_layout(desktop).await?).await?;
        } else if unknown {
            wait(desktop,r#"({ok:!!document.querySelector('#canvas-text-editor')&&document.body.textContent.includes('Could not confirm')})"#).await?;
            stage(state, "uncertain", 3, editor_layout(desktop).await?).await?;
        } else {
            wait(
                desktop,
                r#"({ok:!document.getElementById('canvas-text-editor')})"#,
            )
            .await?;
            let current = snapshot(&state.peek());
            let tombstone = current.section(owner).ok_or("Owner tombstone missing")?;
            if tombstone.deleted().unwrap() != Some(true)
                || current.section(&child) != Some(&initial_child)
                || tombstone.crdt().unwrap().unwrap().messages(10).unwrap()
                    != initial_owner.crdt().unwrap().unwrap().messages(10).unwrap()
            {
                return Err("Deletion changed lineage/child or dropped tombstone".into());
            }
            if recovery::stored().await?.is_some() {
                return Err("Deletion journal not retired".into());
            }
            stage(state, "deleted", 3, row(desktop, &child, true).await?).await?;
            let edit = format!(
                "[data-canvas-text-section={}] .canvas-text-open[aria-label='Edit paragraph']",
                json!(child)
            );
            click(desktop, &edit).await?;
            editor_ready(desktop, false).await?;
            let desired = format!("{} edited", texts(&corpus()).1);
            set_draft(desktop, &format!("<div>{}</div>", escaped(&desired))).await?;
            if coalesced() {
                bold(desktop, &desired).await?;
            }
            stage(state, "child-edit", 4, editor_layout(desktop).await?).await?;
            click(desktop, "#canvas-text-save").await?;
            for _ in 0..600 {
                if fixture(&mut state.write()).pending_text.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            stage(state, "child-saving", 5, editor_layout(desktop).await?).await?;
            completion::complete_text(state).await?;
            wait(
                desktop,
                r#"({ok:!document.getElementById('canvas-text-editor')})"#,
            )
            .await?;
            let edited = snapshot(&state.peek());
            if edited.section(owner) != Some(tombstone) {
                return Err("Child edit changed tombstone".into());
            }
            let expected = if coalesced() {
                format!("<b>{}</b>", escaped(&desired))
            } else {
                escaped(&desired)
            };
            let actual = edited
                .section(&child)
                .unwrap()
                .raw()
                .message(12)
                .unwrap()
                .unwrap()
                .message(1)
                .unwrap()
                .unwrap();
            if actual.string(1).unwrap() != Some(expected.as_str()) {
                return Err("Independent child canonical formatting differs".into());
            }
            stage(
                state,
                "child-confirmed",
                6,
                row(desktop, &child, true).await?,
            )
            .await?;
        }
    }
    let source = snapshot(&state.peek());
    let final_rows = tokio::task::spawn_blocking(move || seed::saved(&source))
        .await
        .map_err(|_| "Evidence worker failed")?;
    Ok(
        json!({"ok":true,"phase":phase,"corpus":corpus(),"history":mode(),"coalesced":coalesced(),"noSession":true,"noTransport":true,"layout":layout(desktop).await?,"canonical":final_rows,"journal":recovery::observe().await?,"cancelNoWrite":true,"duplicateGuard":true,"noHttp":true}),
    )
}

// Exercise the existing toolbar handler; never inject formatted markup.
pub(super) async fn bold(
    desktop: &dioxus::desktop::DesktopContext,
    text: &str,
) -> Result<(), String> {
    let expected = json!(text);
    checked(desktop, &format!(r#"(()=>{{const e=document.getElementById('canvas-text-draft'),b=document.querySelector('#canvas-text-editor button[aria-label="Bold"]');if(!e||e.contentEditable!=='true'||!b||b.disabled||e.textContent!=={expected})return {{ok:false}};e.focus();const r=document.createRange();r.selectNodeContents(e);const s=getSelection();s.removeAllRanges();s.addRange(r);return {{ok:s.toString()==={expected}&&e.contains(s.anchorNode)&&e.contains(s.focusNode)}};}})()"#)).await?;
    checked(desktop, r#"(()=>{const b=document.querySelector('#canvas-text-editor button[aria-label="Bold"]');if(!b||b.disabled)return {ok:false};b.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true}));b.click();return {ok:true};})()"#).await?;
    wait(desktop, &format!(r#"(()=>{{const e=document.getElementById('canvas-text-draft'),b=e?.querySelector('b,strong');return {{ok:!!b&&b.textContent==={expected}&&e.textContent==={expected}&&!document.getElementById('canvas-text-save')?.disabled,html:e?.innerHTML}};}})()"#)).await?;
    Ok(())
}
