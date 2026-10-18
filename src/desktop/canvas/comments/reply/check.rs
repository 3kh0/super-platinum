//! Disconnected two-process draft/receipt proof; native input events are synthetic.
use super::*;
use serde_json::{Value, json};
const FIRST: &str = "First reply \u{65e5}\u{672c}\u{8a9e} \u{1f98a} e\u{301}";
const NEWER: &str = "Newer local reply \u{65e5}\u{1f680}";
const UNKNOWN: &str = "Unconfirmed reply \u{65e5}\u{1f98a}";
const FINAL: &str = "A new explicit reply after review";
pub(in crate::canvas::comments) fn start(
    state: Signal<ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) {
    if !fixture::requested()
        || std::env::var("SUPER_PLATINUM_CANVAS_COMMENT_REPLY_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("canvas-comment-reply-check: {receipt}");
    });
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let phase =
        std::env::var("SUPER_PLATINUM_CANVAS_REPLY_PHASE").map_err(|_| "Missing reply phase")?;
    if !matches!(phase.as_str(), "exercise" | "restart") {
        return Err("Invalid reply phase".into());
    }
    wait(desktop, "!!document.querySelector('.canvas-comment-open')").await?;
    click(desktop, ".canvas-comment-open").await?;
    wait(desktop,"!!document.getElementById('canvas-comment-reply') && !document.getElementById('canvas-comment-reply').disabled").await?;
    let mut layouts = Vec::new();
    let initial = snapshot(state).await?;
    if phase == "exercise" {
        if initial["attempt"] != Value::Null
            || initial["drafts"] != json!([])
            || initial["history"] != json!([])
        {
            return Err("Exercise requires fresh isolated storage".into());
        }
        input(desktop, FIRST).await?;
        saved(state, FIRST).await?;
        stage(state, desktop, "saved", 0, &mut layouts).await?;
        click(desktop, "#canvas-comments-back").await?;
        wait(desktop, "!!document.getElementById('canvas-comments-open')").await?;
        click(desktop, "#canvas-comments-open").await?;
        wait(desktop, "!!document.querySelector('.canvas-comment-open')").await?;
        click(desktop, ".canvas-comment-open").await?;
        native_text(desktop, FIRST).await?;
        saved(state, FIRST).await?;
        stage(state, desktop, "reopened", 1, &mut layouts).await?;
        fixture::mode(1);
        click(desktop, "#canvas-reply-send").await?;
        wait(desktop,"document.querySelector('.canvas-comment-reply [role=alert]')?.textContent.includes('Nothing was sent')").await?;
        native_text(desktop, FIRST).await?;
        stage(state, desktop, "not-sent", 2, &mut layouts).await?;
        click(desktop, "#canvas-reply-retry").await?;
        wait(
            desktop,
            "document.getElementById('canvas-reply-send')?.disabled===false",
        )
        .await?;
        fixture::mode(3);
        click(desktop, "#canvas-reply-send").await?;
        wait_state(state, |e| e.sending && e.locked.is_some()).await?;
        input(desktop, NEWER).await?;
        saved(state, NEWER).await?;
        stage(state, desktop, "held-newer", 3, &mut layouts).await?;
        fixture::release();
        wait_state(state, |e| !e.sending && e.locked.is_none()).await?;
        native_text(desktop, NEWER).await?;
        saved(state, NEWER).await?;
        stage(state, desktop, "ack-newer", 4, &mut layouts).await?;
        let before = fixture::ledger()["prepared"].as_array().unwrap().len();
        fixture::mode(4);
        click(desktop, "#canvas-reply-send").await?;
        wait(desktop,"document.querySelector('.canvas-comment-reply [role=alert]')?.textContent.includes('thread or reply changed')").await?;
        if fixture::ledger()["prepared"].as_array().unwrap().len() != before {
            return Err("Stale preflight prepared a write".into());
        }
        stage(state, desktop, "stale", 5, &mut layouts).await?;
        fixture::mode(0);
        click(desktop, "#canvas-comments-refresh").await?;
        wait(desktop,"!!document.getElementById('canvas-comment-reply') && !document.querySelector('#canvas-comments [aria-busy=true]')").await?;
        input(desktop, UNKNOWN).await?;
        saved(state, UNKNOWN).await?;
        fixture::mode(2);
        click(desktop, "#canvas-reply-send").await?;
        wait_state(state, |e| !e.sending && e.locked.is_some()).await?;
        native_text(desktop, UNKNOWN).await?;
        stage(state, desktop, "uncertain", 6, &mut layouts).await?;
    } else {
        if initial["attempt"].is_null()
            || initial["drafts"].as_array().map(Vec::len) != Some(1)
            || initial["drafts"][0]["text"] != UNKNOWN
        {
            return Err("Restored reply receipt/content differs".into());
        }
        native_text(desktop, UNKNOWN).await?;
        wait(desktop,"document.getElementById('canvas-reply-send')?.disabled===true && document.getElementById('canvas-reply-archive')?.disabled===true").await?;
        stage(state, desktop, "restored", 0, &mut layouts).await?;
        if !fixture::ledger()["prepared"].as_array().unwrap().is_empty() {
            return Err("Restart prepared a replay".into());
        }
        click(desktop, "#canvas-reply-reviewed").await?;
        click(desktop, "#canvas-reply-archive").await?;
        wait_state(state, |e| !e.sending && e.locked.is_none()).await?;
        native_text(desktop, "").await?;
        stage(state, desktop, "reviewed", 1, &mut layouts).await?;
        if !fixture::ledger()["prepared"].as_array().unwrap().is_empty() {
            return Err("Review prepared a replay".into());
        }
        input(desktop, FINAL).await?;
        saved(state, FINAL).await?;
        click(desktop, "#canvas-reply-send").await?;
        wait_state(state, |e| {
            !e.sending && e.locked.is_none() && e.text.is_empty()
        })
        .await?;
        native_text(desktop, "").await?;
        stage(state, desktop, "sent", 2, &mut layouts).await?;
    }
    let final_state = snapshot(state).await?;
    let ledger = fixture::ledger();
    let prepared = ledger["prepared"].as_array().unwrap();
    let expected = if phase == "exercise" { 3 } else { 1 };
    if prepared.len() != expected {
        return Err(format!("Prepared count {}", prepared.len()));
    }
    let dispatched = ledger["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["url"]
                .as_str()
                .and_then(|v| url::Url::parse(v).ok())
                .is_some_and(|u| u.path() == "/api/chat.postMessage")
        })
        .count();
    if dispatched != if phase == "exercise" { 2 } else { 1 } {
        return Err("Dispatch count differs".into());
    }
    if (phase == "exercise") != final_state["attempt"].is_object() {
        return Err("Final receipt lock differs".into());
    }
    Ok(
        json!({"ok":true,"phase":phase,"syntheticInput":true,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"durableDraft":true,"exactRequests":true,"unknownNoReplay":true,"preparedCount":prepared.len(),"dispatchCount":dispatched,"initial":initial,"final":final_state,"ledger":ledger,"layout":layouts.last(),"layouts":layouts}),
    )
}
fn own_entry(shell: &ShellState) -> Option<&Entry> {
    shell
        .canvas_checklist
        .comment_replies
        .entries
        .iter()
        .find(|e| e.disk.account == fixture::ACCOUNT && e.root == fixture::TS)
}
async fn wait_state(
    state: Signal<ShellState>,
    test: impl Fn(&Entry) -> bool,
) -> Result<(), String> {
    for _ in 0..100 {
        if own_entry(&state.peek()).is_some_and(&test) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Reply controller did not settle".into())
}
async fn saved(state: Signal<ShellState>, text: &str) -> Result<(), String> {
    wait_state(state, |e| {
        !e.saving
            && e.error.is_none()
            && e.text == text
            && e.inventory.as_ref().is_some_and(|i| {
                i.drafts().len() == 1
                    && i.drafts()[0].text() == text
                    && i.drafts()[0].capture() == e.capture
            })
    })
    .await
}
async fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    let (disk, root) = {
        let shell = state.peek();
        let e = own_entry(&shell).ok_or("Reply entry missing")?;
        (e.disk.clone(), e.root.clone())
    };
    let scope = disk.scope.clone();
    disk.work(move|j|{let i=j.reply_inventory(&scope,&root)?;let h=j.reply_history(&scope,None)?;Ok(json!({"drafts":i.drafts(),"attempt":i.attempt(),"history":h.iter().map(|h|json!({"receipt":h.receipt,"status":h.status,"observed":h.observed})).collect::<Vec<_>>()}))}).await.map_err(|_|"Reply inventory read failed".into())
}
async fn input(desktop: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    eval(desktop,&format!(r#"(()=>{{const e=document.getElementById('canvas-comment-reply');if(!e||e.disabled)return {{ok:false}};e.scrollIntoView({{block:'center'}});e.focus();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,{});e.setSelectionRange(e.value.length,e.value.length,'forward');e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return {{ok:e.value==={}}};}})()"#,json!(text),json!(text))).await?;
    Ok(())
}
async fn native_text(desktop: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    wait(
        desktop,
        &format!(
            "document.getElementById('canvas-comment-reply')?.value==={}",
            json!(text)
        ),
    )
    .await
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
        "canvas-comment-reply-stage: {}",
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
