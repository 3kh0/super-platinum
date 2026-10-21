//! Original rendered crop selection gestures and retained asynchronous replies.
use super::*;

pub(super) fn enabled() -> bool {
    matches!(
        endpoint::case().as_str(),
        "first-avatar-crop-drag"
            | "first-avatar-crop-drag-retry"
            | "first-avatar-crop-back-held"
            | "first-avatar-crop-cancel-held"
    )
}

async fn gesture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    action: &str,
) -> Result<Value, String> {
    let before = avatar_ready(state, desktop, visit).await?;
    let script = include_str!("cropper.js").replace("ACTION", &json!(action).to_string());
    // Dispatch once. A failed action is never replayed by a polling helper.
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(&script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Original crop gesture evaluation failed")?;
    let raw = tokio::time::timeout(Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Original crop gesture callback timed out")?
        .ok_or("Crop gesture callback closed")?;
    let mut proof: Value =
        serde_json::from_str(&raw).map_err(|_| "Invalid crop gesture callback")?;
    if let Value::String(encoded) = proof {
        proof =
            serde_json::from_str(&encoded).map_err(|_| "Invalid encoded crop gesture callback")?;
    }
    if proof["ok"] != true {
        return Err(format!("Original crop gesture refused: {proof}"));
    }
    let epoch = before["firstPublish"]["confirmation"]["local"]["epoch"]
        .as_u64()
        .ok_or("Original local epoch absent")?;
    let completed = until(state, visit, "completed original crop gesture", |p, _| {
        let l = &p["confirmation"]["local"];
        l["epoch"].as_u64().is_some_and(|v| v > epoch)
            && l["avatar"]["cropper"]["ready"] == true
            && l["avatar"]["cropper"]["interactionPending"] == false
            && l["avatar"]["paneBound"] == true
    })
    .await;
    if let Err(error) = completed {
        return Err(format!("{error}; original one-shot gesture={proof}"));
    }
    let after = avatar_ready(state, desktop, visit).await?;
    retained(&before, &after, false)?;
    if before["server"] != after["server"]
        || before["firstPublish"]["confirmation"]["local"]["avatar"]["generation"]
            != after["firstPublish"]["confirmation"]["local"]["avatar"]["generation"]
    {
        return Err("Gesture changed server work or original avatar generation".into());
    }
    let quiet = quiet(state, desktop, visit).await?;
    Ok(json!({"before":before,"gesture":proof,"after":after,"quiet":quiet}))
}

async fn uploaded(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    until(
        state,
        visit,
        "original reply image and cropper ready",
        |p, _| {
            let a = &p["confirmation"]["local"]["avatar"];
            a["pending"] == false
                && a["cropBound"] == true
                && a["paneBound"] == true
                && a["uploaded"]["id"] == "AVATAR_FIRST"
                && a["cropper"]["ready"] == true
                && a["cropper"]["interactionPending"] == false
        },
    )
    .await?;
    avatar_ready(state, desktop, visit).await
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let before = observe(state, desktop, visit).await?;
    upload::choose(desktop, name, 1).await?;
    let ready = uploaded(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "uploaded",
        visit,
        json!({"before":before,"after":ready}),
        captures,
        stages,
    )
    .await?;
    for (action, label) in [("resize", "resized"), ("drag", "dragged")] {
        let exercise = gesture(state, desktop, visit, action).await?;
        stage(
            state, desktop, name, label, visit, exercise, captures, stages,
        )
        .await?;
    }
    let before = observe(state, desktop, visit).await?;
    click(desktop, "workflow-finish-crop-apply").await?;
    let case = endpoint::case();
    if matches!(
        case.as_str(),
        "first-avatar-crop-back-held" | "first-avatar-crop-cancel-held"
    ) {
        return held(state, desktop, name, visit, before, captures, stages).await;
    }
    let mut crop = before;
    if case == "first-avatar-crop-drag-retry" {
        until(
            state,
            visit,
            "original dragged crop rejected",
            |p, server| {
                let a = &p["confirmation"]["local"]["avatar"];
                a["pending"] == false
                    && a["notice"] == "Sorry! Something went wrong. Please try again."
                    && a["receipts"].as_array().is_some_and(|v| {
                        v.last().is_some_and(|r| {
                            r["kind"] == "crop" && r["raw"]["error"] == "invalid_crop"
                        })
                    })
                    && server["requests"]
                        .as_array()
                        .zip(server["settlements"].as_array())
                        .is_some_and(|(a, b)| a.len() == b.len())
            },
        )
        .await?;
        let rejected = avatar_ready(state, desktop, visit).await?;
        retained(&crop, &rejected, true)?;
        let quiet = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "crop-rejected",
            visit,
            json!({"before":crop,"after":rejected,"quiet":quiet}),
            captures,
            stages,
        )
        .await?;
        crop = rejected;
        click(desktop, "workflow-finish-crop-apply").await?;
    }
    until(
        state,
        visit,
        "original dragged crop acknowledged",
        |p, _| {
            let l = &p["confirmation"]["local"];
            l["iconUrl"] == "https://example.test/first-publish-crop.png"
                && l["avatar"]["pending"] == false
                && l["avatar"]["uploaded"].is_null()
                && l["avatar"]["notice"].is_null()
                && l["avatar"]["paneBound"] == true
        },
    )
    .await?;
    let after = avatar_ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "cropped",
        visit,
        json!({"before":crop,"after":after}),
        captures,
        stages,
    )
    .await
}

