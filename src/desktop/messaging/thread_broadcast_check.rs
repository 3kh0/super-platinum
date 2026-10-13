//! Disconnected real-composer checks. No session, transport, or network send.
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
const ROOT: &str = "100.000001";

pub(crate) fn install(shell: &mut ShellState) {
    shell.core.session = None;
    shell.core.transport = None;
    shell.core.active_account = None;
    shell.overlay = None;
    shell.toast = None;
    open(shell, ROOT);
    shell.core.thread_composer.text =
        "Reply stays in its thread until explicitly broadcast.".into();
    shell.checkpoint_composers();
}
fn open(shell: &mut ShellState, root: &str) {
    shell.begin_thread(root.into(), None);
    let key = shell.composer_key(ComposerTarget::Thread).unwrap();
    shell
        .core
        .threads
        .entry((key.team, key.channel.clone(), root.into()))
        .or_default()
        .upsert(super_platinum_core::slack::models::Message {
            ts: Some(root.into()),
            channel: Some(key.channel),
            user: Some("U0".into()),
            text: Some("Thread broadcast verification".into()),
            ..Default::default()
        });
    shell.thread_loading = false;
    shell.refresh_thread_from_core();
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_THREAD_BROADCAST_CHECK").as_deref() != Ok("1")
        || !matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("thread-broadcast" | "thread-broadcast-narrow")
        )
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(receipt) => receipt,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("thread-broadcast-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Connected fixture refused".into());
    }
    ready(desktop, false).await?;
    let original = state.peek().core.thread_composer.text.clone();
    click(desktop, "#thread-reply-broadcast").await?;
    ready(desktop, true).await?;
    wait_state(state, |s| s.core.thread_composer.reply_broadcast).await?;
    let key = state.peek().composer_key(ComposerTarget::Thread).unwrap();
    let scope = crate::view::composer::field_scope(&state.peek(), ComposerTarget::Thread);
    let generation = state.peek().thread_generation;
    {
        open(&mut state.write(), "100.000002");
    }
    ready(desktop, false).await?;
    {
        open(&mut state.write(), ROOT);
    }
    ready(desktop, true).await?;
    if state
        .write()
        .set_thread_broadcast(&scope, generation, false)
    {
        return Err("Stale visit callback admitted".into());
    }
    if state.peek().core.thread_composer.text != original {
        return Err("Thread draft text lost on navigation".into());
    }
    let snapshot = super_platinum_core::drafts::codec::encode_composer(
        &key,
        &state.peek().core.thread_composer,
    )
    .map_err(|e| e.to_string())?;
    if snapshot.destinations[0]["broadcast"] != true
        || snapshot.destinations[0]["thread_ts"] != ROOT
    {
        return Err("Wrong captured destination".into());
    }
    let source =
        crate::view::scheduled::ComposeSource::capture(&state.peek(), ComposerTarget::Thread)
            .ok_or("Missing schedule source")?;
    let request = source.request(&state.peek(), 2000000000);
    if request.destinations != snapshot.destinations {
        return Err("Schedule lost broadcast destination".into());
    }
    // Exercise real queue/clear through the Send button. The absent transport is
    // deliberate: only the original pending payload remains, never an HTTP call.
    let before = state.peek().core.threads[&(key.team.clone(), key.channel.clone(), ROOT.into())]
        .pending
        .len();
    click(desktop, ".thread-composer .composer-send-btn").await?;
    wait_state(state, |s| s.core.thread_composer.text.is_empty()).await?;
    ready(desktop, false).await?;
    {
        let shell = state.peek();
        let messages = &shell.core.threads[&(key.team.clone(), key.channel.clone(), ROOT.into())];
        if messages.pending.len() != before + 1
            || !messages.messages.iter().any(|m| {
                m.subtype.as_deref() == Some("thread_broadcast")
                    && m.thread_ts.as_deref() == Some(ROOT)
                    && m.text.as_deref() == Some(original.as_str())
            })
        {
            return Err("Captured thread pending payload missing".into());
        }
    }
    // A second click on the now-empty composer cannot enqueue another message.
    click(desktop, ".thread-composer .composer-send-btn").await?;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    if state.peek().core.threads[&(key.team.clone(), key.channel.clone(), ROOT.into())]
        .pending
        .len()
        != before + 1
    {
        return Err("Duplicate empty send queued".into());
    }
    for (im, mpim, label) in [
        (true, false, "Also send as direct message"),
        (false, true, "Also send to the group"),
        (false, false, "Also send to ship"),
    ] {
        {
            let mut shell = state.write();
            let channel = shell
                .core
                .workspaces
                .get_mut(&key.team)
                .unwrap()
                .channels
                .get_mut(&key.channel)
                .unwrap();
            channel.is_im = im;
            channel.is_mpim = mpim;
            shell.toast = None;
        }
        wait_dom(
            desktop,
            &format!(
                "({{ok:document.querySelector('.thread-broadcast')?.textContent.trim()==={}}})",
                serde_json::to_string(label).unwrap()
            ),
        )
        .await?;
    }
    state.write().toast = None;
    let mut receipt = wait_dom(desktop, LAYOUT).await?;
    for flag in [
        "choicePersistsByThread",
        "staleVisitRejected",
        "capturedDestination",
        "schedulePreserved",
        "sendClearedChoice",
        "duplicateEmptySendRejected",
        "dmLabels",
        "noTransport",
    ] {
        receipt[flag] = json!(true);
    }
    receipt["queuedMessages"] = json!(1);
    Ok(receipt)
}
async fn ready(desktop: &dioxus::desktop::DesktopContext, checked: bool) -> Result<Value, String> {
    wait_dom(desktop,&format!("({{ok:!!document.getElementById('thread-composer')&&document.getElementById('thread-reply-broadcast')?.checked==={checked}}})")).await
}
async fn click(desktop: &dioxus::desktop::DesktopContext, selector: &str) -> Result<(), String> {
    let value = eval(desktop,&format!("(()=>{{const e=document.querySelector({});if(!e||e.disabled)return {{ok:false}};e.click();return {{ok:true}};}})()",serde_json::to_string(selector).unwrap())).await?;
    if value["ok"] == true {
        Ok(())
    } else {
        Err(format!("Missing control {selector}"))
    }
}
async fn wait_state(
    state: Signal<ShellState>,
    predicate: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..60 {
        if predicate(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Composer state did not settle".into())
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
    Err(format!("Native thread broadcast did not settle: {last}"))
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |v| {
            let _ = tx.send(v);
        })
        .map_err(|_| "Evaluation failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| "Evaluation timed out")?
        .ok_or("Callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid evaluation")?;
    match value {
        Value::String(v) => {
            serde_json::from_str(&v).map_err(|_| "Invalid encoded evaluation".into())
        }
        v => Ok(v),
    }
}
const LAYOUT: &str = r#"(()=>{const e=document.querySelector('.thread-broadcast'),c=document.getElementById('thread-reply-broadcast'),f=document.getElementById('thread-composer');if(!e||!c||!f)return {ok:false};c.focus();const rect=e.getBoundingClientRect();const eachVisible=[e,c,f].every(x=>{const r=x.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1});return {ok:eachVisible&&document.activeElement===c,eachVisible,keyboardFocused:document.activeElement===c,noOverflow:rect.right<=innerWidth+1,viewport:[innerWidth,innerHeight]};})()"#;
