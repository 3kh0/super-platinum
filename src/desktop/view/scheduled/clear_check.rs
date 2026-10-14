//! Synthetic successful scheduling receipts only: no Slack request or storage.
//! Composition events exercise the native DOM guard, not an actual OS IME.
use super::{ComposeSource, ComposerTarget, ShellState, complete_source};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::time::Duration;

const ORIGINAL: &str = "Scheduled original — 日本語 🚀";
const NEWER: &str = "Newer native input stays in this draft — 日本語 🚀";
const ROOT: &str = "1719800400.000000";

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    shell.core.session = None;
    shell.core.transport = None;
    if name.ends_with("-thread") {
        open_thread(shell);
    }
    shell.sync_composer_drafts();
    shell.core.composer.text = ORIGINAL.into();
    if shell.thread_root.is_some() {
        shell.core.thread_composer.text = ORIGINAL.into();
    }
    shell.checkpoint_composers();
}

fn open_thread(shell: &mut ShellState) {
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    shell.thread_generation = shell.thread_generation.wrapping_add(1);
    shell.thread_root = Some(ROOT.into());
    shell.core.threads.insert(
        (team, channel, ROOT.into()),
        crate::thread_vm::fixture_bag(4),
    );
    shell.sync_composer_drafts();
    shell.refresh_thread_from_core();
}

pub(crate) async fn run(state: Signal<ShellState>) {
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    if std::env::var("SUPER_PLATINUM_SCHEDULE_CLEAR_CHECK").as_deref() != Ok("1")
        || !matches!(
            fixture.as_str(),
            "schedule-clear" | "schedule-clear-narrow" | "schedule-clear-thread"
        )
    {
        return;
    }
    let result = check(state, &fixture).await;
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("schedule-clear-check: {receipt}");
}

async fn check(mut state: Signal<ShellState>, fixture: &str) -> Result<Value, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Disconnected scheduling fixture required".into());
    }
    if state.peek().thread_root.is_none() {
        open_thread(&mut state.write());
    }
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let source = prepare(state, target).await?;
        native_value(target, NEWER, false).await?;
        complete_source(state, &source).await;
        expect_native(target, NEWER).await?;
        assert_local(state, target, NEWER)?;
        navigation_roundtrip(state, target).await?;
        expect_native(target, NEWER).await?;
        assert_local(state, target, NEWER)?;

        let source = prepare(state, target).await?;
        let id = field(target);
        let start = format!(
            "(()=>{{const e=document.getElementById('{id}');e.dispatchEvent(new CompositionEvent('compositionstart',{{bubbles:true,data:'に'}}));return {{ok:e.dataset.composing==='true'}};}})()"
        );
        checked(&start).await?;
        complete_source(state, &source).await;
        expect_native(target, ORIGINAL).await?;
        assert_local(state, target, ORIGINAL)?;
        checked(&format!("(()=>{{const e=document.getElementById('{id}');e.dispatchEvent(new CompositionEvent('compositionend',{{bubbles:true,data:'日本語'}}));return {{ok:e.dataset.composing==='false'}};}})()")).await?;

        let source = prepare(state, target).await?;
        complete_source(state, &source).await;
        expect_native(target, "").await?;
        if !text(&state.peek(), target).is_empty() {
            return Err("Matching source did not settle".into());
        }
    }

    let channel_source = prepare(state, ComposerTarget::Channel).await?;
    navigation_roundtrip(state, ComposerTarget::Channel).await?;
    // Exercise the actual delayed native command with an old A→B→A scope,
    // not only the Rust response guard.
    let stale = super::super::composer::settle_scheduled_field(
        "channel-composer",
        &channel_source.field_scope,
        &channel_source.text,
    )
    .await;
    if !matches!(
        stale,
        super::super::composer::ScheduledClearReceipt::Unavailable
    ) {
        return Err("Old channel visit reached the replacement native field".into());
    }
    complete_source(state, &channel_source).await;
    expect_native(ComposerTarget::Channel, ORIGINAL).await?;
    assert_local(state, ComposerTarget::Channel, ORIGINAL)?;

    let source = prepare(state, ComposerTarget::Thread).await?;
    {
        let mut shell = state.write();
        shell.close_thread();
        open_thread(&mut shell);
        shell.core.thread_composer.text = ORIGINAL.into();
        shell.checkpoint_composers();
    }
    let scope = super::super::composer::field_scope(&state.peek(), ComposerTarget::Thread);
    wait_scope(ComposerTarget::Thread, &scope).await?;
    native_value(ComposerTarget::Thread, ORIGINAL, false).await?;
    complete_source(state, &source).await;
    expect_native(ComposerTarget::Thread, ORIGINAL).await?;
    if text(&state.peek(), ComposerTarget::Thread) != ORIGINAL {
        return Err("Reopened thread was cleared by the previous visit".into());
    }

    let final_target = if fixture.ends_with("-thread") {
        ComposerTarget::Thread
    } else {
        state.write().close_thread();
        ComposerTarget::Channel
    };
    prepare(state, final_target).await?;
    native_value(final_target, NEWER, true).await?;
    commit_native(state, final_target, NEWER).await?;
    let id = field(final_target);
    let mut receipt = checked(&format!("(()=>{{const e=document.getElementById('{id}');const r=e.getBoundingClientRect();const h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);const eachVisible=r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;const hitTested=h===e||e.contains(h);return {{ok:eachVisible&&hitTested,eachVisible,hitTested,viewport:[innerWidth,innerHeight]}};}})()")).await?;
    for key in [
        "newerNativeInputPreserved",
        "syntheticCompositionPreserved",
        "reopenedThreadPreserved",
        "reopenedChannelPreserved",
        "matchingSourceCleared",
        "bothComposersChecked",
        "navigationRetainedNewerDraft",
        "noSlackMutation",
    ] {
        receipt[key] = json!(true);
    }
    receipt["noTransport"] = json!(state.peek().core.transport.is_none());
    receipt["noSession"] = json!(state.peek().core.session.is_none());
    receipt["preparedRequests"] = json!(0);
    Ok(receipt)
}

