//! Actual same-identity Try again. No hidden action/controller invocation.
use super::*;
use backend::resend as endpoint_resend;
pub(crate) fn requested() -> bool {
    endpoint_resend::requested() && !backend::posting::requested()
}
pub(super) fn script(mode: &str, expected: &Value) -> String {
    include_str!("resend/native.js")
        .replace("GEOMETRY", include_str!("drafts/geometry.js"))
        .replace("MODE", &json!(mode).to_string())
        .replace("EXPECTED", &expected.to_string())
}
pub(crate) fn wrap(base: &str) -> String {
    format!(
        "(()=>{{const base={base},retry={};return{{...base,ok:base.ok&&retry.ok,shareResendRow:retry.row,shareResendOriginal:retry.original}}}})()",
        script("observe", &Value::Null)
    )
}
fn retry(v: &Value) -> &Value {
    last(&v["firstPublish"])["confirmationShare"]["operation"]["resends"]
        .as_array()
        .and_then(|v| v.last())
        .unwrap_or(&Value::Null)
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    label: &str,
) -> Result<Value, String> {
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute resend capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let check = path.clone();
    if tokio::task::spawn_blocking(move || check.try_exists())
        .await
        .map_err(|_| "Resend capture worker failed")?
        .map_err(|_| "Resend capture path failed")?
    {
        return Err("Resend capture must be fresh".into());
    }
    let before = observe(state, desktop, visit).await?;
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err("Resend capture changed original observation".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(
        json!({"path":path.to_string_lossy(),"pngDimensions":[width,height],"before":before,"after":after}),
    )
}
pub(crate) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
) -> Result<Value, String> {
    if !requested() {
        return Err("Explicit resend fixture required".into());
    }
    let case = endpoint_resend::case();
    let before = observe(state, desktop, visit).await?;
    let row = &before["native"]["shareResendRow"];
    if case == "suppressed" {
        if !row["resend"].is_null()
            || row["cancel"]["text"] != "Cancel"
            || row["cancel"]["hitPoint"].is_null()
        {
            return Err("Read-only failure must expose only Cancel".into());
        }
        let quiet = quiet(state, desktop, visit).await?;
        if quiet["before"] != before || quiet["after"] != before {
            return Err("Suppressed resend changed original observation".into());
        }
        let cap = capture(state, desktop, name, visit, "resend-suppressed").await?;
        if cap["before"] != before {
            return Err("Suppressed resend capture changed original observation".into());
        }
        return Ok(
            json!({"before":before,"activation":null,"held":null,"blocked":null,"release":null,"after":before,"quiet":quiet,"captureEvidence":{"resend-suppressed":cap},"captures":{"resend-suppressed":cap["path"]}}),
        );
    }
    let original = backend::capture(&state.peek())?;
    let activation = evaluate(desktop, &script("click", row)).await?;
    until(
        state,
        visit,
        "original explicit resend response held",
        |p, s| {
            let r = last(p)["confirmationShare"]["operation"]["resends"]
                .as_array()
                .and_then(|v| v.last());
            r.is_some_and(|v| v["finished"] == false)
                && s["confirmationAdd"]["share"]["resend"]["held"] == json!([1])
        },
    )
    .await?;
    let held = observe(state, desktop, visit).await?;
    let pending = &held["native"]["shareResendRow"];
    if pending["messageId"] != row["messageId"]
        || !pending["resend"].is_null()
        || !pending["cancel"].is_null()
    {
        return Err("Sending resend exposes recovery action or changes identity".into());
    }
    let blocked = json!({"kind":"passive-original-control-absence","resendAbsent":true,"cancelAbsent":true,"actionDispatched":false,"quiet":quiet(state,desktop,visit).await?});
    if blocked["quiet"]["before"] != held || blocked["quiet"]["after"] != held {
        return Err("Held resend did not remain quiet".into());
    }
    let pending_capture = capture(state, desktop, name, visit, "resend-pending").await?;
    if pending_capture["before"] != held {
        return Err("Pending resend capture changed original observation".into());
    }
    let release = endpoint_resend::release(&original)?;
    until(state, visit, "original explicit resend settled", |p, _| {
        last(p)["confirmationShare"]["operation"]["resends"]
            .as_array()
            .and_then(|v| v.last())
            .is_some_and(|r| r["finished"] == true)
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if retry(&after)["finished"] != true {
        return Err("Original resend terminal absent".into());
    }
    let quiet = quiet(state, desktop, visit).await?;
    if quiet["before"] != after || quiet["after"] != after {
        return Err("Settled resend did not remain quiet".into());
    }
    let settled_capture = capture(state, desktop, name, visit, "resend-settled").await?;
    if settled_capture["before"] != after {
        return Err("Settled resend capture changed original observation".into());
    }
    Ok(
        json!({"before":before,"activation":activation,"held":held,"blocked":blocked,"release":release,"after":after,"quiet":quiet,
        "captureEvidence":{"resend-pending":pending_capture,"resend-settled":settled_capture},
        "captures":{"resend-pending":pending_capture["path"],"resend-settled":settled_capture["path"]}}),
    )
}
