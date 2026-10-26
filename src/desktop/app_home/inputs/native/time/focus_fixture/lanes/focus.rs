use super::*;

pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let mount = mount_flag(s, d, "pick").await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    if focused["target"]["expanded"] != true {
        return Err("True Time mount did not open its options".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "mount-open",
        "pick",
        json!({"mount":mount,"focused":focused,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let away = probe("away", "pick", json!({})).await?;
    observed(s, d).await?;
    let before = effect("pick").await?;
    let source = flags(&mut s.write(), Some("pick"));
    observed(s, d).await?;
    let after = effect("pick").await?;
    if before["active"] != after["active"]
        || after["target"]["active"] == true
        || after["target"]["expanded"] != false
    {
        return Err(format!(
            "Unchanged true Time focus/source mismatch: {}",
            json!({"before":before,"after":after,"source":source,"model":snapshot(&s.peek())})
        ));
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "away-unchanged",
        "pick",
        json!({"away":away,"before":before,"source":source,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let off = flags(&mut s.write(), None);
    observed(s, d).await?;
    let on = flags(&mut s.write(), Some("pick"));
    observed(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    if focused["target"]["expanded"] != false {
        return Err("Time flag update incorrectly opened its options".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "toggle-focus-only",
        "pick",
        json!({"off":off,"on":on,"focused":focused,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let range = probe("range", "pick", json!({})).await?;
    let before = effect("pick").await?;
    let off = flags(&mut s.write(), None);
    observed(s, d).await?;
    let on = flags(&mut s.write(), Some("pick"));
    observed(s, d).await?;
    let after = effect("pick").await?;
    if after["target"]["active"] != true
        || after["target"]["selection"] != before["target"]["selection"]
        || after["target"]["focus"]["attempts"] != before["target"]["focus"]["attempts"]
        || after["target"]["expanded"] != false
    {
        return Err("Already-focused Time toggle repeated focus/selection/open".into());
    }
    shot(
        s,
        d,
        n,
        "already-focused-toggle",
        "pick",
        json!({"range":range,"before":before,"off":off,"on":on,"after":after}),
        st,
        cap,
    )
    .await?;
    let off = flags(&mut s.write(), None);
    observed(s, d).await?;
    let after = effect("pick").await?;
    if after["target"]["active"] != true || after["target"]["expanded"] != false {
        return Err("False Time flag blurred or opened the input".into());
    }
    shot(
        s,
        d,
        n,
        "flag-off-focused",
        "pick",
        json!({"off":off,"after":after}),
        st,
        cap,
    )
    .await?;

    let input = probe("input", "pick", json!({"value":"17:45"})).await?;
    observed(s, d).await?;
    let range = probe("range", "pick", json!({})).await?;
    let before = effect("pick").await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["initial_time"] = json!("10:15")
    });
    observed(s, d).await?;
    let after = effect("pick").await?;
    selected_all(&after)?;
    if after["target"]["value"] != "17:45"
        || after["target"]["selected"] != "10:15"
        || after["target"]["node"] != before["target"]["node"]
        || after["target"]["expanded"] != true
        || after["target"]["rawDirty"] != true
    {
        return Err("Focused Time source update lost raw buffer or option state".into());
    }
    let silence = quiet(s).await?;
    shot(s,d,n,"focused-source-draft","pick",json!({"input":input,"range":range,"before":before,"source":source,"after":after,"silence":silence}),st,cap).await?;

    let blur = probe("blur", "pick", json!({})).await?;
    counts(s, 1, 1).await?;
    settle(s, d).await?;
    let after = effect("pick").await?;
    if after["target"]["active"] != false
        || after["target"]["expanded"] != false
        || snapshot(&s.peek())["formState"]["values"]["pick"]["value"]["selected_time"] != "17:45"
    {
        return Err("Explicit Time blur did not commit the exact draft".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "explicit-blur",
        "pick",
        json!({"blur":blur,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let mount = mount_flag(s, d, "required").await?;
    observed(s, d).await?;
    let after = active(d, "required", true).await?;
    if after["target"]["value"] != ""
        || after["target"]["expanded"] != true
        || !after["target"]["selected"].is_null()
    {
        return Err("Empty Time true mount did not focus and open without a value".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "empty-mount",
        "required",
        json!({"mount":mount,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await
}
