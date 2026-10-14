//! Native checks only for the exact disconnected checklist fixture.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_CANVAS_CHECKLIST_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-checklist" | "canvas-checklist-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = run(state, &desktop).await;
        let receipt = match result {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-checklist-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    wait(
        desktop,
        r#"({ok:!!document.querySelector('[data-canvas-section="item"] button:not(:disabled)')})"#,
    )
    .await?;
    if !super::fixture::enabled(&state.peek()) || state.peek().core.session.is_some() {
        return Err("A disconnected canvas fixture is required".into());
    }
    let layout = checked(desktop, r#"(()=>{const d=document.querySelector('.canvas-dialog'),b=d.querySelector('[data-canvas-section="item"] button'),u=d.querySelector('[data-canvas-section="unmapped"] [role="checkbox"]');b.focus();return {ok:b.getAttribute('aria-checked')==='false'&&document.activeElement===b&&u.getAttribute('aria-disabled')==='true'&&u.tabIndex===0&&u.title.length>0&&d.scrollWidth<=d.clientWidth+1,viewport:[innerWidth,innerHeight],unsupportedInert:u.tagName==='SPAN'};})()"#).await?;
    checked(desktop, r#"(()=>{const b=document.querySelector('[data-canvas-section="item"] button');b.click();b.click();return {ok:true};})()"#).await?;
    wait(desktop, r#"({ok:document.querySelector('[data-canvas-section="item"] button')?.disabled&&document.querySelector('.canvas-dialog [role="status"]')?.textContent.includes('Saving')})"#).await?;
    if super::fixture::attempts(&state.peek()) != 1 {
        return Err("Duplicate toggle dispatched more than once".into());
    }
    capture_stage(state, "saving", 0).await?;
    if !super::fixture::complete(&mut state.write()) {
        return Err("Fixture save did not complete".into());
    }
    wait(desktop, r#"(()=>{const b=document.querySelector('[data-canvas-section="item"] button');return {ok:b&&!b.disabled&&b.getAttribute('aria-checked')==='true'};})()"#).await?;
    capture_stage(state, "checked", 1).await?;
    checked(desktop, r#"(()=>{document.querySelector('[data-canvas-section="item"] button').click();return {ok:true};})()"#).await?;
    wait(
        desktop,
        r#"({ok:document.querySelector('[data-canvas-section="item"] button')?.disabled})"#,
    )
    .await?;
    if super::fixture::attempts(&state.peek()) != 2 || !super::fixture::complete(&mut state.write())
    {
        return Err("Restore did not produce exactly one second request".into());
    }
    wait(desktop, r#"(()=>{const b=document.querySelector('[data-canvas-section="item"] button');return {ok:b&&!b.disabled&&b.getAttribute('aria-checked')==='false'};})()"#).await?;
    capture_stage(state, "restored", 2).await?;
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Fixture acquired real session or transport".into());
    }
    Ok(
        json!({"ok":true,"attempts":2,"duplicateBlocked":true,"canonicalReadback":true,"restored":true,"noSession":true,"noTransport":true,"layout":layout}),
    )
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
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
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
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
        "canvas-checklist-stage: {}",
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
