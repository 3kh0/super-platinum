use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let p = probe("open", "quiet", json!({})).await?;
    month(&p, "2026-10")?;
    take(s, d, n, "opened", "quiet", p, st, cap).await?;
    let p = probe("input", "quiet", json!({"value":"November 5th, 2026"})).await?;
    take(s, d, n, "date-draft", "quiet", p, st, cap).await?;
    let p = probe("focus-day", "quiet", json!({"date":"2026-10-25"})).await?;
    month(&p, "2026-10")?;
    if p["effect"]["selected"] != "2026-11-05"
        || p["effect"]["active"]["date"] != "2026-10-25"
        || p["effect"]["popup"]["proof"]["generation"]
            != p["before"]["popup"]["proof"]["generation"]
        || p["effect"]["popup"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["selected"] == "true")
    {
        return Err("Live selected update reset calendar or lost focused day identity".into());
    }
    counts(s, 0, 0, 0, 0).await?;
    take(s, d, n, "blur-selected", "quiet", p, st, cap).await?;
    let p = probe("button", "quiet", json!({"action":"next-month"})).await?;
    month(&p, "2026-11")?;
    if p["effect"]["popup"]["proof"]["active"] != "2026-11-05" {
        return Err("Calendar navigation ignored updated selected Date".into());
    }
    take(s, d, n, "next-month", "quiet", p, st, cap).await?;
    let p = probe(
        "button",
        "quiet",
        json!({"action":"day","date":"2026-11-06"}),
    )
    .await?;
    visible(&p, false)?;
    counts(s, 0, 0, 0, 0).await?;
    take(s, d, n, "day-selected", "quiet", p, st, cap).await?;
    let p = probe("keyboard-open", "quiet", json!({})).await?;
    month(&p, "2026-11")?;
    take(s, d, n, "lease-open", "quiet", p, st, cap).await?;
    probe("remember", "quiet", json!({})).await?;
    let visit = {
        let v = s.peek();
        let h = v.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    let before = probe("read", "quiet", json!({})).await?;
    let lease = inputs::native::lock_owned(s, visit)
        .await
        .ok_or("Composite popup lease refused")?;
    let p = probe("lease-attempt", "quiet", json!({})).await?;
    visible(&p, false)?;
    if p["effect"]["snapshot"] != before["effect"]["snapshot"]
        || p["effect"]["seq"] != before["effect"]["seq"]
        || p["effect"]["time"]["readOnly"] != true
    {
        return Err("Lease changed acknowledged composite triple".into());
    }
    take(s, d, n, "lease-held", "quiet", p, st, cap).await?;
    lease.release();
    capture::wait(
        d,
        "[...(window.__spHomeCalendarNative?.entries.values()||[])].every(e=>!e.lease)",
    )
    .await?;
    let p = probe("read", "quiet", json!({})).await?;
    visible(&p, false)?;
    take(s, d, n, "lease-released", "quiet", p, st, cap).await?;
    let p = probe("keyboard-open", "quiet", json!({})).await?;
    probe("remember", "quiet", json!({})).await?;
    take(s, d, n, "source-open", "quiet", p, st, cap).await?;
    let parent = snapshot(&s.peek())["formState"].clone();
    let source = helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][2]["element"]["initial_date_time"] = json!(1793032662i64)
    });
    observed(s, d).await?;
    let p = probe("read", "quiet", json!({})).await?;
    visible(&p, false)?;
    unchanged_parent(&s.peek(), &parent)?;
    take(
        s,
        d,
        n,
        "source-retired",
        "quiet",
        json!({"source":source,"read":p}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let p = probe("stale-attempt", "quiet", json!({})).await?;
    observed(s, d).await?;
    if snapshot(&s.peek()) != before {
        return Err("Retired composite popup action mutated current model".into());
    }
    take(s, d, n, "stale-refused", "quiet", p, st, cap).await?;
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
    capture::wait(d,"[...(window.__spHomeCalendarNative?.entries.values()||[])].filter(e=>e.datetime&&e.host.isConnected).every(e=>e.date.disabled&&e.node.disabled)").await?;
    take(
        s,
        d,
        n,
        "disabled",
        "quiet",
        json!({"offlineCooldown":true}),
        st,
        cap,
    )
    .await?;
    s.write().app_home.cooldowns.remove(&key);
    text::observe_cooling(&mut s.write());
    observed(s, d).await?;
    let p = probe("keyboard-open", "quiet", json!({})).await?;
    month(&p, "2026-11")?;
    take(s, d, n, "recovered", "quiet", p, st, cap).await?;
    let p = probe("key", "quiet", json!({"key":"Escape"})).await?;
    escape(&p)?;
    take(s, d, n, "recovered-escape", "quiet", p, st, cap).await?;
    counts(s, 0, 0, 0, 0).await?;
    Ok(())
}
