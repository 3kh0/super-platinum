//! Disconnected fixture receipts. Taskbar pixels are captured by the Python owner.
use super::{
    Controller, Owner,
    windows::{Sink, Snapshot},
};
use crate::state::ShellState;
use dioxus::{desktop::DesktopContext, prelude::*};
use serde_json::{Value, json};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
use super_platinum_core::state::DockBadge;

pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_TASKBAR_BADGE_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel")
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: DesktopContext,
    snapshot: impl Fn() -> Option<Snapshot>,
) {
    let projection = std::env::var("SUPER_PLATINUM_TASKBAR_BADGE_CHECK").as_deref()
        == Ok("projection")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel");
    if !requested() && !projection {
        return;
    }
    let result = if projection {
        projection_check(state, snapshot).await
    } else {
        check(state, desktop).await
    }
    .unwrap_or_else(|error| json!({"ok":false,"error":error}));
    eprintln!("taskbar-badge-check: {result}");
}

fn isolated(state: Signal<ShellState>) -> Result<(), String> {
    super::projection_check::require_isolated(state)
}

async fn checkpoint(
    stage: &str,
    snapshot: Snapshot,
    writes: usize,
    ordinal: usize,
) -> Result<Value, String> {
    let directory = PathBuf::from(
        std::env::var("SUPER_PLATINUM_TASKBAR_BADGE_ACK_DIR")
            .map_err(|_| "Taskbar check requires its runner acknowledgement directory")?,
    );
    let ack = directory.join(format!("{ordinal}.ack"));
    let receipt = snapshot.native;
    if receipt.last_error.is_some() || receipt.label != receipt.desired_label {
        return Err(format!(
            "Native taskbar state failed at {stage}: {receipt:?}"
        ));
    }
    // Allow the shell compositor a frame; the runner captures actual taskbar pixels.
    tokio::time::sleep(Duration::from_millis(250)).await;
    let value = json!({"stage":stage,"ordinal":ordinal,"hwnd":snapshot.hwnd,
        "native":receipt,"writes":writes});
    eprintln!("taskbar-badge-stage: {value}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let path = ack.clone();
        if tokio::task::spawn_blocking(move || path.is_file())
            .await
            .map_err(|e| e.to_string())?
        {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Taskbar capture acknowledgement timed out at {stage}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn require_identity() -> Result<(), String> {
    // Main startup must isolate fixture grouping from the user's live app.
    use windows::Win32::{
        System::Com::CoTaskMemFree, UI::Shell::GetCurrentProcessExplicitAppUserModelID,
    };
    let id = unsafe { GetCurrentProcessExplicitAppUserModelID() }.map_err(|e| e.to_string())?;
    let app_id = unsafe { id.to_string() }.map_err(|e| e.to_string());
    unsafe {
        CoTaskMemFree(Some(id.0.cast()));
    }
    if app_id? != "com.echonet.superplatinum.fixture.badge" {
        return Err("Taskbar fixture needs an isolated process AUMID".into());
    }
    Ok(())
}

async fn check(state: Signal<ShellState>, desktop: DesktopContext) -> Result<Value, String> {
    isolated(state)?;
    require_identity()?;
    let sink = Sink::new(&desktop)?;
    let recorded = Rc::new(RefCell::new(Vec::new()));
    let writes = recorded.clone();
    let output = sink.clone();
    let mut controller = Controller::with_fallible_writer(move |label| {
        output.write(label.clone())?;
        writes.borrow_mut().push(label);
        Ok(())
    });
    // Give the initial real TaskbarButtonCreated notification time to settle.
    tokio::time::sleep(Duration::from_millis(750)).await;
    let first = Some(Owner {
        account: "fixture-a".into(),
        epoch: 1,
    });
    let second = Some(Owner {
        account: "fixture-b".into(),
        epoch: 2,
    });
    let mut stages = Vec::new();
    for (name, owner, badge, expected, expected_writes) in [
        ("clear", None, DockBadge::Empty, None, 1),
        ("number", first.clone(), DockBadge::Count(42), Some("42"), 1),
        (
            "unchanged",
            first.clone(),
            DockBadge::Count(42),
            Some("42"),
            0,
        ),
        ("dot", first.clone(), DockBadge::Dot, Some("•"), 1),
        ("zero", first.clone(), DockBadge::Count(0), None, 1),
        (
            "newNumber",
            first.clone(),
            DockBadge::Count(7),
            Some("7"),
            1,
        ),
        (
            "accountChange",
            second.clone(),
            DockBadge::Count(7),
            Some("7"),
            2,
        ),
        ("retired", None, DockBadge::Count(999), None, 1),
        ("beforeDrop", second, DockBadge::Dot, Some("•"), 1),
    ] {
        isolated(state)?;
        let before = recorded.borrow().len();
        controller.update(owner, badge);
        let count = recorded.borrow().len() - before;
        if count != expected_writes || sink.receipt().label.as_deref() != expected {
            return Err(format!(
                "Unexpected taskbar transition at {name}: {:?}",
                sink.receipt()
            ));
        }
        stages.push(checkpoint(name, sink.snapshot(), count, stages.len()).await?);
        if name == "number" {
            let before = sink.receipt();
            sink.simulate_recreation();
            let after = sink.receipt();
            if after.successes != before.successes + 1
                || after.recreations != before.recreations + 1
                || after.label != before.label
            {
                return Err(
                    "Synthetic TaskbarButtonCreated did not reapply desired overlay".into(),
                );
            }
            stages.push(checkpoint("syntheticRecreation", sink.snapshot(), 0, stages.len()).await?);
        }
    }
    drop(controller);
    if sink.receipt().label.is_some() {
        return Err("Controller drop did not clear taskbar".into());
    }
    stages.push(checkpoint("dropClear", sink.snapshot(), 1, stages.len()).await?);
    if recorded.borrow().len() != 10 {
        return Err("Unexpected controller write count".into());
    }
    Ok(
        json!({"ok":true,"noTransport":true,"noPreferenceWrites":true,
        "isolatedAppId":true,"nativeHresultSuccess":true,"unchangedNoWrite":true,
        "ownerClear":true,"dropClear":true,"syntheticTaskbarRecovery":true,
        "actualExplorerRestart":false,"visualTaskbarPixelsProven":false,
        "writes":10,"stages":stages}),
    )
}

async fn projection_check(
    state: Signal<ShellState>,
    snapshot: impl Fn() -> Option<Snapshot>,
) -> Result<Value, String> {
    isolated(state)?;
    require_identity()?;
    let mut stages = Vec::new();
    for step in super::projection_check::STEPS {
        let before = snapshot()
            .map(|sample| sample.native.successes)
            .unwrap_or(0);
        step.apply(state)?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let observed = loop {
            isolated(state)?;
            let cached = super::label(state.peek().dock_badge_value());
            if let Some(sample) = snapshot() {
                if production_matches(&sample, cached.as_deref(), step.expected(), before) {
                    break sample;
                }
                if sample.native.last_error.is_some() {
                    return Err(format!(
                        "Production badge failed at {}: {:?}",
                        step.name(),
                        sample.native
                    ));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "Production badge hook timed out at {}: cached={cached:?}, native={:?}",
                    step.name(),
                    snapshot()
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let writes = observed.native.successes - before;
        let mut stage = checkpoint(step.name(), observed, writes, stages.len()).await?;
        stage["cachedLabel"] = json!(super::label(state.peek().dock_badge_value()));
        stages.push(stage);
    }
    Ok(
        json!({"ok":true,"mode":"projection","noTransport":true,"noPreferenceWrites":true,
        "isolatedAppId":true,"productionHook":true,"singleProductionSink":true,
        "nativeHresultSuccess":true,"filteredActivity":true,"authoritativeZero":true,
        "laterEligibility":true,"ownerClear":true,"visualTaskbarPixelsProven":false,
        "actualExplorerRestart":false,"stages":stages}),
    )
}

fn production_matches(
    sample: &Snapshot,
    cached: Option<&str>,
    expected: Option<&str>,
    previous_successes: usize,
) -> bool {
    sample.hwnd != 0
        && cached == expected
        && sample.native.label.as_deref() == expected
        && sample.native.desired_label.as_deref() == expected
        && sample.native.last_error.is_none()
        && sample.native.successes > previous_successes
        && sample.native.attempts == sample.native.successes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_observation_requires_new_success_and_current_cached_value() {
        let mut sample = Snapshot {
            hwnd: 1,
            native: super::super::windows::Receipt {
                label: Some("6".into()),
                desired_label: Some("6".into()),
                attempts: 2,
                successes: 2,
                ..Default::default()
            },
        };
        assert!(production_matches(&sample, Some("6"), Some("6"), 1));
        assert!(!production_matches(&sample, Some("6"), Some("6"), 2));
        assert!(!production_matches(&sample, None, Some("6"), 1));
        sample.native.desired_label = Some("7".into());
        assert!(!production_matches(&sample, Some("6"), Some("6"), 1));
        sample.native.desired_label = Some("6".into());
        sample.native.last_error = Some("native failure".into());
        assert!(!production_matches(&sample, Some("6"), Some("6"), 1));
    }
}
