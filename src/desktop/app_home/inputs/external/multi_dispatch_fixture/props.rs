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
        r["blocks"][0]["element"]["initial_options"] = json!([initial]);
    });
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "initial-descriptor-emits", source, st, cap).await?;
    let source = refresh(&mut s.write(), |_r| {});
    let before = snapshot(&s.peek());
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "deep-equal-initial-silent",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(
        &mut s.write(),
        |r| r["state"] = json!({"values":{"pick":{"value":{"type":"multi_external_select","selected_options":[beta()]}}}}),
    );
    let before = snapshot(&s.peek());
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "saved-only-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
    });
    let before = snapshot(&s.peek());
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "initial-removed-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([])
    });
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "initial-empty-emits", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([installed(), alpha(), beta()])
    });
    let before = snapshot(&s.peek());
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "initial-overmax-local",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["max_selected_items"] = json!(3)
    });
    let before = snapshot(&s.peek());
    let quiet = no_callback(s, &before).await?;
    shot(
        s,
        d,
        n,
        "maximum-only-retains-invalid",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let removed = remove(
        s,
        d,
        "pick",
        "installed",
        json!([alpha(), beta()]),
        json!([alpha(), beta()]),
    )
    .await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s, d, n, "corrected-valid-array", removed, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([gamma(), beta()]);
        r["blocks"][0]["element"]["placeholder"]["text"] = json!("Find current releases");
    });
    counts(s, 4, 4, 0, 0).await?;
    shot(s, d, n, "changed-descriptor-prop-emits", source, st, cap).await?;
    let navigation = revisit(s, d).await?;
    shot(
        s,
        d,
        n,
        "revisit-projected-descriptors",
        navigation,
        st,
        cap,
    )
    .await?;
    let started = Instant::now();
    let first = remove(
        s,
        d,
        "pick",
        "alpha,desktop",
        json!([projected(beta())]),
        json!([projected(beta())]),
    )
    .await?;
    let second = remove(s, d, "pick", "beta", json!([]), json!([])).await?;
    let before = trailing_before(&s.peek(), started, 4)?;
    counts(s, 5, 5, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "explicit-empty-parent",
        json!({"first":first,"second":second,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-explicit-empty", navigation, st, cap).await?;
    let pressed = click(s, d).await?;
    counts(s, 5, 5, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Multi empty Button reply missing".into());
    }
    counts(s, 5, 5, 1, 1).await?;
    shot(s, d, n, "button-retains-empty", pressed, st, cap).await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-raw-saved",
        json!({"reset":reset,"silence":quiet,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
