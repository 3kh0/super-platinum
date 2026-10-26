use super::*;
mod calendar;
mod guards;
fn visible(p: &Value, open: bool) -> Result<(), String> {
    if p["effect"]["popup"]["open"] != open {
        return Err(format!("Composite popup visibility differs: {p}"));
    }
    Ok(())
}
fn month(p: &Value, value: &str) -> Result<(), String> {
    visible(p, true)?;
    if p["effect"]["popup"]["proof"]["month"] != value {
        return Err(format!("Composite calendar month differs: {p}"));
    }
    Ok(())
}
fn clear(p: &Value, expected: bool) -> Result<(), String> {
    let found = p["effect"]["popup"]["buttons"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r["action"] == "clear"));
    if found != expected {
        return Err("Composite clear policy differs".into());
    }
    Ok(())
}
fn escape(p: &Value) -> Result<(), String> {
    visible(p, false)?;
    let i = &p["intent"];
    let q = &i["propagation"];
    if i["prevented"] != false
        || q["documentCapture"] != 1
        || q["popup"] != 1
        || q["documentBubble"] != 0
        || q["windowBubble"] != 0
        || q["stoppedAtPopup"] != true
        || q["localMarker"] != true
    {
        return Err(format!("Composite Escape leaked: {p}"));
    }
    Ok(())
}
async fn take(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    block: &str,
    p: Value,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    observed(s, d).await?;
    shot(s, d, n, stage, block, json!({"intent":p}), st, cap).await
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    if case() == "guards" {
        guards::run(s, d, n, st, cap).await
    } else {
        calendar::run(s, d, n, st, cap).await
    }
}
