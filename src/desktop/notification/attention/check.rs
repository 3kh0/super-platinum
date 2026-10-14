//! Explicit isolated bare-dev AppKit invocation proof, never a Slack preference write.
use super::*;
use serde_json::json;
use std::time::{Duration, Instant};

pub(crate) async fn run(state: Signal<ShellState>) {
    if std::env::var("SUPER_PLATINUM_DOCK_ATTENTION_CHECK").as_deref() != Ok("1")
        || std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() != Ok("channel")
    {
        return;
    }
    let result = check(state).await;
    let receipt = match result {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    };
    eprintln!("dock-attention-check: {receipt}");
}

fn isolated(state: Signal<ShellState>) -> bool {
    let shell = state.peek();
    crate::fixture::is_fixture() && shell.core.session.is_none() && shell.core.transport.is_none()
}

async fn check(state: Signal<ShellState>) -> Result<serde_json::Value, &'static str> {
    if !isolated(state) {
        return Err("The attention check requires a disconnected fixture");
    }
    // Hidden, not merely temporarily inactive during startup: the runner must
    // explicitly hide its owned process before this check can issue attention.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let hidden = MainThreadMarker::new()
            .is_some_and(|mtm| NSApplication::sharedApplication(mtm).isHidden());
        if hidden && !app_active() {
            break;
        }
        if Instant::now() >= deadline {
            return Err("The owned fixture did not become hidden and inactive");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut modes = Vec::new();
    for (name, mode) in [
        ("once", DockAttention::Once),
        ("untilActive", DockAttention::UntilActive),
    ] {
        let status = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(read_focus),
        )
        .await;
        let status = match status {
            Ok(Ok(status)) => status,
            _ => focus::Status::Unknown,
        };
        if !isolated(state) || app_active() {
            return Err("Fixture isolation or inactive application ownership changed");
        }
        let focus = match status {
            focus::Status::Available => "available",
            focus::Status::Quiet => "quiet",
            focus::Status::Unknown => "unknown",
        };
        if status != focus::Status::Available {
            modes.push(json!({"mode":name,"focus":focus,"suppressed":true,"accepted":false,"cancelled":false}));
            continue;
        }
        let Some(cancel) = native_request(mode) else {
            return Err("AppKit did not return an attention request while inactive");
        };
        // No await or retained attention between these two native invocations.
        // The exact returned AppKit ID is closed over by this one-shot closure.
        cancel();
        modes.push(
            json!({"mode":name,"focus":focus,"suppressed":false,"accepted":true,"cancelled":true}),
        );
    }
    let complete = modes
        .iter()
        .all(|m| m["accepted"] == true && m["cancelled"] == true);
    Ok(
        json!({"ok":true,"noTransport":true,"noPreferenceWrites":true,"focusRespected":true,"immediateCancellation":true,"appKitInvocationProven":complete,"visualBounceProven":false,"modes":modes,"proofGap":if complete {"Visual Dock bounce and real-message delivery were not exercised."} else {"Actual Focus state suppressed one or more modes; AppKit invocation remains unverified for them."}}),
    )
}
