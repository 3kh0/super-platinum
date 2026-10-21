use super::*;
mod cropper;
mod upload;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let result = if cropper::enabled() {
        cropper::run(state, desktop, name, visit, captures, stages).await
    } else {
        avatar_inner(state, desktop, name, visit, captures, stages).await
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let diagnostics = native(desktop, "diagnostic").await;
            let s = state.peek();
            Err(format!(
                "{error}; avatar native={diagnostics:?}; product={}; server={}",
                product::snapshot(&s, visit),
                endpoint::snapshot(&s)
            ))
        }
    }
}

async fn avatar_inner(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let Some(before) = upload::start(state, desktop, name, visit, captures, stages).await? else {
        return Ok(());
    };
    until(state, visit, "original uploaded image", |p, _| {
        p["confirmation"]["local"]["avatar"]["pending"] == false
            && p["confirmation"]["local"]["avatar"]["cropBound"] == true
            && p["confirmation"]["local"]["avatar"]["cropper"]["ready"] == true
            && p["confirmation"]["local"]["avatar"]["paneBound"] == true
            && p["confirmation"]["local"]["avatar"]["uploaded"]["id"] == "AVATAR_FIRST"
    })
    .await?;
    let uploaded = avatar_ready(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "uploaded",
        visit,
        json!({"before":before,"after":uploaded}),
        captures,
        stages,
    )
    .await?;
    if endpoint::case() == "first-avatar-crop-back" {
        let exercise = upload::back(state, desktop, visit, "workflow-finish-crop-back").await?;
        stage(
            state,
            desktop,
            name,
            "back-details",
            visit,
            exercise,
            captures,
            stages,
        )
        .await?;
        return Ok(());
    }
    for (id, value) in [
        ("workflow-finish-crop-size", "128"),
        ("workflow-finish-crop-x", "64"),
        ("workflow-finish-crop-y", "64"),
    ] {
        input(desktop, id, value).await?;
        let key = id.rsplit('-').next().unwrap();
        let expected = value.parse::<u64>().unwrap();
        until(state, visit, &format!("actual crop value {id}"), |p, _| {
            p["confirmation"]["local"]["avatar"]["crop"][key].as_f64() == Some(expected as f64)
        })
        .await?;
    }
    let mut crop = observe(state, desktop, visit).await?;
    click(desktop, "workflow-finish-crop-apply").await?;
    let case = endpoint::case();
    if matches!(
        case.as_str(),
        "first-avatar-crop-retry" | "first-avatar-crop-cancel"
    ) {
        until(state, visit, "original crop rejected", |p, server| {
            let a = &p["confirmation"]["local"]["avatar"];
            a["pending"] == false
                && a["notice"] == "Sorry! Something went wrong. Please try again."
                && a["receipts"].as_array().is_some_and(|v| {
                    v.last()
                        .is_some_and(|v| v["kind"] == "crop" && v["raw"]["error"] == "invalid_crop")
                })
                && server["requests"]
                    .as_array()
                    .zip(server["settlements"].as_array())
                    .is_some_and(|(a, b)| a.len() == b.len())
        })
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
        if case == "first-avatar-crop-cancel" {
            click(desktop, "workflow-finish-crop-cancel").await?;
            until(state, visit, "original crop cancelled", |p, _| {
                let a = &p["confirmation"]["local"]["avatar"];
                a["pending"] == false && a["uploaded"].is_null() && a["notice"].is_null()
            })
            .await?;
            let cancelled = avatar_ready(state, desktop, visit).await?;
            retained(&rejected, &cancelled, false)?;
            if rejected["server"] != cancelled["server"]
                || rejected["firstPublish"]["confirmation"]["local"]["avatar"]["receipts"]
                    != cancelled["firstPublish"]["confirmation"]["local"]["avatar"]["receipts"]
            {
                return Err("Crop Cancel changed original server or receipts".into());
            }
            stage(
                state,
                desktop,
                name,
                "crop-cancelled",
                visit,
                json!({"before":rejected,"after":cancelled}),
                captures,
                stages,
            )
            .await?;
            return Ok(());
        }
        crop = rejected;
        click(desktop, "workflow-finish-crop-apply").await?;
    }
    until(state, visit, "original crop acknowledged", |p, _| {
        p["confirmation"]["local"]["iconUrl"] == "https://example.test/first-publish-crop.png"
            && p["confirmation"]["local"]["avatar"]["pending"] == false
            && p["confirmation"]["local"]["avatar"]["uploaded"].is_null()
            && p["confirmation"]["local"]["avatar"]["notice"].is_null()
    })
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
    .await?;
    Ok(())
}

// Compare original values and object identities, not mutable outcome/epoch fields.
fn retained(before: &Value, after: &Value, crop_present: bool) -> Result<(), String> {
    let b = &before["firstPublish"]["confirmation"];
    let a = &after["firstPublish"]["confirmation"];
    let bn = &before["native"]["modal"];
    let an = &after["native"]["modal"];
    if before["model"] != after["model"]
        || before["native"]["fields"] != after["native"]["fields"]
        || ["id", "visit", "document", "revision"]
            .iter()
            .any(|k| b[*k] != a[*k])
        || ["title", "description", "iconUrl"]
            .iter()
            .any(|k| b["local"][*k] != a["local"][*k])
        || bn["nodeId"] != an["nodeId"]
        || an["originalNodes"] != true
        || bn["upload"]["nodeId"] != an["upload"]["nodeId"]
        || bn["upload"]["files"] != an["upload"]["files"]
        || bn["custody"]["file"] != an["custody"]["file"]
        || (crop_present
            && (bn["custody"]["cropNodes"] != an["custody"]["cropNodes"]
                || b["local"]["avatar"]["uploaded"] != a["local"]["avatar"]["uploaded"]
                || b["local"]["avatar"]["crop"] != a["local"]["avatar"]["crop"]))
    {
        return Err("Crop recovery changed original draft, File, or controls".into());
    }
    Ok(())
}

async fn avatar_ready(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let expected = { product::snapshot(&state.peek(), visit) };
    let c = &expected["confirmation"];
    if c["current"] != true || c["local"]["avatar"]["paneBound"] != true {
        return Err("Current bound avatar pane required".into());
    }
    wait_dom(
        desktop,
        &include_str!("avatar-ready.js").replace("EXPECTED", &c.to_string()),
    )
    .await?;
    let observed = observe(state, desktop, visit).await?;
    if observed["firstPublish"] != expected {
        return Err(format!(
            "Avatar model changed while awaiting original DOM: before={expected} after={}",
            observed["firstPublish"]
        ));
    }
    Ok(observed)
}
