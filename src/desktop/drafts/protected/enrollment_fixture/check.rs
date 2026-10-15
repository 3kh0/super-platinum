//! Actual modal controls drive local discovery/enrollment; no direct consent calls.
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
    dioxus::core::spawn_forever(Box::pin(async move {
        let value = match tokio::time::timeout(Duration::from_secs(140), run(state, &desktop)).await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Enrollment checker timed out"}),
        };
        eprintln!("protected-enrollment-check: {value}");
    }));
}
async fn ui_wait(dialog: bool) -> Result<(), String> {
    for _ in 0..150 {
        let value=eval(format!("const d=document.getElementById('protected-enrollment-dialog'),s=document.getElementById('protected-enrollment-status');dioxus.send({{ok:true,ready:{}}});await dioxus.recv();",if dialog{"!!d&&!!document.getElementById('protected-enrollment-submit')"}else{"!d&&!!s&&!!document.getElementById('protected-enrollment-reload')&&!document.getElementById('protected-enrollment-reload').disabled"})).await?;
        if value["ready"] == true {
            return frames().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Enrollment surface did not settle".into())
}
async fn click(control: &str, text: &str) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await protectedEnrollmentClick({}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("check_dom.js"),json!({"control":control,"text":text}))).await
}
async fn surface(page: &str) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await protectedEnrollmentSurface({}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("check_dom.js"),json!({"page":page}))).await
}
async fn type_text(state: Signal<ShellState>, b: &fixture::Binding) -> Result<Value, String> {
    let before = b.routed.load(Ordering::Acquire);
    let value = native(state, b.target(), "type", Some(fixture::NEWER)).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Acquire) > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Acquire) == before + 1 && b.refused.load(Ordering::Acquire) == 0,
        "Native input did not route exactly once",
    )?;
    fixture::flush_inputs(b).await?;
    Ok(value)
}
fn pages(scenario: &str, stage: &str) -> Vec<&'static str> {
    if stage == "dialog" {
        return if scenario == "failed-editor-channel" {
            vec!["channel", "thread", "pending"]
        } else {
            vec!["channel", "thread"]
        };
    }
    if matches!(stage, "prepared" | "enqueued")
        || stage == "interleaved" && scenario != "canceled-waiter-channel"
    {
        vec!["modal"]
    } else {
        vec!["status"]
    }
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    b: &fixture::Binding,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    snapshots: &mut Vec<Value>,
) -> Result<Value, String> {
    let mut observations = Vec::new();
    for page in pages(&b.scenario, stage) {
        let geometry = surface(page).await?;
        let name = if stage == "dialog" {
            format!("{stage}-{page}")
        } else {
            stage.into()
        };
        let path = b.capture_path(&name).await?;
        eprintln!(
            "protected-enrollment-stage: {}",
            json!({"stage":name,"edge":"before"})
        );
        let (path, _, _) =
            crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop)
                .await?;
        captures.insert(name.clone(), json!(path.to_string_lossy()));
        eprintln!(
            "protected-enrollment-stage: {}",
            json!({"stage":name,"edge":"after"})
        );
        observations.push(geometry);
    }
    let field = native(state, b.target(), "read", None).await?;
    let composer = if pages(&b.scenario, stage) == vec!["status"] {
        dom(b.target(), "proof", None).await?
    } else {
        Value::Null
    };
    let durable = fixture::durable(b).await?;
    let projection = fixture::projection(&state.peek(), b);
    ensure(
        projection["isolated"] == true
            && projection["sessionPresent"] == false
            && projection["transportPresent"] == false
            && projection["cachePresent"] == false,
        "Enrollment fixture isolation changed",
    )?;
    let value = json!({"stage":stage,"pages":observations,"native":field,"composer":composer,"durable":durable,"projection":projection});
    snapshots.push(value.clone());
    Ok(value)
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(state).await?;
    fixture::wait_runner(&b).await?;
    let target = b.target();
    let mounted = home(state, target).await?;
    native(state, target, "focus", None).await?;
    ui_wait(false).await?;
    let mut captures = serde_json::Map::new();
    let mut snapshots = Vec::new();
    let mut details = serde_json::Map::new();
    details.insert("nativeInitial".into(), mounted);
    capture(state, desktop, &b, "initial", &mut captures, &mut snapshots).await?;
    if b.scenario == "failed-editor-channel" {
        fixture::arm_failure(&b).await?;
        details.insert("typed".into(), type_text(state, &b).await?);
        capture(state, desktop, &b, "input", &mut captures, &mut snapshots).await?;
    }
    details.insert("open".into(), click("open", "Review draft").await?);
    ui_wait(true).await?;
    capture(state, desktop, &b, "dialog", &mut captures, &mut snapshots).await?;
    details.insert("submit".into(), click("submit", "Enable recovery").await?);
    if b.scenario != "failed-editor-channel" {
        let gate = if b.scenario == "canceled-waiter-channel" {
            "enqueued"
        } else {
            "prepared"
        };
        b.wait_gate(gate).await?;
        let held = capture(state, desktop, &b, gate, &mut captures, &mut snapshots).await?;
        if b.scenario == "late-typing-channel" {
            details.insert("typed".into(), type_text(state, &b).await?);
            capture(
                state,
                desktop,
                &b,
                "interleaved",
                &mut captures,
                &mut snapshots,
            )
            .await?;
        } else if matches!(
            b.scenario.as_str(),
            "owner-change-channel" | "canceled-waiter-channel"
        ) {
            if b.scenario == "canceled-waiter-channel" {
                details.insert("cancel".into(), click("cancel", "Cancel waiting").await?);
                details.insert("canceled".into(), b.wait_result().await?);
                ui_wait(false).await?;
            }
            let before = fixture::projection(&state.peek(), &b);
            fixture::replace_shell(state, &b)?;
            ready(state, target, &held["native"]["native"]["visit"]).await?;
            details.insert(
                "replacement".into(),
                json!({"before":before,"after":fixture::projection(&state.peek(),&b)}),
            );
            capture(
                state,
                desktop,
                &b,
                "interleaved",
                &mut captures,
                &mut snapshots,
            )
            .await?;
        }
        if b.scenario == "canceled-waiter-channel" {
            details.insert("check".into(), click("check", "Check status").await?);
        } else {
            b.release_gate();
        }
    }
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let result = b.wait_result().await?;
            if b.scenario != "canceled-waiter-channel" || result["kind"] != "canceled" {
                return Ok::<Value, String>(result);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| "Actual Check status did not publish its result")??;
    details.insert("result".into(), result);
    ui_wait(false).await?;
    capture(state, desktop, &b, "result", &mut captures, &mut snapshots).await?;
    let count = fixture::projection(&state.peek(), &b)["fixtureActions"]
        .as_array()
        .ok_or("Missing action ledger")?
        .len();
    details.insert(
        "reload".into(),
        click("reload", "Reload saved drafts").await?,
    );
    let mut reloaded = false;
    for _ in 0..150 {
        let p = fixture::projection(&state.peek(), &b);
        if p["discovery"]["status"] == "Ready"
            && p["fixtureActions"].as_array().is_some_and(|a| {
                a.len() == count + 2 && a.last().is_some_and(|v| v["kind"] == "current-load")
            })
        {
            reloaded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        reloaded,
        "Explicit Reload saved drafts did not publish Current inventory",
    )?;
    ui_wait(false).await?;
    let final_snapshot = capture(
        state,
        desktop,
        &b,
        "reloaded",
        &mut captures,
        &mut snapshots,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_|"Missing alias")?,"scenario":b.scenario,"phase":"exercise","nonce":b.nonce,"pid":std::process::id(),"isolatedRoot":b.root.to_string_lossy(),"target":label(target),"viewport":final_snapshot["native"]["native"]["viewport"],"captures":captures,"snapshots":snapshots,"details":details,"finalInventory":final_snapshot["durable"],"finalProjection":final_snapshot["projection"],"evidenceScope":"protected local discovery/enrollment consent; fixture construction has no HTTP capability"}),
    )
}
