use super::*;
pub(super) fn script(command: &str, stage: &str) -> String {
    include_str!("../known.js")
        .replace("__COMMAND__", &json!(command).to_string())
        .replace("__STAGE__", &json!(stage).to_string())
}
pub(super) async fn observe(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let proof = data::proof(&state.peek());
    capture::verify_snapshot(&proof, 2, true)?;
    if proof["resolver"]["requests"]
        .as_array()
        .is_none_or(|r| !r.is_empty())
    {
        return Err("Metadata refresh navigated or acquired a destination".into());
    }
    Ok(
        json!({"layout":eval(desktop,&script("state",name)).await?,"proof":proof,"acquisition":source::snapshot(),"reply":reply::fixture::attribution_snapshot(state).await?}),
    )
}
pub(super) fn stable(value: &Value) -> Value {
    let p = &value["proof"]["retained"];
    json!({"serial":p["serial"],"query":p["query"],"rows":p["rows"],"root":p["root"],"ownerCurrent":p["ownerCurrent"],"loading":p["loading"],"error":p["error"],"pendingFile":p["pendingFile"],"navigationError":p["navigationError"],"native":value["layout"]["native"],"sourceIdentity":value["layout"]["sourceIdentity"],"reply":value["reply"]})
}
pub(super) async fn quiet(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<Value, String> {
    eval(desktop, &script("scroll", name)).await?;
    let before = observe(state, desktop, name).await?;
    let started = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let after = observe(state, desktop, name).await?;
    if before != after {
        return Err(format!(
            "{name}: bounded quiet changed: before={before}; after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after,"observedMs":started.elapsed().as_millis()}))
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    baseline: Option<&Value>,
    exercise: Value,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    eval(desktop, &script("scroll", name)).await?;
    let before = observe(state, desktop, name).await?;
    if let Some(baseline) = baseline {
        if stable(&before) != *baseline {
            return Err(format!(
                "{name}: original root/native/SQLite custody changed: before={baseline}; after={}",
                stable(&before)
            ));
        }
    }
    let mut receipt = before.clone();
    receipt["ok"] = json!(true);
    receipt["stage"] = json!(name);
    receipt["ordinal"] = json!(stages.len());
    receipt["case"] = json!(source::case());
    receipt["noSession"] = json!(true);
    receipt["noTransport"] = json!(true);
    receipt["exercise"] = exercise;
    capture::emit(
        "canvas-known-file-freshness",
        name,
        receipt.clone(),
        stages.len(),
    )
    .await?;
    let after = observe(state, desktop, name).await?;
    if before != after {
        return Err(format!(
            "{name}: source/native/private draft changed during capture"
        ));
    }
    stages.push(receipt);
    Ok(before)
}
pub(super) async fn phase(
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<(), String> {
    wait(desktop,&format!("document.querySelector('[data-canvas-file-acquisition=\"{}\"]')?.getAttribute('data-phase')==={}",data::ROOT,json!(name))).await
}
pub(super) async fn requests(count: usize, completed: usize) -> Result<(), String> {
    for _ in 0..120 {
        let s = source::snapshot();
        if s["requests"].as_array().map(Vec::len) == Some(count)
            && s["settlements"].as_array().map(Vec::len) == Some(completed)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "Known request/settlement {count}/{completed} missing: {}",
        source::snapshot()
    ))
}
pub(super) async fn ready(
    desktop: &dioxus::desktop::DesktopContext,
    version: u64,
) -> Result<(), String> {
    wait(desktop,&format!("document.querySelector('[data-canvas-file-id=\"FCANON001\"]')?.dataset.canvasFileUrl==={}&&!document.querySelector('[data-canvas-file-acquisition=\"{}\"]')",json!(format!("https://example.test/canonical/fresh-{version}")),data::ROOT)).await
}
pub(super) fn event(mut state: Signal<ShellState>) -> Value {
    let before = crate::file_events::snapshot_revision(&state.peek());
    crate::file_events::invalidate(&mut state.write(), "T1", data::HOSTED);
    let after = crate::file_events::snapshot_revision(&state.peek());
    json!({"id":data::HOSTED,"team":"T1","before":before,"after":after})
}
pub(super) async fn receiver_closed(serial: usize) -> Result<(), String> {
    for _ in 0..120 {
        if source::snapshot()["held"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|r| r["serial"] == serial && r["receiverClosed"] == true)
        }) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("Original known receiver did not retire".into())
}
