//! Exact disconnected appearance fixtures; never persists settings.
use crate::state::{Overlay, SettingsSection, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
fn fixture() -> Option<String> {
    if std::env::var("SUPER_PLATINUM_CONTRAST_CHECK").as_deref() != Ok("1") {
        return None;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").ok()?;
    matches!(
        name.as_str(),
        "appearance-countertop"
            | "appearance-blue-steel"
            | "appearance-paper-bag"
            | "appearance-custom-background"
    )
    .then_some(name)
}
fn isolated(state: &ShellState) -> Result<(), String> {
    if fixture().is_some()
        && crate::fixture::is_fixture()
        && state.core.session.is_none()
        && state.core.transport.is_none()
    {
        Ok(())
    } else {
        Err("Contrast check requires exact disconnected appearance fixture".into())
    }
}
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    let Some(name) = fixture() else {
        return;
    };
    if isolated(&state.peek()).is_err() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop, &name).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("{name}-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<Value, String> {
    wait(desktop, "({ok:!!document.querySelector('.jump-to .hint') && !!document.querySelector('.rail-badge,.ping-badge')})").await?;
    isolated(&state.peek())?;
    let shell = sample(desktop).await?;
    stage(state, name, "shell", 0).await?;
    isolated(&state.peek())?;
    {
        let mut shell = state.write();
        shell.overlay = Some(Overlay::Settings);
        shell.settings_section = SettingsSection::Appearance;
    }
    wait(
        desktop,
        "({ok:!!document.querySelector('.settings-grid > button')})",
    )
    .await?;
    isolated(&state.peek())?;
    let settings = sample(desktop).await?;
    stage(state, name, "settings", 1).await?;
    isolated(&state.peek())?;
    state.write().overlay = None;
    wait(desktop, "({ok:!document.querySelector('.settings-grid')})").await?;
    isolated(&state.peek())?;
    let layout = eval(
        desktop,
        "({ok:!document.querySelector('[data-contrast-probe]'),viewport:[innerWidth,innerHeight]})",
    )
    .await?;
    if layout["ok"] != true {
        return Err("Contrast probes remained mounted".into());
    }
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"sampledActual":true,"sampledProbes":true,"shell":shell,"settings":settings,"layout":layout}),
    )
}
async fn sample(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let value = eval(desktop, include_str!("sample.js")).await?;
    if value["ok"] == true {
        Ok(value)
    } else {
        Err(format!("Computed contrast failed: {value}"))
    }
}
async fn wait(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<(), String> {
    for _ in 0..80 {
        if eval(desktop, script).await?["ok"] == true {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("Contrast fixture did not mount expected controls".into())
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Native evaluation timed out")?
        .ok_or("Native callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid encoded native callback".into())
        }
        value => Ok(value),
    }
}
async fn stage(
    state: Signal<ShellState>,
    name: &str,
    stage: &str,
    ordinal: usize,
) -> Result<(), String> {
    isolated(&state.peek())?;
    // Reuse the ordered pixel-ack protocol with the runner's fresh private directory.
    let directory = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Contrast capture requires private acknowledgement directory")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    let ack = directory.join(format!("{ordinal}.ack"));
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    isolated(&state.peek())?;
    eprintln!(
        "{name}-stage: {}",
        json!({"ok":true,"stage":stage,"ordinal":ordinal,"noSession":true,"noTransport":true})
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        let read = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Capture acknowledgement task failed")?;
        isolated(&state.peek())?;
        match read {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Capture acknowledgement malformed".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgement unreadable".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Capture acknowledgement timed out at {stage}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