fn assert_local(
    state: Signal<ShellState>,
    target: ComposerTarget,
    value: &str,
) -> Result<(), String> {
    let shell = state.peek();
    let key = shell
        .composer_key(target)
        .ok_or("Missing composer destination")?;
    if text(&shell, target) == value
        && shell
            .core
            .drafts
            .items
            .get(&key)
            .is_some_and(|draft| draft.composer.text == value)
    {
        Ok(())
    } else {
        Err("DOM receipt did not directly preserve its local draft without an input event".into())
    }
}
async fn navigation_roundtrip(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<(), String> {
    match target {
        ComposerTarget::Channel => {
            let original = state.peek().active_channel;
            let other = if original == 0 { 1 } else { 0 };
            state
                .write()
                .select_channel(other, crate::state::ChannelOpen::Global);
            let away = super::super::composer::field_scope(&state.peek(), target);
            wait_scope(target, &away).await?;
            state
                .write()
                .select_channel(original, crate::state::ChannelOpen::Global);
            let returned = super::super::composer::field_scope(&state.peek(), target);
            wait_scope(target, &returned).await?;
            if state.peek().thread_root.is_none() {
                open_thread(&mut state.write());
            }
        }
        ComposerTarget::Thread => {
            state.write().close_thread();
            wait("({ok:!document.getElementById('thread-composer')})").await?;
            open_thread(&mut state.write());
        }
    }
    let scope = super::super::composer::field_scope(&state.peek(), target);
    wait_scope(target, &scope).await
}

fn field(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Channel => "channel-composer",
        ComposerTarget::Thread => "thread-composer",
    }
}
fn text(shell: &ShellState, target: ComposerTarget) -> &str {
    match target {
        ComposerTarget::Channel => &shell.core.composer.text,
        ComposerTarget::Thread => &shell.core.thread_composer.text,
    }
}
async fn prepare(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<ComposeSource, String> {
    let source = {
        let mut shell = state.write();
        match target {
            ComposerTarget::Channel => shell.core.composer.text = ORIGINAL.into(),
            ComposerTarget::Thread => shell.core.thread_composer.text = ORIGINAL.into(),
        }
        shell.checkpoint_composers();
        ComposeSource::capture(&shell, target).ok_or("Missing fixture source")?
    };
    wait_scope(target, &source.field_scope).await?;
    native_value(target, ORIGINAL, false).await?;
    Ok(source)
}
async fn native_value(target: ComposerTarget, value: &str, input: bool) -> Result<(), String> {
    let args = serde_json::to_string(&(field(target), value, input)).unwrap();
    checked(&format!("(()=>{{const [id,value,input]={args};const e=document.getElementById(id);if(!e)return {{ok:false}};e.value=value;e.dataset.composing='false';if(input)e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()")).await.map(|_| ())
}
async fn wait_scope(target: ComposerTarget, scope: &str) -> Result<(), String> {
    let args = serde_json::to_string(&(field(target), scope)).unwrap();
    wait(&format!("(()=>{{const [id,scope]={args};const e=document.getElementById(id);return {{ok:!!e&&e.dataset.draftScope===scope}};}})()")).await
}
async fn expect_native(target: ComposerTarget, value: &str) -> Result<(), String> {
    let args = serde_json::to_string(&(field(target), value)).unwrap();
    wait(&format!("(()=>{{const [id,value]={args};const e=document.getElementById(id);return {{ok:!!e&&e.value===value,actual:e?.value}};}})()")).await
}
async fn commit_native(
    state: Signal<ShellState>,
    target: ComposerTarget,
    value: &str,
) -> Result<(), String> {
    checked(&format!("(()=>{{document.getElementById('{}').dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()", field(target))).await?;
    for _ in 0..40 {
        if text(&state.peek(), target) == value {
            let shell = state.peek();
            let key = shell.composer_key(target).unwrap();
            if shell
                .core
                .drafts
                .items
                .get(&key)
                .is_some_and(|draft| draft.composer.text == value)
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Native input did not return to its destination-owned local draft".into())
}
async fn wait(expression: &str) -> Result<(), String> {
    let mut last = Value::Null;
    for _ in 0..50 {
        last = evaluate(expression).await?;
        if last["ok"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("Native scheduling assertion timed out: {last}"))
}
async fn checked(expression: &str) -> Result<Value, String> {
    let value = evaluate(expression).await?;
    if value["ok"] == true {
        Ok(value)
    } else {
        Err(format!("Native scheduling assertion failed: {value}"))
    }
}
async fn evaluate(expression: &str) -> Result<Value, String> {
    let mut eval = document::eval(&format!(
        "try{{dioxus.send({expression});}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();"
    ));
    let value = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>())
        .await
        .map_err(|_| "Scheduling DOM assertion timed out")?
        .map_err(|e| e.to_string());
    let _ = eval.send(());
    value
}
