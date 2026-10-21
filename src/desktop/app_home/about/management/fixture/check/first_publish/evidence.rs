use super::*;
pub(super) async fn settled_observe(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    // Older publication scenes now also mount the asynchronous Add panel.
    // Their stable snapshot must wait for that initial acquisition, just as
    // the dedicated Add driver already does before entering this function.
    if !endpoint::confirmation::requested()
        && last(&product::snapshot(&state.peek(), visit))["complete"] == true
    {
        until(
            state,
            visit,
            "confirmation channel acquisition settled",
            |p, _| {
                let add = &last(p)["confirmationAdd"];
                add.is_object() && add["status"] != "loading"
            },
        )
        .await?;
    }
    let expected = { product::snapshot(&state.peek(), visit) };
    let attempt = last(&expected);
    if attempt["finished"] != true || attempt["closed"] != false {
        return Err("Settled original publication required".into());
    }
    let dom = json!({"id":attempt["id"],"outcome":attempt["publication"]["outcome"],
        "notice":attempt["local"]["notice"]});
    let script = include_str!("settled.js").replace("EXPECTED", &dom.to_string());
    if endpoint::confirmation::requested() {
        observed_wait(
            state,
            desktop,
            visit,
            "settled publication predicate",
            &script,
        )
        .await?;
    } else {
        wait_dom(desktop, &script).await?;
    }
    let observed = observe(state, desktop, visit).await.map_err(|error| {
        if endpoint::confirmation::requested() {
            let s = state.peek();
            format!(
                "Post-settled original observation failed: {error}; product={} server={}",
                product::snapshot(&s, visit),
                endpoint::snapshot(&s)
            )
        } else {
            error
        }
    })?;
    if observed["firstPublish"] != expected {
        return Err(format!(
            "Settled publication changed while awaiting original DOM: before={expected} after={}",
            observed["firstPublish"]
        ));
    }
    Ok(observed)
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
        return Err("First-Publish synthetic isolation changed".into());
    }
    let observed = observe(state, desktop, visit).await?;
    if crate::channel_workflows::details::fixture::requested()
        && matches!(
            label,
            "about"
                | "editor"
                | "draft"
                | "finish"
                | "details"
                | "confirmed"
                | "checked"
                | "selected"
        )
    {
        stages.push(json!({"stage":label,"model":observed["model"],"geometry":observed["native"],"proof":{"firstPublish":observed["firstPublish"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
        return Ok(());
    }
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
        return Err("First-Publish capture must be fresh".into());
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
            "First-Publish capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"firstPublish":observed["firstPublish"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}

/// Same native predicate/timebound; retain the final passive sample on failure.
pub(super) async fn observed_wait(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    boundary: &str,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        desktop
            .webview
            .evaluate_script_with_callback(script, move |value| {
                let _ = send.send(value);
            })
            .map_err(|_| "Native evaluation failed")?;
        let raw = tokio::time::timeout(Duration::from_secs(2), receive.recv())
            .await
            .map_err(|_| "Native evaluation timed out")?
            .ok_or("Native callback closed")?;
        last = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
        if let Value::String(encoded) = last {
            last = serde_json::from_str(&encoded).map_err(|_| "Invalid encoded callback")?;
        }
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let s = state.peek();
    Err(format!(
        "Add native assertion timed out at {boundary}: last={last}; product={} server={}",
        product::snapshot(&s, visit),
        endpoint::snapshot(&s)
    ))
}
