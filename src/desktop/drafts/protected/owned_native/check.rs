//! Mounted native input -> root-owned Delivery -> exact Owned queue execution.
use super as fixture;
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};
#[path = "check_helpers.rs"]
mod helpers;
use helpers::*;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let value = match tokio::time::timeout(Duration::from_secs(150), run(state, &desktop)).await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Owned native check timeout"}),
        };
        eprintln!("protected-owned-send-check: {value}");
    });
}
async fn typed(state: Signal<ShellState>, b: &fixture::Run, text: &str) -> Result<Value, String> {
    let count = b.routed.load(Ordering::Acquire);
    let result = dom(b.target(), "type", Some(text)).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Acquire) > count {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Acquire) == count + 1 && b.refused.load(Ordering::Acquire) == 0,
        "Actual owned input was not accepted exactly once",
    )?;
    let editor = b.editor(b.target()).map_err(|e| format!("{e:?}"))?;
    editor
        .flush()
        .await
        .map_err(|e| format!("Owned input flush {e:?}"))?;
    let actual = editor.snapshot();
    ensure(
        actual.pending.is_none()
            && !actual.running
            && actual.failed_inputs.is_empty()
            && actual.error.is_none(),
        "Owned input did not settle",
    )?;
    ensure(
        actual.view.local().composer.text == text,
        "Owned canonical text differs",
    )?;
    fixture::poll(&state.peek());
    ensure(
        result["native"]["text"] == text
            && result["native"]["display"] == text
            && result["fieldFocused"] == true,
        "Native input differs",
    )?;
    Ok(result)
}
async fn released(target: ComposerTarget) -> Result<Value, String> {
    for _ in 0..200 {
        let v = dom(target, "read", None).await?;
        if v["readOnly"] == false && v["admission"].is_null() {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Owned native admission did not release".into())
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    b: &fixture::Run,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    snapshots: &mut Vec<Value>,
) -> Result<Value, String> {
    // Read only: do not create/release a projection ticket while Delivery lives.
    let geometry = dom(b.target(), "read", None).await?;
    ensure(
        geometry["readOnly"] == false && geometry["admission"].is_null(),
        "Unexpected native admission at capture",
    )?;
    let durable = fixture::inventory(b).await?;
    let projection = fixture::snapshot(&state.peek(), b);
    ensure(
        projection["current"] == true
            && projection["syntheticSession"] == true
            && projection["cachePresent"] == false,
        "Owned fixture identities changed",
    )?;
    let path = fixture::isolation::capture_path(b, stage).await?;
    eprintln!(
        "protected-owned-send-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "protected-owned-send-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    let value =
        json!({"stage":stage,"geometry":geometry,"durable":durable,"projection":projection});
    snapshots.push(value.clone());
    Ok(value)
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(state).await?;
    fixture::isolation::wait_runner(&b).await?;
    let native_initial = home(state, b.target()).await?;
    dom(b.target(), "focus", None).await?;
    let mut captures = serde_json::Map::new();
    let mut snapshots = vec![];
    let initial = capture(state, desktop, &b, "initial", &mut captures, &mut snapshots).await?;
    let input = typed(state, &b, fixture::REQUESTED).await?;
    capture(state, desktop, &b, "input", &mut captures, &mut snapshots).await?;
    let click = controls(b.target(), true).await?;
    ensure(
        click["clicked"] == true,
        "Actual Send control was not clicked",
    )?;
    b.wait_held().await?;
    released(b.target()).await?;
    let cleared = capture(state, desktop, &b, "cleared", &mut captures, &mut snapshots).await?;
    ensure(
        cleared["geometry"]["native"]["text"] == "" && b.attempts.lock().unwrap().is_empty(),
        "Clear ACK did not precede Prepare/HTTP",
    )?;
    let parent = b
        .parent
        .lock()
        .unwrap()
        .clone()
        .ok_or("Actual parent missing")?;
    {
        let e = parent.record().evidence.lock().unwrap();
        ensure(
            e.consumed
                .as_ref()
                .is_some_and(|v| v.native_clear == super::super::NativeClear::Cleared)
                && e.receipt.is_none(),
            "Actual clear/claim boundary differs",
        )?;
    }
    let newer = typed(state, &b, fixture::NEWER).await?;
    let before = capture(state, desktop, &b, "newer", &mut captures, &mut snapshots).await?;
    ensure(
        b.attempts.lock().unwrap().is_empty(),
        "HTTP preceded newer durable checkpoint",
    )?;
    b.release();
    let status = tokio::time::timeout(Duration::from_secs(30), parent.ready())
        .await
        .map_err(|_| "Owned parent completion timeout")?
        .map_err(|e| format!("{e:?}"))?;
    ensure(
        status == super::super::operation::Status::Finished,
        "Actual retained Delivery did not finish",
    )?;
    let editor = b.editor(b.target()).map_err(|e| format!("{e:?}"))?;
    editor.flush().await.map_err(|e| format!("{e:?}"))?;
    let final_value = capture(state, desktop, &b, "result", &mut captures, &mut snapshots).await?;
    ensure(
        same_native(&newer["native"], &final_value["geometry"]["native"]),
        "Late completion changed newer native input/selection",
    )?;
    let target = label(b.target());
    let other = if target == "thread" {
        "channel"
    } else {
        "thread"
    };
    ensure(
        initial["durable"]["scopes"][other] == final_value["durable"]["scopes"][other],
        "Unrelated Owned scope changed",
    )?;
    ensure(
        final_value["durable"]["managedCount"] == 1
            && final_value["durable"]["localCount"] == 1
            && final_value["durable"]["scopes"][target]["row"].is_null(),
        "Accepted Send did not hand off actual local ownership",
    )?;
    ensure(
        before["durable"]["scopes"][target]["local"]
            == final_value["durable"]["scopes"][target]["local"],
        "Record/Release lost newer canonical input",
    )?;
    ensure(
        b.attempts.lock().unwrap().len() == 1 && b.refused.load(Ordering::Acquire) == 0,
        "Wrong actual prepared/refused count",
    )?;
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").ok(),"scenario":b.scenario,"phase":"exercise","nonce":b.nonce,"pid":std::process::id(),
        "account":b.account.id(),"ownedTicket":b.ticket,"isolatedRoot":b.root,"target":target,"viewport":snapshots[0]["geometry"]["viewport"],"captures":captures,"snapshots":snapshots,
        "details":{"nativeInitial":native_initial,"input":input,"send":click,"newer":newer},"attempts":b.attempts.lock().unwrap().clone(),"actions":b.actions.lock().unwrap().clone(),"nativeAdmission":b.native.lock().unwrap().clone(),
        "preparedRequests":1,"realHttpRequests":0,"syntheticConnectedEvent":true,"syntheticSession":true,"actualOwnedRoute":true,
        "evidenceScope":"named isolated mounted Send; actual native clear and typed queue; synthetic socket Connected and HTTP response; no production selector, Source gap/drain, restart or broad C3 parity"}),
    )
}
