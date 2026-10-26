//! Live selected-prop regression for the shared ordinary/composite calendar.
use super::*;
async fn take(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    stage: &str,
    p: Value,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    observed(d).await?;
    shot(s, d, n, stage, "quiet", json!({"intent":p}), st, cap).await
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = probe("open", "quiet", json!({})).await?;
    take(s, d, n, "opened", p, st, cap).await?;
    let p = probe("input", "quiet", json!({"value":"November 5th, 2026"})).await?;
    take(s, d, n, "draft", p, st, cap).await?;
    let p = probe("focus-day", "quiet", json!({"date":"2026-10-25"})).await?;
    if p["effect"]["selected"] != "2026-11-05"
        || p["effect"]["popup"]["proof"]["month"] != "2026-10"
        || p["effect"]["active"]["date"] != "2026-10-25"
        || p["intent"]["nodeWitness"]["active"] != true
        || p["intent"]["nodeWitness"]["connected"] != true
        || p["intent"]["nodeWitness"]["inPopup"] != true
        || p["intent"]["nodeWitness"]["generationBefore"]
            != p["intent"]["nodeWitness"]["generationAfter"]
        || p["effect"]["popup"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["selected"] == "true")
    {
        return Err("Ordinary selected update replaced focus/view/selected markings".into());
    }
    observed(d).await?;
    existing::helpers::counts(s, 0, 0, 0, 0).await?;
    parent(&s.peek(), "quiet", Some("2026-11-05"))?;
    take(s, d, n, "blur-selected", p, st, cap).await?;
    let p = probe("button", "quiet", json!({"action":"next-month"})).await?;
    if p["effect"]["popup"]["proof"]["month"] != "2026-11"
        || p["effect"]["popup"]["proof"]["active"] != "2026-11-05"
    {
        return Err("Ordinary navigation ignored current selected Date".into());
    }
    take(s, d, n, "next-month", p, st, cap).await?;
    let p = probe(
        "button",
        "quiet",
        json!({"action":"day","date":"2026-11-06"}),
    )
    .await?;
    observed(d).await?;
    existing::helpers::counts(s, 0, 0, 0, 0).await?;
    parent(&s.peek(), "quiet", Some("2026-11-06"))?;
    take(s, d, n, "day-selected", p, st, cap).await?;
    let p = probe("keyboard-open", "quiet", json!({})).await?;
    take(s, d, n, "reopened", p, st, cap).await?;
    let p = probe("key", "quiet", json!({"key":"Escape"})).await?;
    if p["effect"]["popup"]["open"] != false
        || p["effect"]["active"]["id"] != crate::view::app_modal::field_id(&existing::key("quiet"))
    {
        return Err("Ordinary props popup did not dismiss to opener".into());
    }
    take(s, d, n, "escape", p, st, cap).await?;
    Ok(())
}
