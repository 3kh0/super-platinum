use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = flags(&mut s.write(), Some("pick"));
    settle(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "auto-focus",
        "pick",
        json!({"source":source,"observed":focused,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let range = probe("range", "pick", json!({})).await?;
    let before = effect("pick").await?;
    let source = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]["placeholder"] =
            json!({"type":"plain_text","text":"Updated date hint"})
    });
    settle(s, d).await?;
    let after = active(d, "pick", true).await?;
    if before["target"]["value"] != after["target"]["value"]
        || before["target"]["selection"] != after["target"]["selection"]
    {
        return Err("Date unrelated props changed focused draft/range".into());
    }
    shot(
        s,
        d,
        n,
        "range-preserved",
        "pick",
        json!({"range":range,"source":source,"before":before,"after":after}),
        st,
        cap,
    )
    .await?;

    let away = probe("away", "pick", json!({})).await?;
    settle(s, d).await?;
    let before = effect("pick").await?;
    let source = existing::helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["label"]["text"] = json!("Updated release date")
    });
    settle(s, d).await?;
    let silence = quiet(s).await?;
    let after = effect("pick").await?;
    if before["active"] != after["active"] || after["target"]["active"] == true {
        return Err("Unchanged Date focus flag stole focus".into());
    }
    shot(
        s,
        d,
        n,
        "away-updated",
        "pick",
        json!({"away":away,"source":source,"before":before,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let off = flags(&mut s.write(), None);
    settle(s, d).await?;
    let on = flags(&mut s.write(), Some("pick"));
    settle(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    shot(
        s,
        d,
        n,
        "toggle-focus",
        "pick",
        json!({"off":off,"on":on,"observed":focused}),
        st,
        cap,
    )
    .await?;
    let off = flags(&mut s.write(), None);
    settle(s, d).await?;
    let focused = active(d, "pick", true).await?;
    shot(
        s,
        d,
        n,
        "flag-off-still-focused",
        "pick",
        json!({"off":off,"observed":focused}),
        st,
        cap,
    )
    .await?;

    let on = flags(&mut s.write(), Some("pick"));
    settle(s, d).await?;
    let before = widget(&s.peek(), "pick");
    let navigation = existing::navigation::revisit(s, d).await?;
    settle(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    let after = widget(&s.peek(), "pick");
    if before["instance"] == after["instance"] {
        return Err("New Date visit reused focus history".into());
    }
    shot(
        s,
        d,
        n,
        "return-fresh",
        "pick",
        json!({"on":on,"navigation":navigation,"before":before,"after":after,"observed":focused}),
        st,
        cap,
    )
    .await?;

    let source = flags(&mut s.write(), Some("required"));
    settle(s, d).await?;
    let focused = active(d, "required", true).await?;
    selected_all(&focused)?;
    if focused["target"]["value"] != "" {
        return Err("Empty required Date gained a value from focus".into());
    }
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "empty-focus",
        "required",
        json!({"source":source,"observed":focused,"silence":silence}),
        st,
        cap,
    )
    .await
}
