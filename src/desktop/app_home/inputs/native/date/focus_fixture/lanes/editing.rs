use super::*;

fn date(s: &ShellState, block: &str) -> Value {
    snapshot(s)["dateWidgets"]
        .as_array()
        .and_then(|a| a.iter().find(|w| w["block"] == block))
        .cloned()
        .unwrap_or(Value::Null)
}
fn state(
    s: &ShellState,
    block: &str,
    raw: &str,
    selected: Option<&str>,
    parent: Option<&str>,
) -> Result<(), String> {
    let model = snapshot(s);
    let w = date(s, block);
    if w["raw"] != raw
        || w["visible"] != json!(selected)
        || model["formState"]["values"][block]["value"]["selected_date"] != json!(parent)
    {
        return Err(format!("Date editing raw/local/parent mismatch: {w}"));
    }
    Ok(())
}
fn resolved(s: &ShellState, block: &str, selected: Option<&str>) -> Result<(), String> {
    let w = date(s, block);
    if w["resolvedRaw"] != "not a date"
        || w["resolvedSelected"] != json!(selected)
        || w["bufferValid"] != true
    {
        return Err(format!(
            "Explicit Date commit did not resolve its exact raw/selected pair: {w}"
        ));
    }
    Ok(())
}
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let change = flags(&mut s.write(), Some("pick"));
    active(d, "pick", true).await?;
    observed(s, d).await?;
    let focused = effect("pick").await?;
    selected_all(&focused)?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "auto-focus",
        "pick",
        json!({"flags":change,"focus":focused,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let before = snapshot(&s.peek());
    let input = probe("input", "pick", json!({"value":"November 2nd, 2026"})).await?;
    observed(s, d).await?;
    state(
        &s.peek(),
        "pick",
        "November 2nd, 2026",
        Some("2026-10-26"),
        Some("2026-10-26"),
    )?;
    if snapshot(&s.peek())["events"] != before["events"] {
        return Err("Date draft input emitted a callback".into());
    }
    let range = probe("range", "pick", json!({})).await?;
    shot(
        s,
        d,
        n,
        "draft-local",
        "pick",
        json!({"input":input,"range":range,"before":before}),
        st,
        cap,
    )
    .await?;

    let before = effect("pick").await?;
    let change = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01");
    });
    observed(s, d).await?;
    let after = active(d, "pick", true).await?;
    state(
        &s.peek(),
        "pick",
        "November 2nd, 2026",
        Some("2026-12-01"),
        Some("2026-10-26"),
    )?;
    if before["target"]["selection"] != after["target"]["selection"] {
        return Err("Focused Date source refresh replaced the draft selection".into());
    }
    shot(
        s,
        d,
        n,
        "focused-source-change",
        "pick",
        json!({"refresh":change,"before":before,"after":after}),
        st,
        cap,
    )
    .await?;

    let escape = probe("escape", "pick", json!({})).await?;
    observed(s, d).await?;
    selected_all(&effect("pick").await?)?;
    state(
        &s.peek(),
        "pick",
        "December 1st, 2026",
        Some("2026-12-01"),
        Some("2026-10-26"),
    )?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "escape-current-initial",
        "pick",
        json!({"escape":escape,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let input = probe("input", "pick", json!({"value":"November 2nd, 2026"})).await?;
    observed(s, d).await?;
    let enter = probe("enter", "pick", json!({})).await?;
    existing::helpers::counts(s, 1, 1, 0, 0).await?;
    settle(s, d).await?;
    state(
        &s.peek(),
        "pick",
        "November 2nd, 2026",
        Some("2026-11-02"),
        Some("2026-11-02"),
    )?;
    shot(
        s,
        d,
        n,
        "valid-enter",
        "pick",
        json!({"input":input,"enter":enter}),
        st,
        cap,
    )
    .await?;

    let before = snapshot(&s.peek());
    let input = probe("input", "pick", json!({"value":"not a date"})).await?;
    observed(s, d).await?;
    state(
        &s.peek(),
        "pick",
        "not a date",
        Some("2026-11-02"),
        Some("2026-11-02"),
    )?;
    if inputs::ready(&s.peek())
        || snapshot(&s.peek())["events"] != before["events"]
        || date(&s.peek(), "pick")["bufferValid"] != false
        || !date(&s.peek(), "pick")["resolvedRaw"].is_null()
    {
        return Err("Uncommitted invalid Date did not block readiness without callback".into());
    }
    shot(
        s,
        d,
        n,
        "optional-invalid-draft",
        "pick",
        json!({"input":input,"before":before}),
        st,
        cap,
    )
    .await?;
    let enter = probe("enter", "pick", json!({})).await?;
    existing::helpers::counts(s, 2, 2, 0, 0).await?;
    settle(s, d).await?;
    state(&s.peek(), "pick", "not a date", Some(""), None)?;
    resolved(&s.peek(), "pick", Some(""))?;
    shot(
        s,
        d,
        n,
        "optional-invalid-clear",
        "pick",
        json!({"enter":enter}),
        st,
        cap,
    )
    .await?;
    let escape = probe("escape", "pick", json!({})).await?;
    observed(s, d).await?;
    state(&s.peek(), "pick", "", Some(""), None)?;
    selected_all(&effect("pick").await?)?;
    shot(
        s,
        d,
        n,
        "optional-escape",
        "pick",
        json!({"escape":escape}),
        st,
        cap,
    )
    .await?;

    let focus = probe("focus", "required", json!({})).await?;
    observed(s, d).await?;
    let before = snapshot(&s.peek());
    let input = probe("input", "required", json!({"value":"not a date"})).await?;
    observed(s, d).await?;
    let enter = probe("enter", "required", json!({})).await?;
    observed(s, d).await?;
    state(&s.peek(), "required", "not a date", None, None)?;
    if date(&s.peek(), "required")["bufferValid"] != false
        || !date(&s.peek(), "required")["resolvedRaw"].is_null()
        || inputs::ready(&s.peek())
    {
        return Err("Required invalid Date with no fallback gained permission".into());
    }
    if snapshot(&s.peek())["events"] != before["events"] {
        return Err("Required Date without fallback emitted a callback".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "required-no-prior-invalid",
        "required",
        json!({"focus":focus,"input":input,"enter":enter,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let input = probe("input", "required", json!({"value":"11/03/2026"})).await?;
    observed(s, d).await?;
    let enter = probe("enter", "required", json!({})).await?;
    existing::helpers::counts(s, 3, 3, 0, 0).await?;
    settle(s, d).await?;
    state(
        &s.peek(),
        "required",
        "11/03/2026",
        Some("2026-11-03"),
        Some("2026-11-03"),
    )?;
    shot(
        s,
        d,
        n,
        "required-valid-enter",
        "required",
        json!({"input":input,"enter":enter}),
        st,
        cap,
    )
    .await?;

    let before = snapshot(&s.peek());
    let input = probe("input", "required", json!({"value":"not a date"})).await?;
    observed(s, d).await?;
    let enter = probe("enter", "required", json!({})).await?;
    settle(s, d).await?;
    state(
        &s.peek(),
        "required",
        "not a date",
        Some("2026-11-03"),
        Some("2026-11-03"),
    )?;
    resolved(&s.peek(), "required", Some("2026-11-03"))?;
    if snapshot(&s.peek())["events"] != before["events"] {
        return Err("Equal required fallback emitted a callback".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "required-invalid-fallback",
        "required",
        json!({"input":input,"enter":enter,"silence":silence}),
        st,
        cap,
    )
    .await?;
    let escape = probe("escape", "required", json!({})).await?;
    observed(s, d).await?;
    state(
        &s.peek(),
        "required",
        "November 3rd, 2026",
        Some("2026-11-03"),
        Some("2026-11-03"),
    )?;
    selected_all(&effect("required").await?)?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "required-escape",
        "required",
        json!({"escape":escape,"silence":silence}),
        st,
        cap,
    )
    .await
}
