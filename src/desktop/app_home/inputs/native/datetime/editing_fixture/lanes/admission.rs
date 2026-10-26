use super::*;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::text::fixture::admission as observer;
// Ordinary Buttons retain a finite processing window after an attempted click.
// Observe actual enabled DOM before the next explicit, still hit-tested click.
async fn press_ready(d: &DesktopContext) -> Result<Value, String> {
    capture::wait(d, "[...document.querySelectorAll('#app-home button')].some(b=>b.textContent.trim()==='Send form'&&!b.disabled)").await?;
    probe("press", "pick", json!({})).await
}

async fn wait(
    s: Signal<ShellState>,
    requests: usize,
    responses: usize,
    refusals: usize,
) -> Result<(), String> {
    for _ in 0..300 {
        let x = snapshot(&s.peek());
        let b = &x["buttonSync"];
        if b["requests"].as_array().map(Vec::len) == Some(requests)
            && b["responses"].as_array().map(Vec::len) == Some(responses)
            && b["refusals"].as_array().map(Vec::len) == Some(refusals)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!(
        "Datetime admission ledger did not settle: {}",
        snapshot(&s.peek())
    ))
}
async fn barrier(s: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..180 {
        let x = snapshot(&s.peek());
        if x["admission"]["waiting"] == true {
            return Ok(x);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Datetime admission observer did not reach actual calendar boundary".into())
}
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for (i, child, value) in [
        (0usize, "date", "5. November 2026"),
        (1, "picker", "2026-11-06"),
        (2, "time", "17:45"),
    ] {
        let before = snapshot(&s.peek());
        let unseen = probe(
            "unseen",
            "pick",
            json!({"child":child,"value":value,"focus":false}),
        )
        .await?;
        let press = press_ready(d).await?;
        wait(s, 0, 0, i + 1).await?;
        let raw = unseen["before"]["target"]["value"]
            .as_str()
            .ok_or("Unseen original literal missing")?;
        let restore = probe(
            "unseen",
            "pick",
            json!({"child":child,"value":raw,"focus":false}),
        )
        .await?;
        observed(s, d).await?;
        if snapshot(&s.peek())["formState"] != before["formState"]
            || snapshot(&s.peek())["nativeMounts"] != before["nativeMounts"]
        {
            return Err("Unseen Date child mutation advanced received custody".into());
        }
        shot(s,d,n,&format!("unseen-{child}-refused"),"pick",json!({"before":before,"unseen":unseen,"press":press,"restore":restore,"eventAuthorityInjected":false}),st,cap).await?;
    }
    let focus = act(s, d, "focus", "pick", json!({})).await?;
    let input = act(
        s,
        d,
        "input",
        "pick",
        json!({"value":"invalid-uncommitted"}),
    )
    .await?;
    if snapshot(&s.peek())["ready"] != false {
        return Err("Unresolved invalid Date draft gained admission".into());
    }
    let disabled = probe("disabled-press", "pick", json!({})).await?;
    wait(s, 0, 0, 3).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "invalid-date-refused",
        "pick",
        json!({"focus":focus,"input":input,"disabled":disabled,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let escape = act(s, d, "escape", "pick", json!({})).await?;
    let away = act(s, d, "away", "pick", json!({})).await?;
    for (i, child, value) in [
        (0usize, "date", "5. November 2026"),
        (1, "picker", "2026-11-06"),
        (2, "time", "17:45"),
    ] {
        let before = snapshot(&s.peek());
        let index = observer::arm(&mut s.write());
        let press = press_ready(d).await?;
        let attempted = barrier(s).await?;
        let edit = act(
            s,
            d,
            "input",
            "pick",
            json!({"child":child,"value":value,"focus":false}),
        )
        .await?;
        let acknowledged = snapshot(&s.peek());
        if acknowledged["formState"] != before["formState"] {
            return Err("Observed Datetime progress committed parent".into());
        }
        observer::release(&mut s.write())?;
        wait(s, i + 1, i, 3).await?;
        buttons::release(&mut s.write(), true);
        wait(s, i + 1, i + 1, 3).await?;
        let delivered = snapshot(&s.peek());
        if delivered["formState"] != before["formState"]
            || delivered["nativeMounts"] != acknowledged["nativeMounts"]
        {
            return Err("Trusted Datetime retry replaced exact triple/parent".into());
        }
        shot(s,d,n,&format!("trusted-{child}-progress"),"pick",json!({"index":index,"before":before,"press":press,"attempted":attempted,"edit":edit,"acknowledged":acknowledged,"delivered":delivered,"escape":escape,"away":away,"noRequestReplay":true}),st,cap).await?;
    }
    let index = observer::arm(&mut s.write());
    let press = press_ready(d).await?;
    let attempted = barrier(s).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][3]["label"]["text"] = json!("Current release notes");
    });
    observer::release(&mut s.write())?;
    wait(s, 3, 3, 4).await?;
    observed(s, d).await?;
    shot(s,d,n,"source-retired","pick",json!({"index":index,"press":press,"attempted":attempted,"source":source,"allThreeAttestationsRetired":true}),st,cap).await?;
    let press = press_ready(d).await?;
    wait(s, 4, 3, 4).await?;
    buttons::release(&mut s.write(), true);
    wait(s, 4, 4, 4).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "fresh-explicit-button",
        "pick",
        json!({"press":press,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let visit = {
        let state = s.peek();
        let h = state.app_home.surface.as_ref().unwrap();
        actions::Key {
            visit: h.id,
            revision: h.revision,
        }
    };
    let before = effect("pick").await?;
    let lease = inputs::native::lock_owned(s, visit)
        .await
        .ok_or("Exact current Datetime lease could not be acquired")?;
    let attempts = probe("lease-attempt", "pick", json!({})).await?;
    let held = effect("pick").await?;
    if held["target"]["snapshot"] != before["target"]["snapshot"]
        || held["target"]["acked"] != before["target"]["acked"]
        || held["target"]["lease"] != true
    {
        return Err("Held Datetime lease changed acknowledged triple".into());
    }
    shot(s,d,n,"lease-held","pick",json!({"before":before,"attempts":attempts,"held":held,"adapter":"Direct production native::lock_owned; actual Button admission verified in preceding stages"}),st,cap).await?;
    lease.release();
    capture::wait(
        d,
        "[...(window.__spHomeCalendarNative?.entries.values()||[])].every(e=>!e.lease)",
    )
    .await?;
    observed(s, d).await?;
    let after = effect("pick").await?;
    if after["active"] != before["active"]
        || after["target"]["snapshot"] != before["target"]["snapshot"]
        || after["target"]["date"]["readOnly"] != false
        || after["target"]["picker"]["readOnly"] != false
        || after["target"]["time"]["readOnly"] != false
    {
        return Err("Datetime release changed focus/buffer or retained readonly child".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "lease-released",
        "pick",
        json!({"before":before,"after":after,"silence":quiet}),
        st,
        cap,
    )
    .await
}
