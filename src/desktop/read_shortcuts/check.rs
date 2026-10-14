//! Disconnected native Escape dispatch through real DOM keyboard and read handlers.
use super::fixture;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::time::Duration;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => json!({"ok":false,"error":error}),
                Err(_) => json!({"ok":false,"error":"Reading shortcut check timed out"}),
            };
        eprintln!("reading-shortcuts-check: {receipt}");
    });
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Reading shortcuts DOM: {value}")),
        _ => Err("Reading shortcuts DOM timed out".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn ready() -> Result<(std::path::PathBuf, String, String), String> {
    let (dir, nonce) = fixture::directory()?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":name});
    let path = dir.join("ready.json");
    if path.exists() {
        return Err("Readiness marker must be new".into());
    }
    eprintln!("reading-shortcuts-waiting: {expected}");
    for _ in 0..300 {
        let check = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let meta = match std::fs::symlink_metadata(&check) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Cannot inspect readiness".into()),
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 {
                return Err("Invalid readiness file".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Readiness reparse point refused".into());
                }
            }
            serde_json::from_slice(&std::fs::read(check).map_err(|_| "Cannot read readiness")?)
                .map(Some)
                .map_err(|_| "Invalid readiness JSON".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            if found != expected {
                return Err("Readiness owner mismatch".into());
            }
            return Ok((dir, nonce, name));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}

async fn wait(
    state: Signal<ShellState>,
    test: impl Fn(&ShellState) -> bool,
    label: &str,
) -> Result<(), String> {
    for _ in 0..150 {
        if test(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Escape state did not settle: {label}"))
}
async fn settle(state: Signal<ShellState>) -> Result<(), String> {
    frames().await?;
    crate::bootstrap::refresh_selected_channel(state).await;
    frames().await?;
    Ok(())
}
async fn input(state: Signal<ShellState>, id: &str, text: &str) -> Result<(), String> {
    eval(format!("const e=document.getElementById({});if(e){{e.focus({{preventScroll:true}});e.value={};e.setSelectionRange(e.value.length,e.value.length);e.dispatchEvent(new Event('input',{{bubbles:true}}));}}dioxus.send({{ok:!!e}});await dioxus.recv();",json!(id),json!(text))).await?;
    wait(
        state,
        |s| {
            if id == "thread-composer" {
                s.core.thread_composer.text == text
            } else {
                s.core.composer.text == text
            }
        },
        "composer input",
    )
    .await
}
async fn key(selector: &str, extra: Value) -> Result<Value, String> {
    eval(format!("const e=document.querySelector({});if(e)e.focus({{preventScroll:true}});const event=new KeyboardEvent('keydown',{{key:'Escape',bubbles:true,cancelable:true,...{}}});if(e)e.dispatchEvent(event);dioxus.send({{ok:!!e&&document.activeElement===e,prevented:event.defaultPrevented,text:e?.value??null,facts:{{ctrlKey:event.ctrlKey,metaKey:event.metaKey,altKey:event.altKey,repeat:event.repeat,isComposing:event.isComposing,keyCode:event.keyCode}}}});await dioxus.recv();",json!(selector),extra)).await
}
async fn count(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let result = wait(
        state,
        |_| fixture::attempts().is_ok_and(|a| a.len() == expected),
        "prepared request count",
    )
    .await;
    if result.is_err() {
        let debug=eval("dioxus.send({ok:true,events:window.superPlatinumReadShortcutCheckDiagnostics?.snapshot()??[]});await dioxus.recv();".into()).await?;
        let shell = state.peek();
        return Err(format!(
            "Escape request count: expected {expected}, actual {}, owner {}, primary preparation {}, native key {}, DOM diagnostics {debug}",
            fixture::attempts()?.len(),
            fixture::owner(&shell),
            super::escape::prepare(&shell, "primary", None).is_some(),
            super::key(&shell)
        ));
    }
    result
}
async fn unchanged(expected: usize) -> Result<(), String> {
    frames().await?;
    tokio::time::sleep(Duration::from_millis(180)).await;
    ensure(
        fixture::attempts()?.len() == expected,
        "Guarded key dispatched a read",
    )
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    let mut value=eval("const rect=e=>{if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};const t=document.getElementById('message-timeline'),c=document.getElementById('channel-composer'),p=document.querySelector('.thread-pane'),r=document.getElementById('thread-composer');const b=t?.getBoundingClientRect();dioxus.send({ok:!!t&&!!c&&b.width>0&&b.height>0&&b.top>=0&&b.bottom<=innerHeight+1&&document.documentElement.scrollWidth<=innerWidth+1,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,timeline:rect(t),composer:rect(c),thread:rect(p),threadComposer:rect(r),threadVisible:!!p,activeId:document.activeElement?.id??null,channelText:c?.value,threadText:r?.value??null});await dioxus.recv();".into()).await?;
    value["stage"] = json!(stage);
    value["requestCount"] = json!(fixture::attempts()?.len());
    {
        let s = state.peek();
        ensure(
            value["channelText"].as_str() == Some(s.core.composer.text.as_str()),
            "Rendered channel draft differs from model",
        )?;
        if s.thread_root.is_some() {
            ensure(
                value["threadText"].as_str() == Some(s.core.thread_composer.text.as_str()),
                "Rendered thread draft differs from model",
            )?;
        }
    }
    geometry.push(value);
    eprintln!(
        "reading-shortcuts-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) = crate::agent::capture::window(
        Some(
            dir.join(format!("{stage}.png"))
                .to_string_lossy()
                .into_owned(),
        ),
        desktop,
    )
    .await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "reading-shortcuts-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn captured_key() -> Result<super::Payload, String> {
    let value=eval("const e=document.getElementById('channel-composer');e?.focus({preventScroll:true});let payload=null;const capture=event=>{payload=window.superPlatinumReadShortcuts?.capture(event);event.stopImmediatePropagation();};e?.addEventListener('keydown',capture,{once:true,capture:true});e?.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));dioxus.send({ok:!!payload,payload});await dioxus.recv();".into()).await?;
    serde_json::from_value(value["payload"].clone())
        .map_err(|_| "Captured Escape payload malformed".into())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, name) = ready().await?;
    ensure(
        fixture::owner(&state.peek()),
        "Disconnected Escape owner missing",
    )?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mut checks = serde_json::Map::new();
    let mut guard_events = Vec::new();
    eval(include_str!("check_diagnostics.js").into()).await?;
    settle(state).await?;
    capture(
        state,
        desktop,
        &dir,
        "initial",
        &mut captures,
        &mut geometry,
    )
    .await?;
    // Metadata supplies a real server coordinate even with zero loaded rows.
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        let b = w.messages.get_mut("C2").unwrap();
        b.messages.clear();
        b.loaded = false;
        b.latest = None;
        w.channels
            .get_mut("C2")
            .unwrap()
            .extra
            .insert("latest".into(), json!({"ts":fixture::ts(99)}));
        s.core.pending_scroll_to = None;
        s.refresh_from_core();
    }
    frames().await?;
    fixture::stage("metadata-read")?;
    ensure(
        super::escape::prepare(&state.peek(), "primary", None).is_some(),
        "Metadata fixture did not prepare a channel read",
    )?;
    key("#channel-composer", json!({})).await?;
    count(state, 1).await?;
    ensure(
        state.peek().core.workspaces["T1"].read_cursor("C2") == Some(fixture::ts(99).as_str()),
        "Metadata-only cursor did not acknowledge",
    )?;
    ensure(
        state.peek().core.composer.text == fixture::DRAFT,
        "Metadata key altered draft",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "metadata-read",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("metadataOnly".into(), json!(true));
    {
        let mut s = state.write();
        fixture::reset(&mut s);
        let b = s
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap();
        b.last_read = Some(fixture::ts(48));
        b.unread_count = 0;
        s.refresh_from_core();
    }
    settle(state).await?;
    fixture::stage("ghost-cleared")?;
    key("#channel-composer", json!({})).await?;
    wait(state, |s| s.unread_anchor.is_none(), "ghost clear").await?;
    unchanged(1).await?;
    capture(
        state,
        desktop,
        &dir,
        "ghost-cleared",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("ghostNoRequest".into(), json!(true));
    fixture::reset(&mut state.write());
    settle(state).await?;
    input(state, "channel-composer", "").await?;
    fixture::stage("composer-empty")?;
    key("#channel-composer", json!({})).await?;
    count(state, 2).await?;
    ensure(
        state.peek().core.composer.text.is_empty(),
        "Empty draft changed",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "composer-empty",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("emptyComposer".into(), json!(true));
    fixture::reset(&mut state.write());
    settle(state).await?;
    input(state, "channel-composer", fixture::DRAFT).await?;
    eval("const e=document.getElementById('channel-composer');e.setSelectionRange(3,8,'backward');dioxus.send({ok:true});await dioxus.recv();".into()).await?;
    fixture::stage("composer-nonempty")?;
    key("#channel-composer", json!({})).await?;
    count(state, 3).await?;
    ensure(
        state.peek().core.composer.text == fixture::DRAFT,
        "Nonempty draft changed",
    )?;
    let selection=eval("const e=document.getElementById('channel-composer');dioxus.send({ok:e.selectionStart===3&&e.selectionEnd===8&&e.selectionDirection==='backward',start:e.selectionStart,end:e.selectionEnd,direction:e.selectionDirection});await dioxus.recv();".into()).await?;
    capture(
        state,
        desktop,
        &dir,
        "composer-nonempty",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("nonemptyComposerPreserved".into(), json!(true));
    {
        let mut s = state.write();
        fixture::reset(&mut s);
        fixture::thread(&mut s);
    }
    settle(state).await?;
    input(state, "thread-composer", fixture::REPLY).await?;
    fixture::stage("thread-read")?;
    key("#thread-composer", json!({})).await?;
    count(state, 4).await?;
    ensure(
        state.peek().thread_root.as_deref() == Some(fixture::ROOT),
        "First thread Escape closed pane",
    )?;
    ensure(
        state.peek().core.thread_composer.text == fixture::REPLY,
        "Thread read changed reply",
    )?;
    let thread_key = ("T1".into(), "C2".into(), fixture::ROOT.into());
    ensure(
        state.peek().core.thread_cursor(&thread_key) == Some(fixture::THREAD_LATEST),
        "Thread cursor not acknowledged",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "thread-read",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("threadFirstRead".into(), json!(true));
    fixture::stage("thread-closed")?;
    key("#thread-composer", json!({})).await?;
    wait(
        state,
        |s| s.thread_root.is_none(),
        "second thread Escape close",
    )
    .await?;
    unchanged(4).await?;
    let draft_key = super_platinum_core::drafts::DraftKey {
        team: "T1".into(),
        channel: "C2".into(),
        thread: Some(fixture::ROOT.into()),
    };
    ensure(
        state
            .peek()
            .core
            .drafts
            .items
            .get(&draft_key)
            .is_some_and(|draft| draft.composer.text == fixture::REPLY),
        "Closed reply draft was lost",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "thread-closed",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("threadSecondCloses".into(), json!(true));
    {
        let mut s = state.write();
        fixture::reset(&mut s);
        fixture::thread(&mut s);
    }
    settle(state).await?;
    let thread_before = state
        .peek()
        .core
        .thread_cursor(&thread_key)
        .map(str::to_owned);
    fixture::stage("focused-primary")?;
    // Workspace rail focus falls through to primary; an open thread is not focus.
    key(".rail-btn", json!({})).await?;
    count(state, 5).await?;
    ensure(
        state.peek().thread_root.as_deref() == Some(fixture::ROOT)
            && state.peek().core.thread_cursor(&thread_key) == thread_before.as_deref(),
        "Other pane focus consumed thread",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "focused-primary",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("otherPaneTargetsPrimary".into(), json!(true));
    fixture::reset(&mut state.write());
    settle(state).await?;
    fixture::stage("blocked-keys")?;
    for (label, extra) in [
        ("control", json!({"ctrlKey":true})),
        ("meta", json!({"metaKey":true})),
        ("alt", json!({"altKey":true})),
        ("repeat", json!({"repeat":true})),
        ("composing", json!({"isComposing":true})),
        ("key229", json!({"keyCode":229})),
    ] {
        let event = key("#channel-composer", extra.clone()).await?;
        ensure(
            extra.as_object().is_some_and(|fields| {
                fields
                    .iter()
                    .all(|(name, value)| &event["facts"][name] == value)
            }),
            "Keyboard constructor did not preserve requested guard",
        )?;
        unchanged(5).await?;
        guard_events.push(json!({"kind":label,"prevented":event["prevented"],"facts":event["facts"],"requestCount":5}));
    }
    // An actual visible completion must consume Escape before the global bridge.
    input(state, "channel-composer", "@").await?;
    for _ in 0..100 {
        let v=eval("const p=document.getElementById('channel-composer-completion');dioxus.send({ok:true,visible:!!p&&!p.hidden&&p.querySelectorAll('[role=option]').length>0});await dioxus.recv();".into()).await?;
        if v["visible"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let completion=eval("const e=document.getElementById('channel-composer'),p=document.getElementById('channel-composer-completion');const before=!!p&&!p.hidden&&p.querySelectorAll('[role=option]').length>0;const event=new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true});e.dispatchEvent(event);dioxus.send({ok:before&&event.defaultPrevented&&p.hidden&&e.value==='@',prevented:event.defaultPrevented,wasVisible:before,nowHidden:p.hidden});await dioxus.recv();".into()).await?;
    unchanged(5).await?;
    guard_events
        .push(json!({"kind":"completion","prevented":completion["prevented"],"requestCount":5}));
    input(state, "channel-composer", fixture::DRAFT).await?;
    // Explicit synthetic popup uses the real bridge's menu blocker selector.
    eval("const m=document.createElement('div');m.id='reading-shortcut-test-menu';m.setAttribute('role','menu');m.textContent='Synthetic keyboard scope';m.style.cssText='position:fixed;top:80px;left:80px;z-index:1000;padding:8px;background:var(--surface-raised);color:var(--fg)';document.body.append(m);dioxus.send({ok:!!m.getClientRects().length});await dioxus.recv();".into()).await?;
    key("#channel-composer", json!({})).await?;
    unchanged(5).await?;
    eval("document.getElementById('reading-shortcut-test-menu')?.remove();dioxus.send({ok:true});await dioxus.recv();".into()).await?;
    guard_events.push(json!({"kind":"menu","requestCount":5}));
    ensure(
        state.peek().core.composer.text == fixture::DRAFT,
        "Guarded keys changed draft",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "blocked-keys",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("blockedKeys".into(), json!(true));
    fixture::stage("stale-events")?;
    let mut stale = Vec::new();
    for kind in ["scope", "account", "focus-return"] {
        let payload = captured_key().await?;
        match kind {
            "scope" => state.write().channel_generation += 1,
            "account" => {
                let mut s = state.write();
                s.core.account_epoch += 1;
                s.core.active_account = Some("reading-shortcuts-other-account".into());
            }
            _ => {
                eval("const other=document.querySelector('.rail-btn'),e=document.getElementById('channel-composer');other.focus({preventScroll:true});e.focus({preventScroll:true});dioxus.send({ok:document.activeElement===e});await dioxus.recv();".into()).await?;
            }
        }
        frames().await?;
        super::handle(state, payload).await;
        unchanged(5).await?;
        if kind == "account" {
            state.write().core.active_account = Some(fixture::ACCOUNT.into());
            frames().await?;
        }
        stale.push(json!({"kind":kind,"requestCount":5}));
    }
    ensure(
        state.peek().core.composer.text == fixture::DRAFT,
        "Stale keys changed draft",
    )?;
    capture(
        state,
        desktop,
        &dir,
        "stale-events",
        &mut captures,
        &mut geometry,
    )
    .await?;
    checks.insert("staleEvents".into(), json!(true));
    checks.insert("draftsPreserved".into(), json!(true));
    ensure(fixture::owner(&state.peek()), "Fixture owner lost")?;
    let attempts = fixture::attempts()?;
    ensure(attempts.len() == 5, "Unexpected Escape request count")?;
    ensure(
        captures.len() == fixture::STAGES.len(),
        "Missing Escape capture stage",
    )?;
    let viewport = geometry.last().ok_or("Missing capture")?["viewport"].clone();
    eval("window.superPlatinumReadShortcutCheckDiagnostics?.dispose();dioxus.send({ok:true});await dioxus.recv();".into()).await?;
    Ok(
        json!({"ok":true,"nonce":nonce,"pid":std::process::id(),"fixture":name,"noSession":true,"noTransport":true,"syntheticAcknowledgements":true,"syntheticKeyboard":true,"captures":captures,"geometry":geometry,"viewport":viewport,"preparedRequests":5,"attempts":attempts,"checks":checks,"selection":selection,"guardEvents":guard_events,"staleEvents":stale}),
    )
}
