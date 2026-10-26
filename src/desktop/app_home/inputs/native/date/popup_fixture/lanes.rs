use super::*;

fn open(p: &Value, expected: bool) -> Result<(), String> {
    if p["effect"]["popup"]["open"] != expected {
        return Err(format!("Date popup visibility mismatch: {p}"));
    }
    Ok(())
}
fn month(p: &Value, expected: &str) -> Result<(), String> {
    open(p, true)?;
    let buttons = p["effect"]["popup"]["buttons"]
        .as_array()
        .ok_or("Popup buttons missing")?;
    let days = buttons
        .iter()
        .filter_map(|b| b["date"].as_str())
        .collect::<Vec<_>>();
    if days.is_empty() || !days.iter().all(|day| day.starts_with(expected)) {
        return Err(format!("Unexpected popup month {expected}: {days:?}"));
    }
    Ok(())
}
fn clear(p: &Value, expected: bool) -> Result<(), String> {
    let present = p["effect"]["popup"]["buttons"]
        .as_array()
        .is_some_and(|a| a.iter().any(|b| b["action"] == "clear"));
    if present != expected {
        return Err("Calendar clear affordance mismatch".into());
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
    observed(d).await?;
    shot(s, d, n, stage, block, json!({"intent":p}), st, cap).await
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = probe("open", "pick", json!({})).await?;
    month(&p, "2026-10")?;
    clear(&p, true)?;
    take(s, d, n, "opened", "pick", p, st, cap).await?;
    if case() == "guards" {
        return guards(s, d, n, st, cap).await;
    }
    for (action, stage, expected) in [
        ("next-month", "next-month", "2026-11"),
        ("next-year", "next-year", "2027-11"),
        ("previous-year", "previous-year", "2026-11"),
        ("previous-month", "previous-month", "2026-10"),
    ] {
        let p = probe("button", "pick", json!({"action":action})).await?;
        month(&p, expected)?;
        parent(&s.peek(), "pick", Some("2026-10-26"))?;
        take(s, d, n, stage, "pick", p, st, cap).await?;
    }
    let p = probe(
        "button",
        "pick",
        json!({"action":"day","date":"2026-10-28"}),
    )
    .await?;
    open(&p, false)?;
    existing::helpers::counts(s, 1, 1, 0, 0).await?;
    settle(s, d).await?;
    parent(&s.peek(), "pick", Some("2026-10-28"))?;
    take(s, d, n, "day-selected", "pick", p, st, cap).await?;
    let p = probe("open", "pick", json!({})).await?;
    month(&p, "2026-10")?;
    take(s, d, n, "reopened", "pick", p, st, cap).await?;
    let p = probe("button", "pick", json!({"action":"clear"})).await?;
    open(&p, false)?;
    existing::helpers::counts(s, 2, 2, 0, 0).await?;
    settle(s, d).await?;
    parent(&s.peek(), "pick", None)?;
    take(s, d, n, "optional-cleared", "pick", p, st, cap).await?;
    let p = probe("open", "required", json!({})).await?;
    month(&p, "2026-10")?;
    clear(&p, false)?;
    take(s, d, n, "required-open", "required", p, st, cap).await?;
    let focus = probe("focus-day", "required", json!({"date":"2026-10-27"})).await?;
    let p = probe("key", "required", json!({"key":"ArrowDown"})).await?;
    month(&p, "2026-11")?;
    take(
        s,
        d,
        n,
        "keyboard-week",
        "required",
        json!({"focus":focus,"key":p}),
        st,
        cap,
    )
    .await?;
    let p = probe("key", "required", json!({"key":"PageUp"})).await?;
    month(&p, "2026-10")?;
    take(s, d, n, "keyboard-month", "required", p, st, cap).await?;
    let p = probe(
        "button",
        "required",
        json!({"action":"day","date":"2026-10-29"}),
    )
    .await?;
    open(&p, false)?;
    existing::helpers::counts(s, 3, 3, 0, 0).await?;
    settle(s, d).await?;
    parent(&s.peek(), "required", Some("2026-10-29"))?;
    take(s, d, n, "required-selected", "required", p, st, cap).await?;
    probe("open", "required", json!({})).await?;
    let p = probe("outside", "required", json!({})).await?;
    open(&p, false)?;
    take(s, d, n, "outside-dismissed", "required", p, st, cap).await?;
    probe("open", "required", json!({})).await?;
    let p = probe("key", "required", json!({"key":"Escape","input":true})).await?;
    open(&p, false)?;
    take(s, d, n, "input-escape", "required", p, st, cap).await?;
    let p = probe("toggle", "required", json!({})).await?;
    open(&p, true)?;
    take(s, d, n, "trigger-open", "required", p, st, cap).await?;
    let p = probe("toggle", "required", json!({})).await?;
    open(&p, false)?;
    take(s, d, n, "trigger-closed", "required", p, st, cap).await?;
    parent(&s.peek(), "required", Some("2026-10-29"))?;
    Ok(())
}
async fn guards(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = probe("input", "pick", json!({"value":"November 5th, 2026"})).await?;
    take(s, d, n, "draft-open", "pick", p, st, cap).await?;
    probe("remember", "pick", json!({})).await?;
    let source = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01")
    });
    settle(s, d).await?;
    let p = probe("read", "pick", json!({})).await?;
    open(&p, false)?;
    if p["effect"]["raw"] != "November 5th, 2026" {
        return Err("Source refresh replaced Date popup draft".into());
    }
    take(
        s,
        d,
        n,
        "source-retired",
        "pick",
        json!({"source":source,"read":p}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let p = probe("stale-attempt", "pick", json!({})).await?;
    observed(d).await?;
    if snapshot(&s.peek()) != before {
        return Err("Retired popup action changed model".into());
    }
    take(s, d, n, "stale-refused", "pick", p, st, cap).await?;
    let p = probe("open", "pick", json!({})).await?;
    month(&p, "2026-12")?;
    probe("remember", "pick", json!({})).await?;
    observed(d).await?;
    take(s, d, n, "lease-open", "pick", p, st, cap).await?;
    let visit = {
        let state = s.peek();
        let h = state.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    let before = probe("read", "pick", json!({})).await?;
    let lease = inputs::native::lock_owned(s, visit)
        .await
        .ok_or("Popup native lease was refused")?;
    let p = probe("lease-attempt", "pick", json!({})).await?;
    open(&p, false)?;
    if p["effect"]["snapshot"] != before["effect"]["snapshot"] {
        return Err("Lease replaced Date pair".into());
    }
    take(s, d, n, "lease-held", "pick", p, st, cap).await?;
    lease.release();
    capture::wait(
        d,
        "[...(window.__spHomeCalendarNative?.entries.values()||[])].every(e=>!e.lease)",
    )
    .await?;
    let p = probe("read", "pick", json!({})).await?;
    open(&p, false)?;
    take(s, d, n, "lease-released", "pick", p, st, cap).await?;
    let ctx = s
        .peek()
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .owner
        .ctx
        .clone();
    let key = (ctx.epoch, ctx.session.team_id.clone());
    s.write()
        .app_home
        .cooldowns
        .insert(key.clone(), Some(Instant::now() + Duration::from_secs(60)));
    text::observe_cooling(&mut s.write());
    capture::wait(d,"[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>e.editableDate&&e.host?.isConnected).every(e=>e.node.disabled)").await?;
    take(
        s,
        d,
        n,
        "disabled",
        "pick",
        json!({"offlineCooldown":true}),
        st,
        cap,
    )
    .await?;
    s.write().app_home.cooldowns.remove(&key);
    text::observe_cooling(&mut s.write());
    settle(s, d).await?;
    let p = probe("open", "pick", json!({})).await?;
    month(&p, "2026-12")?;
    take(s, d, n, "recovered", "pick", p, st, cap).await?;
    parent(&s.peek(), "pick", Some("2026-10-26"))?;
    if snapshot(&s.peek())["events"] != json!([]) {
        return Err("Popup guard lane emitted an action".into());
    }
    Ok(())
}