async fn held(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    before: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let original = endpoint::capture(&state.peek())?;
    until(state, visit, "original dispatched crop held", |p, s| {
        let a = &p["confirmation"]["local"]["avatar"];
        a["pane"] == "crop"
            && a["pending"] == true
            && a["paneBound"] == true
            && s["held"].as_array().is_some_and(|v| v.len() == 1)
    })
    .await?;
    let dispatched = avatar_ready(state, desktop, visit).await?;
    let serial = dispatched["server"]["held"][0]
        .as_u64()
        .ok_or("Held crop serial absent")? as usize;
    let pending_gesture = gesture(state, desktop, visit, "pending-drag").await?;
    let after = observe(state, desktop, visit).await?;
    if before["firstPublish"]["confirmation"]["local"]["avatar"]["crop"]
        == after["firstPublish"]["confirmation"]["local"]["avatar"]["crop"]
        || dispatched["server"] != after["server"]
    {
        return Err("Pending gesture did not retain immutable dispatched crop".into());
    }
    let quiet_evidence = quiet(state, desktop, visit).await?;
    stage(state, desktop, name, "crop-held", visit, json!({"before":before,"dispatched":dispatched,"pendingGesture":pending_gesture,"after":after,"quiet":quiet_evidence}), captures, stages).await?;
    let cancel = endpoint::case() == "first-avatar-crop-cancel-held";
    let exercise = upload::back(
        state,
        desktop,
        visit,
        if cancel {
            "workflow-finish-crop-cancel"
        } else {
            "workflow-finish-crop-back"
        },
    )
    .await?;
    stage(
        state,
        desktop,
        name,
        if cancel {
            "crop-cancelled"
        } else {
            "back-details"
        },
        visit,
        exercise,
        captures,
        stages,
    )
    .await?;
    if cancel {
        let exercise = actions::cancel(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "cancelled",
            visit,
            exercise,
            captures,
            stages,
        )
        .await?;
        reopen(state, desktop, name, visit, captures, stages).await?;
    }
    let before = observe(state, desktop, visit).await?;
    upload::choose(desktop, name, 2).await?;
    let successor = uploaded(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "reentered-crop",
        visit,
        json!({"before":before,"after":successor}),
        captures,
        stages,
    )
    .await?;
    let release = original.release(serial)?;
    if release["delivered"] != true || release["receiverRetired"] != false {
        return Err("Original dispatched crop receiver changed".into());
    }
    let count = successor["firstPublish"]["avatarReceipts"]
        .as_array()
        .ok_or("Avatar receipts absent")?
        .len();
    until(
        state,
        visit,
        "abandoned original crop receipt retained",
        |p, s| {
            p["avatarReceipts"]
                .as_array()
                .is_some_and(|r| r.len() == count + 1)
                && s["settlements"].as_array().is_some_and(|r| {
                    r.iter()
                        .any(|r| r["serial"] == serial && r["returned"] == true)
                })
        },
    )
    .await?;
    let after = observe(state, desktop, visit).await?;
    if successor["model"] != after["model"]
        || successor["native"] != after["native"]
        || ["confirmation", "document", "attempts"]
            .iter()
            .any(|k| successor["firstPublish"][*k] != after["firstPublish"][*k])
        || ["requests", "responses", "current"]
            .iter()
            .any(|k| successor["server"][*k] != after["server"][*k])
    {
        return Err("Abandoned crop changed successor or dispatched work".into());
    }
    let quiet_evidence = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "late-settled",
        visit,
        json!({"before":successor,"release":release,"after":after,"quiet":quiet_evidence}),
        captures,
        stages,
    )
    .await
}

async fn reopen(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, "workflow-management-finish-up").await?;
    until(
        state,
        visit,
        "fresh Finish Up after crop cancellation",
        |p, _| {
            p["confirmation"]["current"] == true
                && p["confirmation"]["local"]["valid"] == true
                && p["confirmation"]["local"]["avatar"]["paneBound"] == true
        },
    )
    .await?;
    let remount = wait_dom(desktop, r#"(()=>{const old=window.__firstPublishRegistry?.modal,m=document.getElementById('workflow-finish-up'),title=document.getElementById('workflow-finish-title'),description=document.getElementById('workflow-finish-description'),upload=document.getElementById('workflow-finish-upload');const p={originalDetached:!!old&&!old.root.isConnected&&!old.title.isConnected&&!old.description.isConnected&&!old.upload.isConnected,newRoot:!!m&&m!==old?.root,newTitle:!!title&&title!==old?.title,newDescription:!!description&&description!==old?.description,newUpload:!!upload&&upload!==old?.upload};return{ok:Object.values(p).every(Boolean),...p};})()"#).await?;
    native(desktop, "modal").await?;
    let after = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "reopened-finish",
        visit,
        json!({"before":before,"after":after,"remount":remount}),
        captures,
        stages,
    )
    .await
}
