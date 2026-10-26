use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        let mut initial = installed();
        initial["description"]["text"] = json!("Initial metadata changed");
        r["blocks"][0]["element"]["initial_option"] = initial;
    });
    shot(s, d, n, "initial-descriptor-only", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_option"] = beta()
    });
    shot(s, d, n, "pristine-initial-changed", source, st, cap).await?;
    let source = refresh(
        &mut s.write(),
        |r| r["state"] = json!({"values":{"pick":{"value":{"type":"external_select","selected_option":alpha()}}}}),
    );
    shot(s, d, n, "saved-only-ignored", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    shot(s, d, n, "initial-removed", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        let mut initial = installed();
        initial["description"]["text"] = json!("Changed initial details");
        r["blocks"][0]["element"]["initial_option"] = initial;
        r["blocks"][0]["element"]["placeholder"]["text"] = json!("Find current release");
    });
    shot(s, d, n, "descriptor-prop-changed", source, st, cap).await?;
    let chosen = choose(s, d, "pick", 0, alpha()).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "callback-current-metadata", chosen, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_option"] = beta()
    });
    shot(s, d, n, "dirty-display-only", source, st, cap).await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-projected-descriptor", navigation, st, cap).await?;
    let cleared = clear(s, d, "pick").await?;
    counts(s, 2, 2, 0, 0).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_option"] = installed();
        r["state"] = json!({"values":{"pick":{"value":{"type":"external_select","selected_option":beta()}}}});
    });
    shot(
        s,
        d,
        n,
        "null-parent-initial-display",
        json!({"clear":cleared,"source":source}),
        st,
        cap,
    )
    .await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-null-fallback", navigation, st, cap).await?;
    let pressed = click(s, d).await?;
    counts(s, 2, 2, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("External null-parent Button reply missing".into());
    }
    counts(s, 2, 2, 1, 1).await?;
    shot(s, d, n, "button-retains-null", pressed, st, cap).await?;
    let reset = reset(s, d).await?;
    let silence = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-raw-saved",
        json!({"reset":reset,"silence":silence,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
