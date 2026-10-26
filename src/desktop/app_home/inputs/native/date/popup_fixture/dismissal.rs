//! Calendar Escape propagation and actual opener focus, using DOM intents only.
use super::*;

fn closed(p: &Value, focus: &str) -> Result<(), String> {
    if p["effect"]["popup"]["open"] != false {
        return Err(format!("Calendar did not close: {p}"));
    }
    let active = &p["effect"]["active"];
    let id = crate::view::app_modal::field_id(&existing::key("pick"));
    let valid = match focus {
        "input" => active["id"] == id,
        "trigger" => active["action"] == "open",
        "neither" => active["id"] != id && active["action"] != "open",
        _ => false,
    };
    if !valid {
        return Err(format!("Calendar close focus differs ({focus}): {p}"));
    }
    Ok(())
}
fn opened(p: &Value, focus: &str) -> Result<(), String> {
    if p["effect"]["popup"]["open"] != true {
        return Err(format!("Calendar did not open: {p}"));
    }
    let active = &p["effect"]["active"];
    let valid = match focus {
        "input" => active["tag"] == "INPUT",
        "day" => active["action"] == "day",
        "content" => active["tag"] == "SECTION",
        _ => false,
    };
    if !valid {
        return Err(format!("Calendar opening focus differs ({focus}): {p}"));
    }
    Ok(())
}
fn escape(p: &Value, prevented: bool) -> Result<(), String> {
    let i = &p["intent"];
    let q = &i["propagation"];
    if i["keyCode"] != 27
        || i["prevented"] != prevented
        || q["documentCapture"] != 1
        || q["popup"] != 1
        || q["documentBubble"] != 0
        || q["windowBubble"] != 0
        || q["stoppedAtPopup"] != true
        || q["localMarker"] != true
    {
        return Err(format!("Calendar Escape propagation differs: {p}"));
    }
    Ok(())
}
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
    shot(s, d, n, stage, "pick", json!({"intent":p}), st, cap).await
}
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = probe("open", "pick", json!({})).await?;
    opened(&p, "input")?;
    take(s, d, n, "pointer-open", p, st, cap).await?;
    probe("focus-day", "pick", json!({"date":"2026-10-26"})).await?;
    let p = probe("key", "pick", json!({"key":"Escape"})).await?;
    escape(&p, false)?;
    closed(&p, "neither")?;
    take(s, d, n, "pointer-day-escape", p, st, cap).await?;

    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "keyboard-open", p, st, cap).await?;
    probe("focus-control", "pick", json!({"action":"next-month"})).await?;
    let p = probe("key", "pick", json!({"key":"Escape","prevented":true})).await?;
    escape(&p, true)?;
    closed(&p, "input")?;
    take(s, d, n, "keyboard-nav-escape", p, st, cap).await?;

    let p = probe("toggle", "pick", json!({})).await?;
    opened(&p, "content")?;
    take(s, d, n, "trigger-open", p, st, cap).await?;
    let p = probe("key", "pick", json!({"key":"Escape"})).await?;
    escape(&p, false)?;
    closed(&p, "trigger")?;
    take(s, d, n, "trigger-escape", p, st, cap).await?;

    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "select-open", p, st, cap).await?;
    let p = probe(
        "button",
        "pick",
        json!({"action":"day","date":"2026-10-28"}),
    )
    .await?;
    closed(&p, "input")?;
    existing::helpers::counts(s, 1, 1, 0, 0).await?;
    settle(s, d).await?;
    parent(&s.peek(), "pick", Some("2026-10-28"))?;
    take(s, d, n, "selection-restored", p, st, cap).await?;
    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "clear-open", p, st, cap).await?;
    let p = probe("button", "pick", json!({"action":"clear"})).await?;
    closed(&p, "input")?;
    existing::helpers::counts(s, 2, 2, 0, 0).await?;
    settle(s, d).await?;
    parent(&s.peek(), "pick", None)?;
    take(s, d, n, "clear-restored", p, st, cap).await?;

    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "outside-open", p, st, cap).await?;
    let p = probe("outside", "pick", json!({})).await?;
    closed(&p, "input")?;
    take(s, d, n, "outside-restored", p, st, cap).await?;

    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "lease-open", p, st, cap).await?;
    let key = {
        let state = s.peek();
        let h = state.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    let before = probe("read", "pick", json!({})).await?;
    let lease = inputs::native::lock_owned(s, key)
        .await
        .ok_or("Dismissal lease refused")?;
    let p = probe("read", "pick", json!({})).await?;
    closed(&p, "neither")?;
    if p["effect"]["snapshot"] != before["effect"]["snapshot"]
        || p["effect"]["seq"] != before["effect"]["seq"]
    {
        return Err("Silent lease close changed Date buffer or watermark".into());
    }
    take(s, d, n, "lease-held", p, st, cap).await?;
    lease.release();
    capture::wait(
        d,
        "[...(window.__spHomeCalendarNative?.entries.values()||[])].every(e=>!e.lease)",
    )
    .await?;
    let p = probe("read", "pick", json!({})).await?;
    closed(&p, "neither")?;
    take(s, d, n, "lease-released", p, st, cap).await?;

    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    probe("remember", "pick", json!({})).await?;
    take(s, d, n, "source-open", p, st, cap).await?;
    let source = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01");
    });
    settle(s, d).await?;
    let p = probe("read", "pick", json!({})).await?;
    closed(&p, "neither")?;
    take(
        s,
        d,
        n,
        "source-retired",
        json!({"source":source,"read":p}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let p = probe("stale-escape", "pick", json!({})).await?;
    closed(&p, "neither")?;
    observed(d).await?;
    if snapshot(&s.peek()) != before {
        return Err("Retired calendar Escape changed model".into());
    }
    take(s, d, n, "stale-escape-refused", p, st, cap).await?;
    let p = probe("keyboard-open", "pick", json!({})).await?;
    opened(&p, "day")?;
    take(s, d, n, "recovered", p, st, cap).await?;
    let p = probe("key", "pick", json!({"key":"Escape"})).await?;
    escape(&p, false)?;
    closed(&p, "input")?;
    take(s, d, n, "recovered-escape", p, st, cap).await?;
    existing::helpers::counts(s, 2, 2, 0, 0).await?;
    parent(&s.peek(), "pick", None)?;
    Ok(())
}
