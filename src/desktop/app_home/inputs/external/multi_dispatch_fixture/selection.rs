use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let opened = open(s, d, "pick").await?;
    let started = Instant::now();
    let first = toggle(
        s,
        d,
        "pick",
        0,
        json!([installed(), alpha()]),
        json!([installed(), alpha()]),
    )
    .await?;
    let removed = remove(
        s,
        d,
        "pick",
        "installed",
        json!([alpha()]),
        json!([alpha()]),
    )
    .await?;
    let second = toggle(
        s,
        d,
        "pick",
        1,
        json!([alpha(), beta()]),
        json!([alpha(), beta()]),
    )
    .await?;
    let sibling = notes_received(s, "Latest sibling").await?;
    let before = trailing_before(&s.peek(), started, 0)?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s,d,n,"ordered-trailing",json!({"open":opened,"first":first,"remove":removed,"second":second,"sibling":sibling,"beforeDelivery":before}),st,cap).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::EmptyValue, false);
    let chosen = toggle(
        s,
        d,
        "pick",
        0,
        json!([alpha(), beta(), empty_value()]),
        json!([alpha(), beta()]),
    )
    .await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "overmax-retains-parent",
        json!({"choice":chosen,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let removed = remove(
        s,
        d,
        "pick",
        "alpha,desktop",
        json!([beta(), empty_value()]),
        json!([beta(), empty_value()]),
    )
    .await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "corrected-order", removed, st, cap).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Options, false);
    let opened = open(s, d, "required").await?;
    let started = Instant::now();
    let chosen = toggle(s, d, "required", 0, json!([alpha()]), json!([alpha()])).await?;
    let removed = remove(s, d, "required", "alpha,desktop", json!([]), json!([])).await?;
    let before = trailing_before(&s.peek(), started, 2)?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "required-empty-keeps-queued",
        json!({"open":opened,"choice":chosen,"remove":removed,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, false);
    let opened = open(s, d, "pick").await?;
    let started = Instant::now();
    let removed = remove(s, d, "pick", "", json!([beta()]), json!([beta()])).await?;
    let chosen = toggle(
        s,
        d,
        "pick",
        0,
        json!([beta(), gamma()]),
        json!([beta(), gamma()]),
    )
    .await?;
    let before = trailing_before(&s.peek(), started, 3)?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "same-value-new-descriptor",
        json!({"open":opened,"remove":removed,"choice":chosen,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    close(d, "pick").await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Options, false);
    let opened = open(s, d, "pick").await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, true);
    let expected = suggestion_count(&s.peek()) + 1;
    let held = query(s, d, "pick", "refresh-descriptors", expected, true).await?;
    shot(
        s,
        d,
        n,
        "suggestions-held",
        json!({"open":opened,"query":held}),
        st,
        cap,
    )
    .await?;
    let chosen = toggle(s, d, "pick", 0, json!([beta()]), json!([beta()])).await?;
    counts(s, 5, 5, 0, 0).await?;
    for _ in 0..150 {
        if suggestions::pending_closed(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let released = suggestions::release(&mut s.write());
    if released["delivered"] != false {
        return Err("Multi choice failed to retire held suggestion".into());
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "held-options-selectable",
        json!({"choice":chosen,"release":released,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, false);
    let expected = suggestion_count(&s.peek()) + 1;
    let queried = query(s, d, "pick", "replacement-ready", expected, false).await?;
    shot(s, d, n, "replacement-results", queried, st, cap).await?;
    let chosen = toggle(
        s,
        d,
        "pick",
        0,
        json!([beta(), gamma()]),
        json!([beta(), gamma()]),
    )
    .await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(s, d, n, "replacement-choice", chosen, st, cap).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::EmptyValue, false);
    let chosen = toggle(
        s,
        d,
        "required",
        0,
        json!([empty_value()]),
        json!([empty_value()]),
    )
    .await?;
    counts(s, 7, 7, 0, 0).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Options, false);
    let quiet = choose(s, d, "quiet", 1, beta()).await?;
    let pressed = click(s, d).await?;
    counts(s, 7, 7, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Multi external Button reply missing".into());
    }
    counts(s, 7, 7, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "empty-value-button",
        json!({"required":chosen,"quiet":quiet,"click":pressed}),
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
        "reset",
        json!({"reset":reset,"silence":quiet}),
        st,
        cap,
    )
    .await
}
