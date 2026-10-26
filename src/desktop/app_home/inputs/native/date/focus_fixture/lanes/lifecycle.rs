use super::*;

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
    selected_all(&effect("pick").await?)?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "auto-focus",
        "pick",
        json!({"flags":change,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let focus = probe("focus", "quiet", json!({})).await?;
    observed(s, d).await?;
    let input = probe("input", "quiet", json!({"value":"November 2nd, 2026"})).await?;
    observed(s, d).await?;
    let before_removal = snapshot(&s.peek());
    let before_effect = effect("quiet").await?;
    let original = before_removal["rawView"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .position(|b| b["block_id"] == "quiet")
        .ok_or("Quiet Date source missing")?;
    let source_block = before_removal["rawView"]["blocks"][original].clone();
    if before_removal["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["dirty"] == true)
        || before_removal["formState"]["values"]["quiet"]["value"]["selected_date"] != "2026-10-27"
    {
        return Err("Date draft removal precondition was not pristine".into());
    }
    let removal = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["block_id"] != "quiet");
    });
    capture::wait(
        d,
        &format!("!document.getElementById({})", json!(id("quiet"))),
    )
    .await?;
    let refused = snapshot(&s.peek());
    let absent = effect("quiet").await?;
    for key in [
        "rawView",
        "formState",
        "fields",
        "events",
        "requests",
        "responses",
    ] {
        if refused[key] != before_removal[key] {
            return Err(format!("Uncommitted Date removal changed {key}"));
        }
    }
    if refused["ready"] != false || !absent["target"].is_null() {
        return Err("Uncommitted Date removal did not retain a blocked parent".into());
    }
    let restored_source = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"]
            .as_array_mut()
            .unwrap()
            .insert(original, source_block);
    });
    observed(s, d).await?;
    let restored = snapshot(&s.peek());
    let restored_effect = effect("quiet").await?;
    for key in ["formState", "fields", "events", "requests", "responses"] {
        if restored[key] != before_removal[key] {
            return Err(format!("Restored uncommitted Date changed {key}"));
        }
    }
    if restored_effect["target"]["value"] != "November 2nd, 2026"
        || restored_effect["target"]["active"] != true
        || restored["ready"] != true
    {
        return Err("Restored Date lost the acknowledged uncommitted literal".into());
    }
    shot(
        s,
        d,
        n,
        "draft-removal-refused",
        "quiet",
        json!({"focus":focus,"input":input,"before":before_removal,"beforeEffect":before_effect,
            "removal":removal,"refused":refused,"absent":absent,"restore":restored_source,
            "restored":restored,"restoredEffect":restored_effect}),
        st,
        cap,
    )
    .await?;
    let enter = probe("enter", "quiet", json!({})).await?;
    settle(s, d).await?;
    if snapshot(&s.peek())["formState"]["values"]["quiet"]["value"]["selected_date"] != "2026-11-02"
        || snapshot(&s.peek())["requests"] != json!([])
        || snapshot(&s.peek())["buttonSync"]["requests"] != json!([])
    {
        return Err("Quiet Date did not make the Reset parent dirty".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "quiet-dirty",
        "quiet",
        json!({"focus":focus,"input":input,"enter":enter,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let before = snapshot(&s.peek());
    let previous = widget(&s.peek(), "pick");
    let reset = probe("reset", "pick", json!({})).await?;
    let mut reset_seen = false;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            reset_seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !reset_seen {
        return Err("Actual Reset did not replace the form revision".into());
    }
    observed(s, d).await?;
    settle(s, d).await?;
    let after = effect("pick").await?;
    no_field_focus(&after)?;
    // Reset makes the form pristine and disables this same button. Chromium
    // may move activeElement to BODY when a focused button becomes disabled.
    // This is distinct from any Date control requesting focus after Reset.
    let reset_button = &after["resetButton"];
    let reset_focus = after["active"]["node"] == reset["intent"]["target"]["node"]
        || after["active"]["tag"] == "BODY";
    if widget(&s.peek(), "pick")["instance"] != previous["instance"]
        || reset["intent"]["targetDisabled"] != false
        || reset_button["node"] != reset["intent"]["target"]["node"]
        || reset_button["connected"] != true
        || reset_button["disabled"] != true
        || !reset_focus
        || snapshot(&s.peek())["formState"]["values"]["quiet"]["value"]["selected_date"]
            != "2026-10-27"
    {
        return Err(format!(
            "Date Reset identity/focus/parent mismatch: {}",
            json!({"previousWidget":previous,"widget":widget(&s.peek(),"pick"),"reset":reset,"after":after,"parent":snapshot(&s.peek())["formState"]["values"]["quiet"]})
        ));
    }
    let silence = quiet(s).await?;
    shot(s,d,n,"reset-no-refocus","pick",json!({"reset":reset,"before":before,"previousWidget":previous,"after":after,"silence":silence}),st,cap).await?;

    let previous = widget(&s.peek(), "pick");
    let returned = existing::navigation::revisit(s, d).await?;
    observed(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    if widget(&s.peek(), "pick")["instance"] == previous["instance"] {
        return Err("Fresh Home visit reused a Date focus instance".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "cached-revisit",
        "pick",
        json!({"return":returned,"previousWidget":previous,"focus":focused,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let previous = widget(&s.peek(), "pick");
    let original = snapshot(&s.peek())["rawView"]["blocks"]
        .as_array()
        .and_then(|a| a.iter().find(|b| b["block_id"] == "pick"))
        .cloned()
        .ok_or("Date source block missing before removal")?;
    let removed = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["block_id"] != "pick");
    });
    capture::wait(
        d,
        &format!("!document.getElementById({})", json!(id("pick"))),
    )
    .await?;
    observed(s, d).await?;
    let absent = effect("pick").await?;
    if !absent["target"].is_null() || !widget(&s.peek(), "pick").is_null() {
        return Err("Removed Date retained live DOM or focus identity".into());
    }
    let added = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"].as_array_mut().unwrap().insert(0, original);
    });
    observed(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    if widget(&s.peek(), "pick")["instance"] == previous["instance"] {
        return Err("Reintroduced Date reused the removed focus instance".into());
    }
    let silence = quiet(s).await?;
    shot(s,d,n,"remove-readd","pick",json!({"remove":removed,"absent":absent,"add":added,"previousWidget":previous,"focus":focused,"silence":silence}),st,cap).await?;

    let before = snapshot(&s.peek());
    let source = {
        let mut state = s.write();
        let source = flags(&mut state, Some("pick"));
        state.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        source
    };
    capture::wait(d,"!!document.querySelector('.app-home') && !document.querySelector('.app-home input') && [...document.querySelectorAll('.app-home button')].some(b=>b.textContent.trim()==='Load Home')").await?;
    let mut retired = false;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["authority"]["homeCurrent"] == false
            && x["nativeMounts"].is_null()
            && x["windows"] == json!([])
            && x["activity"] == json!([])
        {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !retired {
        return Err("Retired Date owner retained mounts or scheduling state".into());
    }
    let after = effect("home-empty").await?;
    no_field_focus(&after)?;
    let silence = quiet(s).await?;
    if snapshot(&s.peek())["requests"] != before["requests"] {
        return Err("Date owner retirement emitted an action".into());
    }
    shot(
        s,
        d,
        n,
        "owner-retired",
        "home-empty",
        json!({"source":source,"before":before,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await
}
