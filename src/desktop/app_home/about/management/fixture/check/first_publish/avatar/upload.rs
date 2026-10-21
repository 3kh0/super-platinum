//! Actual chooser and upload pane abandonment. Original server work is retained.
use super::*;

pub(super) async fn choose(
    desktop: &DesktopContext,
    name: &str,
    serial: u64,
) -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/desktop/app_home/about/management/fixture/first_publish")
        .join(if endpoint::case() == "first-avatar-crop-drag-retry" {
            "avatar-wide.png"
        } else {
            "avatar.png"
        });
    eprintln!(
        "workflow-first-publish-chooser: {}",
        json!({
        "pid":std::process::id(),"fixture":name,"case":endpoint::case(),
        "path":path,"input":"workflow-finish-upload","serial":serial})
    );
    click(desktop, "workflow-finish-upload").await
}

pub(super) async fn back(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    button: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, button).await?;
    until(state, visit, "original avatar details restored", |p, _| {
        let a = &p["confirmation"]["local"]["avatar"];
        a["pane"] == "details"
            && a["paneBound"] == true
            && a["pending"] == false
            && a["notice"].is_null()
            && a["uploaded"].is_null()
    })
    .await?;
    let after = avatar_ready(state, desktop, visit).await?;
    retained(&before, &after, false)?;
    if before["server"] != after["server"]
        || before["firstPublish"]["confirmation"]["local"]["avatar"]["receipts"]
            != after["firstPublish"]["confirmation"]["local"]["avatar"]["receipts"]
    {
        return Err("Avatar Back changed original server or receipts".into());
    }
    Ok(json!({"action":button,"before":before,"after":after}))
}

pub(super) async fn start(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Option<Value>, String> {
    let before = observe(state, desktop, visit).await?;
    choose(desktop, name, 1).await?;
    let case = endpoint::case();
    if case == "first-avatar-upload-cancel-held" {
        held(state, desktop, name, visit, before, captures, stages).await?;
        return Ok(None);
    }
    if !matches!(
        case.as_str(),
        "first-avatar-upload-back" | "first-avatar-upload-retry"
    ) {
        return Ok(Some(before));
    }
    until(state, visit, "original upload rejected", |p, server| {
        let a = &p["confirmation"]["local"]["avatar"];
        a["pane"] == "upload"
            && a["status"] == "error"
            && a["pending"] == false
            && a["paneBound"] == true
            && a["notice"] == "Sorry! Something went wrong. Please try again."
            && server["responses"].as_array().is_some_and(|rows| {
                rows.last()
                    .is_some_and(|r| r["result"]["raw"]["error"] == "invalid_image")
            })
            && server["requests"]
                .as_array()
                .zip(server["settlements"].as_array())
                .is_some_and(|(a, b)| a.len() == b.len())
    })
    .await?;
    let after = avatar_ready(state, desktop, visit).await?;
    let quiet_evidence = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "upload-rejected",
        visit,
        json!({"before":before,"after":after,"quiet":quiet_evidence}),
        captures,
        stages,
    )
    .await?;
    let retry = case == "first-avatar-upload-retry";
    let exercise = back(
        state,
        desktop,
        visit,
        if retry {
            "workflow-finish-upload-retry"
        } else {
            "workflow-finish-upload-back"
        },
    )
    .await?;
    stage(
        state,
        desktop,
        name,
        if retry {
            "retry-details"
        } else {
            "back-details"
        },
        visit,
        exercise,
        captures,
        stages,
    )
    .await?;
    if !retry {
        return Ok(None);
    }
    let before = observe(state, desktop, visit).await?;
    choose(desktop, name, 2).await?;
    Ok(Some(before))
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
    until(state, visit, "original upload held", |p, s| {
        let a = &p["confirmation"]["local"]["avatar"];
        a["pane"] == "upload"
            && a["status"] == "uploading"
            && a["pending"] == true
            && a["paneBound"] == true
            && s["held"].as_array().is_some_and(|v| v.len() == 1)
    })
    .await?;
    let after = avatar_ready(state, desktop, visit).await?;
    let serial = after["server"]["held"][0]
        .as_u64()
        .ok_or("Held upload serial absent")? as usize;
    let quiet_evidence = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "upload-held",
        visit,
        json!({"before":before,"after":after,"quiet":quiet_evidence}),
        captures,
        stages,
    )
    .await?;
    let exercise =
        actions::cancel_button(state, desktop, visit, "workflow-finish-upload-cancel").await?;
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
    let before = observe(state, desktop, visit).await?;
    click(desktop, "workflow-management-finish-up").await?;
    until(
        state,
        visit,
        "fresh Finish Up after upload cancellation",
        |p, _| {
            p["confirmation"]["current"] == true
                && p["confirmation"]["local"]["valid"] == true
                && p["confirmation"]["local"]["avatar"]["paneBound"] == true
        },
    )
    .await?;
    let remount = wait_dom(desktop, r#"(()=>{
        const old=window.__firstPublishRegistry?.modal,m=document.getElementById('workflow-finish-up'),
        title=document.getElementById('workflow-finish-title'),description=document.getElementById('workflow-finish-description'),upload=document.getElementById('workflow-finish-upload');
        const p={originalDetached:!!old&&!old.root.isConnected&&!old.title.isConnected&&!old.description.isConnected&&!old.upload.isConnected,
        newRoot:!!m&&m!==old?.root,newTitle:!!title&&title!==old?.title,newDescription:!!description&&description!==old?.description,newUpload:!!upload&&upload!==old?.upload};
        return{ok:Object.values(p).every(Boolean),...p};})()"#).await?;
    native(desktop, "modal").await?;
    let successor = observe(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "reopened-finish",
        visit,
        json!({"before":before,"after":successor,"remount":remount}),
        captures,
        stages,
    )
    .await?;
    let release = original.release(serial)?;
    if release["delivered"] != true || release["receiverRetired"] != false {
        return Err("Original dispatched upload reply receiver changed".into());
    }
    let count = successor["firstPublish"]["avatarReceipts"]
        .as_array()
        .ok_or("Avatar receipts absent")?
        .len();
    until(
        state,
        visit,
        "original abandoned upload receipt retained",
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
        return Err("Abandoned upload changed successor or dispatched work".into());
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
