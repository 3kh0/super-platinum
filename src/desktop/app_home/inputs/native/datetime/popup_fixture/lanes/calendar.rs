use super::*;
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let parent = snapshot(&s.peek())["formState"].clone();
    let p = probe("open", "pick", json!({})).await?;
    month(&p, "2026-10")?;
    clear(&p, true)?;
    take(s, d, n, "opened", "pick", p, st, cap).await?;
    let p = probe("button", "pick", json!({"action":"next-month"})).await?;
    month(&p, "2026-11")?;
    take(s, d, n, "next-month", "pick", p, st, cap).await?;
    let p = probe("button", "pick", json!({"action":"previous-month"})).await?;
    month(&p, "2026-10")?;
    take(s, d, n, "previous-month", "pick", p, st, cap).await?;
    let p = probe(
        "button",
        "pick",
        json!({"action":"day","date":"2026-10-25"}),
    )
    .await?;
    visible(&p, false)?;
    counts(s, 0, 0, 0, 0).await?;
    unchanged_parent(&s.peek(), &parent)?;
    take(s, d, n, "same-day", "pick", p, st, cap).await?;
    let p = probe("keyboard-open", "pick", json!({})).await?;
    month(&p, "2026-10")?;
    take(s, d, n, "keyboard-open", "pick", p, st, cap).await?;
    let p = probe(
        "button",
        "pick",
        json!({"action":"day","date":"2026-10-28"}),
    )
    .await?;
    visible(&p, false)?;
    counts(s, 1, 1, 0, 0).await?;
    take(s, d, n, "day-selected", "pick", p, st, cap).await?;
    let p = probe("keyboard-open", "pick", json!({})).await?;
    clear(&p, true)?;
    take(s, d, n, "clear-open", "pick", p, st, cap).await?;
    let p = probe("button", "pick", json!({"action":"clear"})).await?;
    visible(&p, false)?;
    counts(s, 2, 2, 0, 0).await?;
    take(s, d, n, "optional-cleared", "pick", p, st, cap).await?;
    let p = probe("open", "pick", json!({})).await?;
    clear(&p, false)?;
    take(s, d, n, "empty-open", "pick", p, st, cap).await?;
    let active =
        probe("read", "pick", json!({})).await?["effect"]["popup"]["proof"]["active"].clone();
    probe("focus-day", "pick", json!({"date":active})).await?;
    let p = probe("key", "pick", json!({"key":"Escape"})).await?;
    escape(&p)?;
    counts(s, 3, 3, 0, 0).await?;
    take(s, d, n, "empty-dismissed", "pick", p, st, cap).await?;
    let p = probe("open", "required", json!({})).await?;
    month(&p, "2026-10")?;
    clear(&p, false)?;
    take(s, d, n, "required-open", "required", p, st, cap).await?;
    probe("focus-day", "required", json!({"date":"2026-10-25"})).await?;
    let p = probe("key", "required", json!({"key":"Escape"})).await?;
    escape(&p)?;
    take(s, d, n, "required-escape", "required", p, st, cap).await?;
    probe("focus-time", "required", json!({})).await?;
    let p = probe("time-input", "required", json!({"value":"not-a-time"})).await?;
    take(s, d, n, "time-draft", "required", p, st, cap).await?;
    let p = probe("keyboard-open", "required", json!({})).await?;
    month(&p, "2026-10")?;
    clear(&p, false)?;
    if p["effect"]["time"]["raw"] != "not-a-time" || p["effect"]["epoch"] != 1792946262i64 {
        return Err("Required Time blur lost invalid draft or changed epoch".into());
    }
    counts(s, 3, 3, 0, 0).await?;
    take(s, d, n, "required-draft-open", "required", p, st, cap).await?;
    let p = probe(
        "button",
        "required",
        json!({"action":"day","date":"2026-10-28"}),
    )
    .await?;
    visible(&p, false)?;
    if p["effect"]["time"]["raw"] != "not-a-time" || p["effect"]["time"]["rawDirty"] != true {
        return Err("Date choice replaced required Time draft".into());
    }
    counts(s, 4, 4, 0, 0).await?;
    take(s, d, n, "required-selected", "required", p, st, cap).await?;
    probe("focus-time", "required", json!({})).await?;
    let input = probe("time-input", "required", json!({"value":"13:15"})).await?;
    let enter = probe("time-enter", "required", json!({})).await?;
    counts(s, 5, 5, 0, 0).await?;
    take(
        s,
        d,
        n,
        "required-time-recovered",
        "required",
        json!({"input":input,"enter":enter}),
        st,
        cap,
    )
    .await?;
    let p = probe("toggle", "pick", json!({})).await?;
    visible(&p, true)?;
    if p["effect"]["active"]["tag"] != "SECTION" {
        return Err("Composite trigger did not focus calendar content".into());
    }
    take(s, d, n, "trigger-open", "pick", p, st, cap).await?;
    let p = probe("key", "pick", json!({"key":"Escape"})).await?;
    escape(&p)?;
    if p["effect"]["active"]["action"] != "open" {
        return Err("Composite Escape did not return trigger focus".into());
    }
    take(s, d, n, "trigger-escape", "pick", p, st, cap).await?;
    counts(s, 5, 5, 0, 0).await?;
    Ok(())
}
