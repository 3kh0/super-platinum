//! Six original native states; selected-map publication never supplies reply authority.
use super::super::emoji_fixture as data;
use super::*;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
mod ready;
static STARTED: AtomicBool = AtomicBool::new(false);
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !data::requested() || STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result =
            match tokio::time::timeout(Duration::from_secs(180), run(state, &desktop)).await {
                Ok(Ok(v)) => v,
                Ok(Err(error)) => json!({"ok":false,"error":error}),
                Err(_) => json!({"ok":false,"error":"Canvas emoji timeout"}),
            };
        eprintln!("canvas-comment-emoji-check: {result}");
    });
}
async fn call(
    desktop: &dioxus::desktop::DesktopContext,
    action: &str,
    mode: &str,
    view: &str,
) -> Result<Value, String> {
    eval_raw(
        desktop,
        &format!(
            "({})({},{},{})",
            include_str!("emoji/native.js"),
            json!(action),
            json!(mode),
            json!(view)
        ),
    )
    .await
}
async fn native(
    desktop: &dioxus::desktop::DesktopContext,
    action: &str,
    mode: &str,
) -> Result<Value, String> {
    native_view(desktop, action, mode, "all").await
}
async fn native_view(
    desktop: &dioxus::desktop::DesktopContext,
    action: &str,
    mode: &str,
    view: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..120 {
        last = call(desktop, action, mode, view).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("Original native state unavailable: {last}"))
}
async fn saved(state: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..120 {
        let disk = reply::fixture::emoji_snapshot(state).await?;
        if disk["runtime"]["loading"] == false
            && disk["runtime"]["saving"] == false
            && disk["runtime"]["error"].is_null()
            && disk["runtime"]["blocks"].is_array()
            && disk["journal"]["drafts"][0]["blocks"] == disk["runtime"]["blocks"]
        {
            return Ok(disk);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Original rich draft not durable".into())
}
async fn ack(ordinal: usize) -> Result<(), String> {
    let base = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Missing ACK directory")?;
    for _ in 0..600 {
        let path = base.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK worker")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Invalid capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("ACK read failed".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Capture ACK timeout".into())
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    mode: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("canvas-comment-emoji-narrow") {
        let views: &[&str] = if name.starts_with("stream-") {
            &["root", "preview"]
        } else {
            &["root", "reply", "editor"]
        };
        for view in views {
            take(state, desktop, name, mode, view, proof, stages).await?;
        }
        Ok(())
    } else {
        take(state, desktop, name, mode, "all", proof, stages).await
    }
}
async fn take(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    mode: &str,
    view: &str,
    proof: &Value,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let exposure = if view == "all" {
        Value::Null
    } else {
        native_view(desktop, "expose", mode, view).await?
    };
    let mut model = Value::Null;
    let mut layout = Value::Null;
    for _ in 0..120 {
        layout = native_view(desktop, "observe", mode, view).await?;
        model = data::snapshot(&state.peek());
        let selected = &model["retained"]["selected"];
        if model["retained"]["ownerCurrent"] == true
            && model["retained"]["loading"] == false
            && model["retained"]["error"].is_null()
            && ((mode == "unobserved" && selected.is_null() && model["selected"].is_null())
                || (mode != "unobserved"
                    && selected["current"] == true
                    && *selected == model["selected"]))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if model["retained"]["ownerCurrent"] != true
        || model["retained"]["loading"] != false
        || model["retained"]["selected"] != model["selected"]
    {
        return Err("Original source/read projection did not settle".into());
    }
    let disk = reply::fixture::emoji_snapshot(state).await?;
    if layout["stream"] == false
        && (disk["runtime"]["blocks"] != layout["draft"]["blocks"]
            || disk["journal"]["drafts"][0]["blocks"] != layout["draft"]["blocks"])
    {
        return Err("Original native/durable draft differs".into());
    }
    let ordinal = stages.len();
    let mut receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":false,"noTransport":false,"isolation":data::isolation(&state.peek()),"model":model,"layout":layout,"reply":disk,"proof":proof});
    if view != "all" {
        receipt["stage"] = json!(format!("{name}-{view}"));
        receipt["logicalStage"] = json!(name);
        receipt["view"] = json!(view);
        receipt["exposure"] = exposure;
    }
    eprintln!("canvas-comment-emoji-stage: {receipt}");
    ack(ordinal).await?;
    let after = native_view(desktop, "observe", mode, view).await?;
    if after["draft"] != layout["draft"]
        || after["rows"] != layout["rows"]
        || reply::fixture::emoji_snapshot(state).await? != disk
        || data::snapshot(&state.peek())["reads"] != model["reads"]
    {
        return Err("Original source/native/disk changed during capture".into());
    }
    stages.push(receipt);
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    ready::wait_runner(state).await?;
    wait(desktop,"document.querySelectorAll('.canvas-comment').length===3 && document.querySelector('.canvas-comments-scroll')?.getAttribute('aria-busy')==='false'").await?;
    let mut proof = json!({"stream":native(desktop,"stream-arm","").await?});
    let mut stages = vec![];
    stage(
        state,
        desktop,
        "stream-unobserved",
        "unobserved",
        &proof,
        &mut stages,
    )
    .await?;
    proof["assignment1"] = data::assignments(state).await?;
    stage(
        state,
        desktop,
        "stream-assigned-pending",
        "pending",
        &proof,
        &mut stages,
    )
    .await?;
    proof["media"] = data::media(&state.peek())?;
    stage(
        state,
        desktop,
        "stream-assigned-ready",
        "ready",
        &proof,
        &mut stages,
    )
    .await?;
    click(
        desktop,
        "[data-comment-ts='1719800000.000100'] .canvas-comment-open",
    )
    .await?;
    wait(desktop,"!!document.getElementById('canvas-comment-reply') && document.getElementById('canvas-comment-reply').getAttribute('contenteditable')==='true'").await?;
    proof["input"] = native(desktop, "input", "").await?;
    saved(state).await?;
    proof["draft"] = native(desktop, "draft-arm", "").await?;
    stage(
        state,
        desktop,
        "opened-thread-draft",
        "ready",
        &proof,
        &mut stages,
    )
    .await?;
    proof["retirement"] = data::retire(&mut state.write())?;
    stage(
        state,
        desktop,
        "socket-retired",
        "unobserved",
        &proof,
        &mut stages,
    )
    .await?;
    proof["assignment2"] = data::assignments(state).await?;
    stage(
        state,
        desktop,
        "fresh-assigned-ready",
        "ready",
        &proof,
        &mut stages,
    )
    .await?;
    let layout = stages.last().unwrap()["layout"].clone();
    Ok(
        json!({"ok":true,"noSession":false,"noTransport":false,"isolation":data::isolation(&state.peek()),"stages":stages,"layout":layout,"model":data::snapshot(&state.peek()),"reply":reply::fixture::emoji_snapshot(state).await?,"syntheticInput":true,"requestCount":2,"previewRetirementNative":false}),
    )
}
