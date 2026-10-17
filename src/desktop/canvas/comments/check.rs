//! Actual DOM controls with synthetic, builder-recorded read responses only.
use super::*;
use serde_json::{Value, json};
mod ready;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_COMMENTS_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-comments-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    ready::wait_runner(state).await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && !document.querySelector('#canvas-comments [aria-busy=true]')").await?;
    click(desktop, "#canvas-comments-back").await?;
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-open'",
    )
    .await?;
    click(desktop, "#canvas-comments-open").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && document.activeElement?.id==='canvas-comments-back'").await?;
    let mut layouts = Vec::new();
    stage(state, desktop, "initial", 0, &mut layouts).await?;
    click(desktop, ".canvas-comment-open").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && !!document.getElementById('canvas-comments-all') && !document.querySelector('#canvas-comments [aria-busy=true]')").await?;
    stage(state, desktop, "thread", 1, &mut layouts).await?;
    click(desktop, "#canvas-comments-more").await?;
    wait(desktop,r#"document.querySelectorAll('.canvas-comment').length===1 && document.getElementById('canvas-comments').textContent.includes('\u65e5\u672c\u8a9e')"#).await?;
    stage(state, desktop, "thread-more", 2, &mut layouts).await?;
    click(desktop, "#canvas-comments-previous").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && !document.getElementById('canvas-comments-previous')").await?;
    stage(state, desktop, "previous-page", 3, &mut layouts).await?;
    click(desktop, "#canvas-comments-all").await?;
    wait(desktop,"!!document.getElementById('canvas-comments-filter') && document.querySelectorAll('.canvas-comment').length===2").await?;
    filter(desktop, "archived").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===1 && document.getElementById('canvas-comments').textContent.includes('Resolved:')").await?;
    stage(state, desktop, "resolved", 4, &mut layouts).await?;
    filter(desktop, "unread").await?;
    wait(
        desktop,
        "document.getElementById('canvas-comments-empty')?.textContent==='No unread comments'",
    )
    .await?;
    stage(state, desktop, "unread", 5, &mut layouts).await?;
    fixture::fail_next();
    filter(desktop, "latest_reply").await?;
    wait(
        desktop,
        "!!document.querySelector('#canvas-comments [role=alert]')",
    )
    .await?;
    stage(state, desktop, "error", 6, &mut layouts).await?;
    click(desktop, "#canvas-comments-refresh").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===2 && !document.querySelector('#canvas-comments [role=alert]')").await?;
    stage(state, desktop, "recovered", 7, &mut layouts).await?;
    click(desktop, "#canvas-comments-more").await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===1 && document.getElementById('canvas-comments').textContent.includes('An older thread') && !document.getElementById('canvas-comments-more')").await?;
    stage(state, desktop, "stream-more", 8, &mut layouts).await?;
    let requests = fixture::ledger();
    if requests.len() != 11 {
        return Err(format!(
            "Unexpected comment request count: {}",
            requests.len()
        ));
    }
    let expected = [
        ("conversations.history", "latest_reply", None),
        ("conversations.history", "latest_reply", None),
        ("conversations.replies", "", None),
        ("conversations.replies", "", Some("reply-next")),
        ("conversations.replies", "", None),
        ("conversations.history", "latest_reply", None),
        ("conversations.history", "archived", None),
        ("conversations.history", "unread", None),
        ("conversations.history", "latest_reply", None),
        ("conversations.history", "latest_reply", None),
        ("conversations.history", "latest_reply", Some("stream-next")),
    ];
    for (request, (method, filter, cursor)) in requests.iter().zip(expected) {
        if !request["url"].as_str().is_some_and(|url| {
            url.split('?')
                .next()
                .is_some_and(|url| url.ends_with(&format!("/api/{method}")))
        }) {
            return Err("Comment request method differed".into());
        }
        let fields = request["fields"]
            .as_array()
            .ok_or("Missing request fields")?;
        let get = |key: &str| {
            fields
                .iter()
                .find(|field| field[0] == key)
                .and_then(|field| field[1].as_str())
        };
        if get("channel") != Some("Cfixture")
            || get("cursor") != cursor
            || get("text").is_some()
            || get("read").is_some()
        {
            return Err("Comment request identity differed".into());
        }
        if !filter.is_empty() && get("document_stream") != Some(filter) {
            return Err("Filter wire differed".into());
        }
        if method == "conversations.replies" && get("ts") != Some("1719800000.000100") {
            return Err("Thread root differed".into());
        }
    }
    click(desktop, "#canvas-comments-back").await?;
    wait(desktop,"!document.getElementById('canvas-comments') && !!document.querySelector('.canvas-document')").await?;
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-open'",
    )
    .await?;
    let shell = state.peek();
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Fixture acquired a live session".into());
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"disconnected":true,"readOnly":true,"exactRequests":true,"filterIsolation":true,"canonicalThread":true,"pagination":true,"errorNotEmpty":true,"explicitRetry":true,"returnedToCanvas":true,"focusContinuity":true,"syntheticInput":true,"requests":requests,"requestCount":requests.len(),"layouts":layouts,"layout":layouts.last()}),
    )
}
async fn filter(desktop: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    eval(desktop,&format!(r#"(()=>{{const e=document.getElementById('canvas-comments-filter');if(!e||e.disabled)return {{ok:false}};e.focus();e.value={};e.dispatchEvent(new Event('change',{{bubbles:true}}));return {{ok:true}};}})()"#,json!(value))).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    eval(desktop,&format!(r#"(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false}};e.scrollIntoView({{block:'center'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(!h||!(e===h||e.contains(h)))return {{ok:false}};e.click();return {{ok:true}};}})()"#,json!(selector))).await?;
    Ok(())
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, condition: &str) -> Result<(), String> {
    for _ in 0..80 {
        if eval_raw(desktop, &format!("({{ok:!!({condition})}})")).await?["ok"] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("Comment DOM did not settle: {condition}"))
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let value = eval_raw(desktop, script).await?;
    if value["ok"] != true {
        Err(format!("Comment native assertion failed: {value}"))
    } else {
        Ok(value)
    }
}
async fn eval_raw(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Comment evaluation failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Comment evaluation timed out")?
        .ok_or("Comment callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid comment callback")?;
    if let Value::String(value) = value {
        serde_json::from_str(&value).map_err(|_| "Invalid encoded comment callback".into())
    } else {
        Ok(value)
    }
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: usize,
    layouts: &mut Vec<Value>,
) -> Result<(), String> {
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-back'",
    )
    .await?;
    let layout = eval(desktop, include_str!("layout.js")).await?;
    layouts.push(layout);
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Comment fixture must remain disconnected".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private capture directory required")?;
    let ack = directory.join(format!("{ordinal}.ack"));
    eprintln!(
        "canvas-comments-stage: {}",
        json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true})
    );
    for _ in 0..600 {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture acknowledgement task failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture acknowledgement".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgement read failed".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("Comment capture timed out at {name}"))
}
