//! First local-only native acceptance through actual input and composer Send.
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
        let result = tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Protected check timed out"}),
        };
        eprintln!("protected-drafts-check: {receipt}");
    });
}

async fn settled(b: &fixture::Binding, failed: bool) -> Result<(), String> {
    // Never return or retain the Editor Arc across the account-away test.
    let editor = b.editor(b.target())?;
    let result = editor.flush().await;
    let snapshot = editor.snapshot();
    ensure(!snapshot.running, "Protected writer still running")?;
    if failed {
        ensure(
            result.is_err()
                && snapshot.error == Some(super::super::durable::Error::Storage)
                && snapshot.failed_inputs.len() == 1,
            "Expected retained actual writer failure",
        )
    } else {
        result.map_err(|e| format!("Protected input flush: {e:?}"))?;
        ensure(
            snapshot.error.is_none()
                && snapshot.pending.is_none()
                && snapshot.failed_inputs.is_empty(),
            "Protected writer did not settle cleanly",
        )
    }
}
async fn type_text(b: &fixture::Binding, text: &str, failed: bool) -> Result<Value, String> {
    let routed = b.routed.load(Ordering::Acquire);
    let value = dom(b.target(), "type", Some(text)).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Acquire) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Acquire) == routed + 1,
        "Actual input was not routed exactly once",
    )?;
    ensure(
        b.refused.load(Ordering::Acquire) == 0,
        "Isolated input was refused",
    )?;
    settled(b, failed).await?;
    ensure(
        value["native"]["text"] == text
            && value["native"]["display"] == text
            && value["fieldFocused"] == true,
        "Native input differs or lost focus",
    )?;
    Ok(value)
}
async fn released(target: ComposerTarget) -> Result<Value, String> {
    for _ in 0..400 {
        let value = dom(target, "read", None).await?;
        if value["readOnly"] == false && value["admission"].is_null() {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Actual native admission did not release".into())
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    b: &fixture::Binding,
    stage: &str,
    locked: bool,
    captures: &mut serde_json::Map<String, Value>,
    snapshots: &mut Vec<Value>,
) -> Result<Value, String> {
    let geometry = dom(b.target(), if locked { "read" } else { "proof" }, None).await?;
    ensure(
        geometry["readOnly"] == locked,
        "Unexpected actual admission state at capture",
    )?;
    let durable = fixture::durable(b).await?;
    let projection = fixture::projection(&state.peek(), b);
    ensure(
        projection["isolated"] == true
            && projection["sessionPresent"] == false
            && projection["transportPresent"] == false
            && projection["cachePresent"] == false,
        "Fixture construction isolation lost",
    )?;
    let snapshot =
        json!({"stage":stage,"geometry":geometry,"durable":durable,"projection":projection});
    let directory = std::env::var_os("SUPER_PLATINUM_PROTECTED_DRAFTS_CAPTURE_DIR")
        .ok_or("Missing protected capture directory")?;
    let alias =
        std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing protected fixture alias")?;
    let path = std::path::PathBuf::from(directory)
        .join(format!("{alias}-{}-exercise-{stage}.png", b.scenario));
    eprintln!(
        "protected-drafts-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "protected-drafts-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    snapshots.push(snapshot.clone());
    Ok(snapshot)
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(state).await?;
    fixture::wait_runner(&b).await?;
    let target = b.target();
    let mounted = home(state, target).await?;
    dom(target, "focus", None).await?;
    let mut captures = serde_json::Map::new();
    let mut snapshots = Vec::new();
    let mut details = serde_json::Map::new();
    details.insert("nativeInitial".into(), mounted);
    let initial = capture(
        state,
        desktop,
        &b,
        "initial",
        false,
        &mut captures,
        &mut snapshots,
    )
    .await?;
    let failure = b.scenario == "failed-write-return-channel";
    if failure {
        fixture::arm_failure(&b).await?;
    }
    let typed = type_text(&b, fixture::REQUESTED, failure).await?;
    details.insert("typed".into(), typed);
    capture(
        state,
        desktop,
        &b,
        "input",
        false,
        &mut captures,
        &mut snapshots,
    )
    .await?;
    if failure {
        let failed = capture(
            state,
            desktop,
            &b,
            "failed",
            false,
            &mut captures,
            &mut snapshots,
        )
        .await?;
        details.insert("away".into(), fixture::rotate_away(state, &b).await?);
        ready(state, target, &failed["geometry"]["native"]["visit"]).await?;
        dom(target, "focus", None).await?;
        let away = capture(
            state,
            desktop,
            &b,
            "away",
            false,
            &mut captures,
            &mut snapshots,
        )
        .await?;
        details.insert("returned".into(), fixture::return_owner(state, &b)?);
        ready(state, target, &away["geometry"]["native"]["visit"]).await?;
        dom(target, "focus", None).await?;
        capture(
            state,
            desktop,
            &b,
            "returned",
            false,
            &mut captures,
            &mut snapshots,
        )
        .await?;
        details.insert("retry".into(), fixture::retry_input(&b).await?);
        capture(
            state,
            desktop,
            &b,
            "retried",
            false,
            &mut captures,
            &mut snapshots,
        )
        .await?;
        type_text(&b, fixture::NEWER, false).await?;
        capture(
            state,
            desktop,
            &b,
            "newer",
            false,
            &mut captures,
            &mut snapshots,
        )
        .await?;
    } else {
        // The actual DOM button enters the existing composer callback. The
        // opted-in callback invokes shared native admission and typed consume.
        details.insert("clicked".into(), controls(target, true).await?);
        b.wait_held().await?;
        dom(target, "focus", None).await?;
        let admitted = capture(
            state,
            desktop,
            &b,
            "admitted",
            true,
            &mut captures,
            &mut snapshots,
        )
        .await?;
        ensure(
            admitted["geometry"]["native"]["text"] == fixture::REQUESTED,
            "Admitted predecessor changed",
        )?;
        if b.scenario == "expired-clear-thread" {
            let expired = released(target).await?;
            ensure(
                same_native(&admitted["geometry"]["native"], &expired["native"]),
                "Lease expiry changed native predecessor",
            )?;
            capture(
                state,
                desktop,
                &b,
                "expired",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            type_text(&b, fixture::NEWER, false).await?;
            let newer = capture(
                state,
                desktop,
                &b,
                "newer",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            b.release_gate();
            let result = b.wait_result().await?;
            ensure(
                result["kind"] == "completed" && result["nativeClear"] == "Deferred",
                "Expired clear was not deferred",
            )?;
            details.insert("result".into(), result);
            let final_value = capture(
                state,
                desktop,
                &b,
                "settled",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            ensure(
                same_native(
                    &newer["geometry"]["native"],
                    &final_value["geometry"]["native"],
                ),
                "Late clear replaced newer input",
            )?;
            ensure(
                newer["durable"] == final_value["durable"],
                "Late clear changed newer durable input",
            )?;
        } else if b.scenario == "canceled-clear-channel" {
            b.cancel_waiter()?;
            released(target).await?;
            details.insert("result".into(), b.wait_result().await?);
            capture(
                state,
                desktop,
                &b,
                "canceled",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            type_text(&b, fixture::NEWER, false).await?;
            let newer = capture(
                state,
                desktop,
                &b,
                "newer",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            b.release_gate();
            let completed = b.wait_completed().await?;
            ensure(
                completed["kind"] == "completed"
                    && completed["nativeClear"] == "Deferred"
                    && completed["operationStatus"] == "ConsumedAwaitingDelivery",
                "Canceled waiter lost retained operation",
            )?;
            details.insert("canceledWaiter".into(), details["result"].clone());
            details.insert("result".into(), completed);
            let settled = capture(
                state,
                desktop,
                &b,
                "settled",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            ensure(
                same_native(&newer["geometry"]["native"], &settled["geometry"]["native"]),
                "Canceled late clear changed native input",
            )?;
            ensure(
                newer["durable"] == settled["durable"],
                "Canceled late clear changed durable input",
            )?;
        } else {
            b.release_gate();
            let result = b.wait_result().await?;
            ensure(
                result["kind"] == "completed" && result["nativeClear"] == "Cleared",
                "Actual clear ACK missing",
            )?;
            details.insert("result".into(), result);
            released(target).await?;
            let cleared = capture(
                state,
                desktop,
                &b,
                "settled",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
            ensure(
                cleared["geometry"]["native"]["text"] == "",
                "Positive native clear did not land",
            )?;
            type_text(&b, fixture::NEWER, false).await?;
            capture(
                state,
                desktop,
                &b,
                "newer",
                false,
                &mut captures,
                &mut snapshots,
            )
            .await?;
        }
    }
    ensure(
        b.refused.load(Ordering::Acquire) == 0,
        "Unexpected refused native input",
    )?;
    let final_inventory = fixture::durable(&b).await?;
    let other = if target == ComposerTarget::Thread {
        "channel"
    } else {
        "thread"
    };
    ensure(
        initial["durable"][other] == final_inventory[other],
        "Unrelated protected draft changed",
    )?;
    let projection = fixture::projection(&state.peek(), &b);
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").ok(),"scenario":b.scenario,
        "phase":"exercise","nonce":b.nonce,"pid":std::process::id(),"isolatedRoot":b.root,
        "target":label(target),"viewport":snapshots[0]["geometry"]["viewport"],"captures":captures,
        "snapshots":snapshots,"details":details,"finalInventory":final_inventory,"finalProjection":projection,
        "evidenceScope":"protected local checkpoint/consume/native-clear; fixture construction has no HTTP capability"}),
    )
}
