use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_channel"] = json!(GAMMA)
    });
    shot(s, d, n, "pristine-initial-changed", source, st, cap).await?;
    let source = refresh(
        &mut s.write(),
        |r| {
            r["state"] = json!({"values":{"pick":{"value":{"type":"channels_select","selected_channel":BETA}}}})
        },
    );
    shot(s, d, n, "saved-only-ignored", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channel");
    });
    shot(s, d, n, "initial-removed-parent-retained", source, st, cap).await?;
    let choice = choose(s, d, "pick", BETA).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "callback-current-metadata", choice, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["response_url_enabled"] = json!(false)
    });
    shot(s, d, n, "flag-only-retained", source, st, cap).await?;
    let pressed = click(s, d).await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Channel flag Button reply not held".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-retained-flag",
        json!({"click":pressed}),
        st,
        cap,
    )
    .await?;
    let choice = choose(s, d, "pick", GAMMA).await?;
    counts(s, 2, 2, 1, 1).await?;
    shot(s, d, n, "callback-current-flag", choice, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_channel"] = json!(BETA)
    });
    shot(s, d, n, "draft-initial-changed", source, st, cap).await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-parent-seeded", navigation, st, cap).await?;
    let cleared = clear(s, d, "pick", GAMMA).await?;
    counts(s, 3, 3, 1, 1).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_channel"] = json!(ALPHA);
        r["state"] = json!({"values":{"pick":{"value":{"type":"channels_select","selected_channel":GAMMA}}}});
    });
    shot(
        s,
        d,
        n,
        "explicit-null-parent-retained",
        json!({"clear":cleared,"refresh":source}),
        st,
        cap,
    )
    .await?;
    let navigation = revisit(s, d).await?;
    shot(
        s,
        d,
        n,
        "revisit-null-initial-fallback",
        navigation,
        st,
        cap,
    )
    .await?;
    let pressed = click(s, d).await?;
    counts(s, 3, 3, 2, 1).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Channel null Button reply not held".into());
    }
    counts(s, 3, 3, 2, 2).await?;
    shot(
        s,
        d,
        n,
        "button-null-parent",
        json!({"click":pressed}),
        st,
        cap,
    )
    .await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-saved-seed",
        json!({"reset":reset,"silence":quiet,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
