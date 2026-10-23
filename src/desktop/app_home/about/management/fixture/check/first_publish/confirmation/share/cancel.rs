//! Explicit local cancellation through the original rendered failed Share row.
use super::*;

pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_WORKFLOW_SHARE_CANCEL_CHECK").as_deref() == Ok("1")
        && drafts::requested()
        && matches!(
            std::env::var(backend::CASE_ENV).as_deref(),
            Ok("rejected" | "lost-response")
        )
}

fn script(mode: &str, expected: &Value) -> String {
    include_str!("cancel/native.js")
        .replace("GEOMETRY", include_str!("drafts/geometry.js"))
        .replace("MODE", &json!(mode).to_string())
        .replace("EXPECTED", &expected.to_string())
}

pub(crate) fn wrap(base: &str) -> String {
    format!(
        "(()=>{{const base={base},cancel={};return{{...base,ok:base.ok&&cancel.ok,shareCancelRow:cancel.row,shareCancelOriginal:cancel.original}}}})()",
        script("observe", &Value::Null)
    )
}

pub(crate) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
) -> Result<Value, String> {
    if !requested() {
        return Err("Explicit failed Share cancellation case required".into());
    }
    let before = observe(state, desktop, visit).await?;
    let operation = &last(&before["firstPublish"])["confirmationShare"]["operation"];
    let ts = operation["message"]["localTs"]
        .as_str()
        .ok_or("Original Share local timestamp absent")?
        .to_string();
    let row = &before["native"]["shareCancelRow"];
    if operation["finished"] != true
        || operation["queued"] != true
        || !matches!(operation["outcome"].as_str(), Some("rejected" | "unknown"))
        || row["row"]["messageId"] != ts
        || row["originalNodes"] != true
    {
        return Err(format!(
            "Original failed Share row not cancellable: {before}"
        ));
    }
    let activation = super::evaluate(desktop, &script("click", row)).await?;
    let mut settled = false;
    for _ in 0..100 {
        let s = state.peek();
        let custody = super::snapshot(&s);
        let absent = custody["messages"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["ts"] != ts))
            && custody["pending"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|r| r != &json!(ts)))
            && custody["delivery"].get(ts.as_str()).is_none();
        drop(s);
        if absent {
            settled = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !settled {
        return Err("Original local Share row/pending/status was not removed".into());
    }
    wait_dom(desktop, &script("removed", &json!({"localTs":ts}))).await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"] {
        return Err("Local Share Cancel changed server ledger".into());
    }
    let quiet = quiet(state, desktop, visit).await?;
    if quiet["before"] != after || quiet["after"] != after {
        return Err("Local Share Cancel did not remain quiet".into());
    }
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute Share Cancel capture directory required")?;
    let path = directory.join(format!("{name}-share-cancelled.png"));
    let check = path.clone();
    if tokio::task::spawn_blocking(move || check.try_exists())
        .await
        .map_err(|_| "Cancel capture worker failed")?
        .map_err(|_| "Cancel capture path failed")?
    {
        return Err("Cancel capture must be fresh".into());
    }
    let capture_before = observe(state, desktop, visit).await?;
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":"share-cancelled","edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    let capture_after = observe(state, desktop, visit).await?;
    if capture_before != capture_after || capture_before != after {
        return Err("Cancel capture changed original observation".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":"share-cancelled","edge":"after"})
    );
    Ok(
        json!({"before":before,"activation":activation,"after":after,"quiet":quiet,
        "capture":{"path":path.to_string_lossy(),"pngDimensions":[width,height],"before":capture_before,"after":capture_after},
        "captures":{"share-cancelled":path.to_string_lossy()}}),
    )
}
