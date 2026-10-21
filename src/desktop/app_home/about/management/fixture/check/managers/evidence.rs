//! Original inline confirmation, ordered responses, and retained account custody.
use super::*;

pub(super) async fn native(desktop: &DesktopContext, capture: bool) -> Result<Value, String> {
    let script =
        include_str!("native.js").replace("CAPTURE", if capture { "true" } else { "false" });
    let mut last = Value::Null;
    for _ in 0..60 {
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        desktop
            .webview
            .evaluate_script_with_callback(&script, move |value| {
                let _ = send.send(value);
            })
            .map_err(|_| "Managers native evaluation failed")?;
        let raw = tokio::time::timeout(Duration::from_secs(2), receive.recv())
            .await
            .map_err(|_| "Managers native evaluation timed out")?
            .ok_or("Managers native callback closed")?;
        last = serde_json::from_str(&raw).map_err(|_| "Invalid Managers native callback")?;
        if let Value::String(encoded) = last {
            last =
                serde_json::from_str(&encoded).map_err(|_| "Invalid encoded Managers callback")?;
        }
        if last["ok"] == true {
            // Binding detail is failure-only; accepted receipt ABI remains exact.
            if let Some(settings) = last.get_mut("settings").and_then(Value::as_object_mut) {
                settings.remove("binding");
                settings.remove("managerBinding");
            }
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("Managers native assertion timed out: last={last}"))
}
pub(super) async fn observe(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let native = native(desktop, false).await?;
    let s = state.peek();
    Ok(
        json!({"model":snapshot(&s),"permissions":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"retainedDocument":retained(&s,visit),"native":native}),
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
            "Held Managers changed without delivery: before={before} after={after}"
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
        return Err("Managers synthetic isolation changed".into());
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
        return Err("Managers capture must be fresh".into());
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
            "Managers capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"permissions":observed["permissions"],"server":observed["server"],"retainedDocument":observed["retainedDocument"],"native":observed["native"],"exercise":exercise}}));
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
    wait(state, "explicit Managers readback", |v| {
        v["busy"] == false
            && v["reads"].as_u64()
                == Some(
                    reads
                        + if endpoint::case() == "managers-no-trigger" {
                            9
                        } else {
                            11
                        },
                )
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

fn retained(s: &ShellState, visit: u64) -> Value {
    let receipt = product::snapshot(s, visit);
    let id = receipt["document"]["id"]
        .as_u64()
        .or_else(|| receipt["attempts"][0]["document"].as_u64());
    s.app_home.about.management.documents.iter().find(|d|Some(d.id)==id).map(|d|json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger()})).unwrap_or(Value::Null)
}
