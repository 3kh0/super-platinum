//! Original inline confirmation, ordered responses, and retained account custody.
use super::*;

pub(super) async fn native(desktop: &DesktopContext, capture: bool) -> Result<Value, String> {
    wait_dom(
        desktop,
        &include_str!("native.js").replace("CAPTURE", if capture { "true" } else { "false" }),
    )
    .await
}
pub(super) async fn observe(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let native = native(desktop, false).await?;
    let s = state.peek();
    Ok(
        json!({"model":snapshot(&s),"permissions":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"native":native}),
    )
}
pub(super) async fn until(
    state: Signal<ShellState>,
    visit: u64,
    label: &str,
    accept: impl Fn(&Value, &Value) -> bool,
) -> Result<(), String> {
    for _ in 0..200 {
        {
            let s = state.peek();
            if accept(&product::snapshot(&s, visit), &endpoint::snapshot(&s)) {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "{label}: permissions={} server={}",
        product::snapshot(&state.peek(), visit),
        endpoint::snapshot(&state.peek())
    ))
}
pub(super) async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err(format!(
            "Held Permissions changed without delivery: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after,"observedMs":500}))
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    visit: u64,
    exercise: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !isolated(&state.peek()) {
        return Err("Permissions synthetic isolation changed".into());
    }
    let observed = observe(state, desktop, visit).await?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Inspect capture worker failed")?
        .map_err(|_| "Inspect capture path failed")?
    {
        return Err("Permissions capture must be fresh".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    let after = observe(state, desktop, visit).await?;
    if observed != after {
        return Err(format!(
            "Permissions capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"permissions":observed["permissions"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}
pub(super) async fn checked(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let reads = before["model"]["reads"]
        .as_u64()
        .ok_or("Read count missing")?;
    click(desktop, "workflow-permissions-check").await?;
    wait(state, "explicit Permissions readback", |v| {
        v["busy"] == false && v["reads"].as_u64() == Some(reads + 11)
    })
    .await?;
    until(state, visit, "readback retained", |d, _| {
        !d["attempts"][0]["readback"].is_null()
    })
    .await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('workflow-management')?.__workflowManagement?.lease})"#,
    )
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after) || before["model"]["writes"] != after["model"]["writes"] {
        return Err("Readback lost draft or replayed mutation".into());
    }
    Ok(json!({"before":before,"after":after}))
}

pub(super) fn draft(v: &Value) -> bool {
    v["model"]["title"] == TITLE
        && v["model"]["description"] == DESCRIPTION
        && v["native"]["originalNodes"] == true
}
